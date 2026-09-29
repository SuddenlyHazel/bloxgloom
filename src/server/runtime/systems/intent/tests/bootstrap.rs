use super::*;
use crate::server::runtime::owner_codec::owner_state_key;
use crate::server::runtime::systems::MAX_OWNER_VALUES_PER_SYSTEM;

#[test]
fn durable_intent_bootstrap_destination_conflict_retries_one_atomic_record() {
    let path = save();
    let mut state = open_with_bootstrap(&path, 1, true);
    load(&mut state, 8);
    let destination = owner_state_key(&system_id(), owner(9));
    // A destination-key reservation, not the producer or mailbox key, must
    // reject the entire proposal before WAL admission. This models another
    // participant creating the same owner while this worker saw absence.
    state.durability.reserved.insert(destination.clone());
    assert_eq!(
        stage(&mut state, 1).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(value(&state, 8), 0);
    assert!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(9))
            .is_none()
    );
    assert!(pending(&state, 9).is_empty());
    assert!(state.system_runtime.durable_wakes.intents.staged.is_empty());
    state.durability.reserved.remove(&destination);
    let wave = stage(&mut state, 2).unwrap().0.unwrap();
    // Confirm WAL, then lose the coordinator's receipt: recovery must create
    // source state, revision-zero destination and inbox together, exactly once.
    let (tx, rx) = std::sync::mpsc::channel();
    let real = std::mem::replace(&mut state.durability.pending[0].receiver, rx);
    real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    drop(tx);
    assert!(complete_barrier(&mut state, wave.barrier()).is_err());
    assert_eq!(value(&state, 8), 0);
    assert!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(9))
            .is_none()
    );
    drop(state);

    let journal = crate::server::journal::Journal::open(path.join("server.wal")).unwrap();
    let record = journal
        .records()
        .iter()
        .find(|record| {
            record
                .changes
                .iter()
                .any(|change| change.key == destination)
        })
        .unwrap();
    let source = owner_state_key(&system_id(), owner(8));
    let mailbox = codec::key(&system_id(), owner(9));
    assert!(
        record
            .changes
            .iter()
            .any(|change| change.key == source && !change.before.is_empty())
    );
    assert!(
        record
            .changes
            .iter()
            .any(|change| change.key == destination && change.before.is_empty())
    );
    assert!(
        record
            .changes
            .iter()
            .any(|change| change.key == mailbox && change.before.is_empty())
    );
    drop(journal);
    let state = open_with_bootstrap(&path, 1, true);
    assert_eq!(value(&state, 8), 1);
    assert_eq!(value(&state, 9), 0);
    assert_eq!(
        pending(&state, 9),
        [IntentDelivery {
            id: api::IntentId {
                source: api::Owner::Chunk([8, 6, 0]),
                revision: 1,
                ordinal: 0
            },
            produced_tick: 2,
            payload: vec![1],
        }]
    );
    assert!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(10))
            .is_none()
    );
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn durable_intent_bootstrap_capacity_and_cancellation_do_not_leave_orphans() {
    // A two-destination batch cannot spend the single remaining slot. A
    // single-destination batch can retry after *late* WAL admission rejection.
    for fanout in [2, 1] {
        let path = save();
        let mut state = open_with_bootstrap(&path, fanout, true);
        for x in 100..100 + (MAX_OWNER_VALUES_PER_SYSTEM - 2) as i32 {
            state
                .system_runtime
                .insert_owner(system_id(), owner(x), vec![0u8])
                .unwrap();
        }
        load(&mut state, 8);
        if fanout == 1 {
            assert_eq!(
                stage_with_in_flight(
                    &mut state,
                    1,
                    &[vec![owner_state_key(&system_id(), owner(9))]]
                )
                .unwrap_err()
                .kind(),
                ErrorKind::WouldBlock
            );
        }
        state.durability.rotation_requested = true;
        assert_eq!(
            stage(&mut state, 2).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        state.durability.rotation_requested = false;
        assert_eq!(value(&state, 8), 0);
        for x in [9, 10] {
            assert!(
                state
                    .system_runtime
                    .owner_snapshot(&system_id(), owner(x))
                    .is_none()
            );
            assert!(pending(&state, x).is_empty());
        }
        assert!(state.system_runtime.durable_wakes.intents.staged.is_empty());
        if fanout == 1 {
            let wave = stage(&mut state, 3).unwrap().0.unwrap();
            complete_barrier(&mut state, wave.barrier()).unwrap();
            assert_eq!(value(&state, 8), 1);
            assert_eq!(value(&state, 9), 0);
            assert_eq!(pending(&state, 9).len(), 1);
            assert_eq!(
                state.system_runtime.durable.cell_count(),
                MAX_OWNER_VALUES_PER_SYSTEM
            );
        } else {
            assert_eq!(
                stage(&mut state, 3).unwrap_err().kind(),
                ErrorKind::WouldBlock
            );
        }
        // Synthetic filler cells are only a capacity fixture, not save data.
        drop(state);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn durable_intent_bootstrap_existing_destination_wins_and_opt_in_is_required() {
    let path = save();
    let mut state = open_with_bootstrap(&path, 1, true);
    state
        .system_runtime
        .insert_owner(system_id(), owner(9), vec![7u8])
        .unwrap();
    load(&mut state, 8);
    let wave = stage(&mut state, 1).unwrap().0.unwrap();
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(
        value(&state, 9),
        7,
        "template cannot overwrite an existing identity"
    );
    assert_eq!(pending(&state, 9).len(), 1);
    drop(state);
    std::fs::remove_dir_all(path).unwrap();

    // No-template behavior remains valid for old seeded systems; sending to an
    // absent destination does not silently give it arbitrary/empty state.
    let mut declaration = Ignitions {
        fanout: 1,
        bootstrap: true,
        observed: None,
    }
    .definition();
    declaration.behavior = Arc::new(Ignitions {
        fanout: 1,
        bootstrap: false,
        observed: None,
    });
    let old = declaration.fingerprint_bytes();
    let path = save();
    let mut catalog = crate::content::Catalog::builtins();
    catalog.register_owner_system(declaration.clone()).unwrap();
    let mut state =
        server_state_with_startup(7, path.clone(), 2, ServerStartup::new(Arc::new(catalog)))
            .unwrap();
    load(&mut state, 8);
    assert_eq!(
        stage(&mut state, 1).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(value(&state, 8), 0);
    assert!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(9))
            .is_none()
    );
    assert!(pending(&state, 9).is_empty());
    assert!(state.system_runtime.durable_wakes.intents.staged.is_empty());
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
    declaration.behavior = Arc::new(Ignitions {
        fanout: 1,
        bootstrap: true,
        observed: None,
    });
    let enabled = declaration.fingerprint_bytes();
    assert_ne!(old, enabled);
    assert!(enabled.starts_with(&old));
}

#[test]
fn durable_intent_bootstrap_prepared_waves_reserve_capacity_across_systems() {
    let path = save();
    let mut catalog = crate::content::Catalog::builtins();
    let mut declaration = Ignitions {
        fanout: 1,
        bootstrap: true,
        observed: None,
    }
    .definition();
    catalog.register_owner_system(declaration.clone()).unwrap();
    declaration.key = "test:other_intents".into();
    let other = SystemId::new(&declaration.key).unwrap();
    catalog.register_owner_system(declaration).unwrap();
    let mut state =
        server_state_with_startup(7, path.clone(), 2, ServerStartup::new(Arc::new(catalog)))
            .unwrap();
    for x in 100..100 + (MAX_OWNER_VALUES_PER_SYSTEM - 3) as i32 {
        state
            .system_runtime
            .insert_owner(system_id(), owner(x), vec![0u8])
            .unwrap();
    }
    load(&mut state, 8);
    let prepare = |state: &mut State, id: &SystemId, wave: u16| {
        let registered = state.phase_plan.system(id).unwrap().clone();
        state.system_runtime.prepare_registered_wave(
            &registered,
            TickId::new(1),
            wave,
            &state.effect_kinds,
            RegisteredWorldInputs {
                world: Some(&mut state.world),
                entities: Some(&state.entities),
                lifecycles: Some(&state.lifecycles),
                players: &[],
                seed: state.seed,
                missing: &mut vec![],
            },
        )
    };
    let first = prepare(&mut state, &system_id(), 0).unwrap().unwrap();
    let error = match prepare(&mut state, &other, 1) {
        Err(error) => error,
        Ok(_) => panic!("a disjoint wave spent reserved owner capacity"),
    };
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert!(
        state
            .system_runtime
            .owner_snapshot(&other, owner(9))
            .is_none()
    );
    state.durability.rotation_requested = true;
    assert_eq!(
        state
            .system_runtime
            .stage_owner_wave(first.durables, &mut state.durability)
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock
    );
    state.durability.rotation_requested = false;
    let second = prepare(&mut state, &other, 2).unwrap().unwrap();
    let staged = state
        .system_runtime
        .stage_owner_wave(second.durables, &mut state.durability)
        .unwrap();
    assert!(
        state
            .system_runtime
            .owner_snapshot(&other, owner(9))
            .is_none()
    );
    complete_barrier(&mut state, staged.barrier).unwrap();
    assert_eq!(
        state
            .system_runtime
            .owner_snapshot(&other, owner(9))
            .unwrap()
            .0,
        0
    );
    assert!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(9))
            .is_none()
    );
    assert_eq!(value(&state, 8), 0);
    assert_eq!(
        state.system_runtime.durable.cell_count(),
        MAX_OWNER_VALUES_PER_SYSTEM
    );
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

struct BootstrapContract {
    data: Vec<u8>,
    accepts: bool,
}
impl api::Behavior for BootstrapContract {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data.len() == 1 {
            Ok(())
        } else {
            Err(RegistrationError("one byte required".into()))
        }
    }
    fn plan(&self, _: &api::Context<'_>) -> Result<api::Plan, RegistrationError> {
        Err(RegistrationError("validation fixture".into()))
    }
    fn accepts_intents(&self) -> bool {
        self.accepts
    }
    fn intent_bootstrap(&self) -> Option<&[u8]> {
        Some(&self.data)
    }
}

#[test]
fn durable_intent_bootstrap_template_is_validated_bounded_and_fingerprinted() {
    let definition = Ignitions {
        fanout: 1,
        bootstrap: true,
        observed: None,
    }
    .definition();
    for (data, accepts) in [(vec![], true), (vec![0, 1], true), (vec![0], false)] {
        let mut invalid = definition.clone();
        invalid.behavior = Arc::new(BootstrapContract { data, accepts });
        assert!(
            crate::content::Catalog::builtins()
                .register_owner_system(invalid)
                .is_err()
        );
    }
    let mut changed = definition.clone();
    changed.behavior = Arc::new(BootstrapContract {
        data: vec![1],
        accepts: true,
    });
    assert!(changed.validate().is_ok());
    assert_ne!(definition.fingerprint_bytes(), changed.fingerprint_bytes());
}

struct PairedProducers;
impl api::Behavior for PairedProducers {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        BootstrapContract {
            data: vec![],
            accepts: true,
        }
        .validate(data)
    }
    fn accepts_intents(&self) -> bool {
        true
    }
    fn intent_bootstrap(&self) -> Option<&[u8]> {
        Some(&[0])
    }
    fn plan(&self, _: &api::Context<'_>) -> Result<api::Plan, RegistrationError> {
        Err(RegistrationError(
            "intent-aware entry point required".into(),
        ))
    }
    fn plan_with_intents(
        &self,
        c: &api::Context<'_>,
        inbox: &[IntentDelivery],
        outbox: &mut api::IntentOutbox,
    ) -> Result<api::Plan, RegistrationError> {
        let mut data = c.data[0] + inbox.len() as u8;
        if matches!(c.owner, api::Owner::Chunk([8 | 10, 6, 0])) && data == 0 {
            outbox.send(api::Owner::Chunk([9, 6, 0]), &[1])?;
            if c.owner == api::Owner::Chunk([8, 6, 0]) {
                outbox.send(api::Owner::Chunk([9, 6, 0]), &[1])?;
            }
            data = 1;
        }
        Ok(api::Plan {
            data: vec![data],
            next_tick: c.tick + 10_000,
            wakes: vec![],
            edits: vec![],
            drops: vec![],
        })
    }
}

#[test]
fn durable_intent_bootstrap_combines_producers_and_ordinals_without_duplicate_creation() {
    let path = save();
    let mut definition = Ignitions {
        fanout: 1,
        bootstrap: true,
        observed: None,
    }
    .definition();
    definition.max_jobs_per_tick = 2;
    definition.seeds.push(api::Seed {
        owner: api::Owner::Chunk([10, 6, 0]),
        data: vec![0],
    });
    definition.behavior = Arc::new(PairedProducers);
    let mut catalog = crate::content::Catalog::builtins();
    catalog.register_owner_system(definition).unwrap();
    let catalog = Arc::new(catalog);
    let open = || {
        server_state_with_startup(7, path.clone(), 2, ServerStartup::new(Arc::clone(&catalog)))
            .unwrap()
    };
    let mut state = open();
    load(&mut state, 8);
    load(&mut state, 10);
    let wave = stage(&mut state, 1).unwrap().0.unwrap();
    assert!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(9))
            .is_none()
    );
    assert_eq!(
        stage(&mut state, 2).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(state.system_runtime.durable.cell_count(), 3);
    assert_eq!(value(&state, 8), 1);
    assert_eq!(value(&state, 10), 1);
    let messages = pending(&state, 9);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id)
            .collect::<Vec<_>>(),
        [
            api::IntentId {
                source: api::Owner::Chunk([8, 6, 0]),
                revision: 1,
                ordinal: 0
            },
            api::IntentId {
                source: api::Owner::Chunk([8, 6, 0]),
                revision: 1,
                ordinal: 1
            },
            api::IntentId {
                source: api::Owner::Chunk([10, 6, 0]),
                revision: 1,
                ordinal: 0
            },
        ]
    );
    drop(state);
    let mut state = open();
    assert_eq!(pending(&state, 9), messages);
    assert_eq!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(9))
            .unwrap()
            .0,
        0
    );
    let (wave, missing) = stage(&mut state, 3).unwrap();
    assert!(wave.is_none());
    assert_eq!(missing, [chunk(9)]);
    load(&mut state, 9);
    let wave = stage(&mut state, 5).unwrap().0.unwrap();
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(value(&state, 9), 3);
    assert!(pending(&state, 9).is_empty());
    drop(state);
    let state = open();
    assert_eq!(value(&state, 9), 3);
    assert!(pending(&state, 9).is_empty());
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}
