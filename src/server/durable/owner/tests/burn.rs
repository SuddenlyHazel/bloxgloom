//! Burn is an owner edit capability, not a second production fire scheduler.
use super::*;
use crate::world::{GRASS, RED_FLOWER, WOOD};

struct Burn {
    after: &'static str,
}

impl system::Behavior for Burn {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        system::Behavior::validate(&Behavior, data)
    }

    fn edit_cause(&self) -> system::EditCause {
        system::EditCause::Burn
    }

    fn accepts_intents(&self) -> bool {
        true
    }

    fn intent_bootstrap(&self) -> Option<&[u8]> {
        Some(&[0])
    }

    fn plan(&self, _: &system::Context<'_>) -> Result<system::Plan, RegistrationError> {
        Err(RegistrationError(
            "intent-aware entry point required".into(),
        ))
    }

    fn plan_with_intents(
        &self,
        c: &system::Context<'_>,
        inbox: &[system::IntentDelivery],
        outbox: &mut system::IntentOutbox,
    ) -> Result<system::Plan, RegistrationError> {
        let mut edits = vec![];
        if c.owner == system::Owner::Chunk([8, 6, 0]) && c.data == [0] {
            for cell in [CELL, [138, 100, 8]] {
                edits.push(system::BlockEdit {
                    cell,
                    before: c
                        .block(cell)
                        .map_err(|e| RegistrationError(e.to_string()))?
                        .state,
                    after: self.after.into(),
                });
            }
            outbox.send(system::Owner::Chunk([9, 6, 0]), &[1])?;
        }
        for message in inbox {
            if message.payload != [1] {
                return Err(RegistrationError("unexpected burn intent".into()));
            }
        }
        Ok(system::Plan {
            data: vec![c.data[0] + 1],
            next_tick: c.tick + 100,
            wakes: vec![],
            edits,
            drops: vec![],
            entity_spawns: vec![],
        })
    }
}

struct BurnGrass;
impl api::Handler for BurnGrass {
    fn handle(&self, c: &mut api::Context<'_>, event: &api::Event) -> Result<(), api::Error> {
        let api::Event::BlockRemoved {
            cell,
            cause: api::RemovalCause::Burn,
            ..
        } = event
        else {
            return Err(api::Error::Invalid(
                "expected public owner burn cause".into(),
            ));
        };
        c.spawn_drop(cell.map(|n| n as f32 + 0.5), "bloxgloom:stick", 1, 250)
    }
}

fn definition(after: &'static str) -> system::System {
    system::System {
        key: KEY.into(),
        schema: 1,
        partition: system::Partition::Chunk,
        max_state_bytes: 1,
        max_jobs_per_tick: 1,
        read_radius_chunks: Some(0),
        after: vec![],
        seeds: vec![system::Seed {
            owner: system::Owner::Chunk([8, 6, 0]),
            data: vec![0],
        }],
        behavior: Arc::new(Burn { after }),
    }
}

impl Extension for Burn {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.owner_system(definition(self.after))?;
        registrar.gameplay_handler(api::HandlerRegistration {
            key: "test:owner_burn_grass".into(),
            version: 1,
            event: api::EventKind::BlockRemoved,
            target: Some("bloxgloom:grass".into()),
            handler: Arc::new(BurnGrass),
        })
    }
}

fn burn_startup(after: &'static str) -> ServerStartup {
    ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&Burn { after })
        .unwrap()
}

fn plant_burn(state: &mut State) {
    state.world.get_block(CELL[0], CELL[1], CELL[2]).unwrap();
    let mut action = empty_action();
    action.world_edits = state
        .world
        .prepare_edits(&[
            (136, 100, 8, GRASS),
            (136, 101, 8, RED_FLOWER),
            (138, 100, 8, WOOD),
        ])
        .unwrap();
    assert!(stage_action(state, &action).unwrap());
    complete_barrier(state, CommitBarrier::AllStaged).unwrap();
    state.durability.publish_queue.clear();
}

fn destination(state: &State) -> Option<(u64, Vec<u8>)> {
    state.system_runtime.owner_value(
        &SystemId::new(KEY).unwrap(),
        OwnerKey::Chunk(ChunkKey { x: 9, y: 6, z: 0 }),
    )
}

fn assert_uncommitted(state: &State) {
    assert_eq!(owner(state), (0, vec![0]));
    assert_eq!(destination(state), None);
    assert_eq!(state.world.cached_block(136, 100, 8), Some(GRASS));
    assert_eq!(state.world.cached_block(136, 101, 8), Some(RED_FLOWER));
    assert_eq!(state.world.cached_block(138, 100, 8), Some(WOOD));
    assert_eq!(drop_count(state), 0);
    assert!(state.durability.publish_queue.is_empty());
}

fn assert_burned(state: &mut State) {
    assert_eq!(owner(state), (1, vec![1]));
    for [x, y, z] in [CELL, [136, 101, 8], [138, 100, 8]] {
        assert_eq!(state.world.get_block(x, y, z).unwrap(), AIR);
    }
    let drops = crate::server::drops::nearby(&state.entities, POSITION);
    let count = |item| {
        drops
            .iter()
            .filter(|drop| drop.item == item)
            .map(|drop| drop.count)
            .sum::<u16>()
    };
    assert_eq!(count(crate::items::STICK), 1, "targeted burn handler runs");
    assert_eq!(
        count(crate::items::ItemId::new(RED_FLOWER.get())),
        1,
        "support loss keeps its own cause and loot"
    );
    assert_eq!(
        drops.iter().map(|drop| drop.count).sum::<u16>(),
        2,
        "burn does not award ordinary wood harvest"
    );
}

#[test]
fn public_owner_burn_retries_and_recovers_loot_support_and_intent_in_one_record() {
    for lose_receipt in [false, true] {
        let path = save();
        let open = || {
            server_state_with_startup(7, path.clone(), 2, burn_startup("bloxgloom:air")).unwrap()
        };
        let mut state = open();
        assert!(stage(&mut state, 1).unwrap().is_none());
        plant_burn(&mut state);
        // Late WAL admission rejection must not consume owner progress or
        // create inboxes, even after gameplay and bootstrap preparation.
        state.durability.rotation_requested = true;
        assert_eq!(
            stage(&mut state, 2).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        state.durability.rotation_requested = false;
        assert_uncommitted(&state);
        let wave = stage(&mut state, 3).unwrap().unwrap();
        assert_uncommitted(&state);
        if lose_receipt {
            let (sender, receiver) = std::sync::mpsc::channel();
            let real = std::mem::replace(&mut state.durability.pending[0].receiver, receiver);
            real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
            drop(sender);
            assert!(complete_barrier(&mut state, wave.barrier()).is_err());
            assert_uncommitted(&state);
        } else {
            complete_barrier(&mut state, wave.barrier()).unwrap();
            assert_burned(&mut state);
            assert_eq!(destination(&state), Some((0, vec![0])));
        }
        drop(state);
        let mut state = open();
        assert_burned(&mut state);
        assert_eq!(destination(&state), Some((0, vec![0])));
        // The absent destination was created atomically, but cannot consume
        // until its authoritative terrain is available on a later tick.
        assert!(stage(&mut state, 5).unwrap().is_none());
        state.world.get_block(152, 100, 8).unwrap();
        let wave = stage(&mut state, 6).unwrap().unwrap();
        assert_eq!(destination(&state), Some((0, vec![0])));
        complete_barrier(&mut state, wave.barrier()).unwrap();
        assert_eq!(destination(&state), Some((1, vec![1])));
        drop(state);
        let mut state = open();
        assert_burned(&mut state);
        assert_eq!(destination(&state), Some((1, vec![1])));
        assert!(
            stage(&mut state, 8).unwrap().is_none(),
            "ack must not replay"
        );
        drop(state);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn public_owner_burn_rejects_replacement_without_committing_any_participant() {
    let path = save();
    let mut state =
        server_state_with_startup(7, path.clone(), 1, burn_startup("bloxgloom:sand")).unwrap();
    plant_burn(&mut state);
    let next_id = state.durability.next_id;
    assert!(stage(&mut state, 2).is_err());
    assert_uncommitted(&state);
    assert_eq!(state.durability.next_id, next_id);
    assert!(state.durability.pending.is_empty());
    assert!(state.durability.reserved.is_empty());
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn public_owner_burn_declaration_requires_terrain_and_changes_only_opt_in_fingerprint() {
    struct BurnOnly;
    impl system::Behavior for BurnOnly {
        fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
            system::Behavior::validate(&Behavior, data)
        }
        fn plan(&self, c: &system::Context<'_>) -> Result<system::Plan, RegistrationError> {
            system::Behavior::plan(&Behavior, c)
        }
        fn edit_cause(&self) -> system::EditCause {
            system::EditCause::Burn
        }
    }
    let mut burn = definition("bloxgloom:air");
    burn.behavior = Arc::new(BurnOnly);
    let mut ordinary = burn.clone();
    ordinary.behavior = Arc::new(Behavior);
    assert!(
        burn.fingerprint_bytes()
            .starts_with(&ordinary.fingerprint_bytes())
    );
    assert_ne!(burn.fingerprint_bytes(), ordinary.fingerprint_bytes());
    burn.read_radius_chunks = None;
    assert!(burn.validate().is_err());
}
