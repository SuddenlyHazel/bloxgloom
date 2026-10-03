use super::*;

#[test]
fn pickup_routing_respects_components_cap_and_failed_destination() {
    let source = Stack {
        item: "test:gem".into(),
        count: 12,
        components: Some(Components {
            version: 1,
            bytes: vec![0, 255],
        }),
    };
    let partial = Slot {
        stack: Some(Stack {
            count: 125,
            ..source.clone()
        }),
        insert: true,
        extract: true,
    };
    let different = Slot {
        stack: Some(Stack {
            components: None,
            ..source.clone()
        }),
        insert: true,
        extract: true,
    };
    let empty = Slot {
        stack: None,
        insert: true,
        extract: true,
    };
    let mut routing = PickupTransfer::new(&source, 20);
    assert_eq!(routing.offer(&different), 0);
    assert_eq!(routing.offer(&partial), 3);
    // Rejected by a host-side filter: later slots can still take all 12.
    assert_eq!(routing.offer(&empty), 12);
    routing.credited(12).unwrap();
    assert_eq!(routing.remaining(), 0);
    assert_eq!(routing.offer(&partial), 0);
    assert!(routing.credited(1).is_err());

    let mut success = PickupTransfer::new(&source, 12);
    success.credited(success.offer(&partial)).unwrap();
    assert_eq!(success.offer(&empty), 9);
    success.credited(9).unwrap();
    assert_eq!(success.remaining(), 0);
}

#[test]
fn non_player_snapshot_has_no_authored_player_position() {
    let mut world = World { reads: 0 };
    let context = Context::new(&mut world, 8);
    assert_eq!(context.player(), None);
    assert_eq!(context.player_position(), None);
}

struct World {
    reads: usize,
}
impl Snapshot for World {
    fn motion_contact(
        &mut self,
        id: u64,
        owner: &str,
    ) -> Result<Option<crate::motion::MotionContact>, Error> {
        if owner != "test" {
            return Err(Error::Invalid("foreign motion".into()));
        }
        self.reads += 1;
        Ok((id == 1).then_some(crate::motion::MotionContact {
            motion_revision: 17,
            tick: 21,
            target: crate::motion::Target::Terrain {
                cell: [0, 0, 0],
                state: "test:stone".into(),
            },
            normal: [0.0, 1.0, 0.0],
        }))
    }

    fn motion(&mut self, id: u64, owner: &str) -> Result<Option<crate::motion::Motion>, Error> {
        if owner != "test" {
            return Err(Error::Invalid("foreign motion".into()));
        }
        Ok((id == 1).then_some(crate::motion::Motion {
            position: [0.0; 3],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
            angular_velocity: [0.0; 3],
            revision: 17,
            grounded: false,
        }))
    }
    fn validate_moving_spawn(&mut self, owner: &str, spawn: &MovingSpawn) -> Result<(), Error> {
        if owner != "test" || spawn.key != "test:bolt" {
            return Err(Error::Invalid("foreign spawn".into()));
        }
        Ok(())
    }
    fn validate_motion_change(
        &self,
        _id: u64,
        owner: &str,
        _motion: &crate::motion::Motion,
    ) -> Result<(), Error> {
        if owner != "test" {
            return Err(Error::Invalid("foreign control".into()));
        }
        Ok(())
    }
    fn seed(&self) -> u64 {
        23
    }
    fn tick(&self) -> u64 {
        0
    }
    fn project_entity_state(&self, _: u64, state: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(state.to_vec())
    }
    fn nearby_entities(&mut self, _: [f32; 3], _: f32) -> Result<Vec<Entity>, Error> {
        Ok(vec![])
    }
    fn entity_state(&mut self, id: u64, owner: &str) -> Result<Option<Vec<u8>>, Error> {
        if id != 1 {
            return Ok(None);
        }
        if owner != "test" {
            return Err(Error::Invalid("entity is not owned by handler".into()));
        }
        Ok(Some(vec![1]))
    }
    fn validate_entity_state(&self, key: &str, _: &str, _: &[u8]) -> Result<(), Error> {
        Err(Error::UnknownContent(key.into()))
    }
    fn inventory_accepts(&self, _: InventoryId, _: usize, _: &Stack) -> bool {
        true
    }
    fn entity(&mut self, _: u64) -> Result<Option<Entity>, Error> {
        Ok(None)
    }
    fn anchored_entity_at(&mut self, _: Cell) -> Result<Option<u64>, Error> {
        Ok(None)
    }
    fn player(&self) -> Option<u128> {
        None
    }
    fn inventory(&mut self, owner: InventoryId) -> Result<Vec<Slot>, Error> {
        Err(Error::InventoryUnavailable(owner))
    }
    fn validate_stack(&self, stack: &Stack) -> Result<(), Error> {
        if self.item_exists(&stack.item) {
            Ok(())
        } else {
            Err(Error::UnknownContent(stack.item.clone()))
        }
    }
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        self.reads += 1;
        if cell[0] < 0 {
            Err(Error::Unavailable(cell))
        } else {
            self.state("test:stone")
        }
    }
    fn state(&self, key: &str) -> Result<Block, Error> {
        if !matches!(key, "test:stone" | "test:air") {
            return Err(Error::UnknownContent(key.into()));
        }
        Ok(Block {
            state: key.into(),
            block_type: key.into(),
            primary_item: Some("test:stone".into()),
            plant: false,
            supports_plant: false,
        })
    }
    fn item_exists(&self, key: &str) -> bool {
        key == "test:stone"
    }
}

#[test]
fn handler_random_is_stable_per_seed_cell_and_registration() {
    use std::sync::{Arc, Mutex};
    struct Probe(Arc<Mutex<Vec<u64>>>);
    impl Handler for Probe {
        fn handle(&self, ctx: &mut Context<'_>, _: &Event) -> Result<(), Error> {
            let first = ctx.random([1, 2, 3], 7)?;
            assert_eq!(first, ctx.random([1, 2, 3], 7)?);
            self.0.lock().unwrap().push(first);
            Ok(())
        }
    }
    let observed = Arc::new(Mutex::new(Vec::new()));
    let event = Event::BlockPlaced {
        cell: [1, 2, 3],
        previous: World { reads: 0 }.state("test:air").unwrap(),
        placed: World { reads: 0 }.state("test:stone").unwrap(),
    };
    for key in ["test:first", "test:first", "test:second"] {
        let registration = HandlerRegistration {
            key: key.into(),
            version: 1,
            event: EventKind::BlockPlaced,
            target: None,
            handler: Arc::new(Probe(Arc::clone(&observed))),
        };
        let mut world = World { reads: 0 };
        let mut ctx = Context::new(&mut world, 8);
        ctx.dispatch(&registration, &event).unwrap();
        ctx.finish().unwrap();
    }
    let words = observed.lock().unwrap();
    assert_eq!(words[0], words[1]);
    assert_ne!(words[0], words[2]);
}

#[test]
fn writes_capture_preimages_and_reads_see_coalesced_changes() {
    let mut world = World { reads: 0 };
    let mut ctx = Context::new(&mut world, 8);
    ctx.set_block([0; 3], "test:air").unwrap();
    assert_eq!(ctx.block([0; 3]).unwrap().state, "test:air");
    ctx.set_block([0; 3], "test:stone").unwrap();
    ctx.spawn_drop([0.5; 3], "test:stone", 1, 250).unwrap();
    let plan = ctx.finish().unwrap();
    assert_eq!(plan.blocks.len(), 1);
    assert_eq!(plan.blocks[&[0; 3]], "test:stone");
    assert_eq!(plan.drops.len(), 1);
    assert_eq!(world.reads, 1);
}

#[test]
fn private_entity_overlay_does_not_authorize_the_next_decision_owner() {
    struct Probe(u8);
    impl Handler for Probe {
        fn handle(&self, ctx: &mut Context<'_>, _: &Event) -> Result<(), Error> {
            // Missing state remains readable across namespaces.
            assert_eq!(ctx.entity_state(2)?, None);
            if self.0 == 0 {
                assert_eq!(ctx.entity_state(1)?, Some(vec![1]));
                ctx.set_block([0; 3], "test:air")?;
            } else {
                let error = match self.0 {
                    1 => ctx.entity_state(1).unwrap_err(),
                    2 => ctx.remove_entity(1).unwrap_err(),
                    // Even a no-op update must check the cached state's owner.
                    _ => ctx.update_entity(1, &[1]).unwrap_err(),
                };
                assert_eq!(
                    error,
                    Error::Invalid("entity is not owned by handler".into())
                );
                // Catching the failure cannot publish the first handler's edit.
            }
            Ok(())
        }
    }
    let registration = |key: &str, mode| HandlerRegistration {
        key: key.into(),
        version: 1,
        event: EventKind::EntityTick,
        target: Some("test:entity".into()),
        handler: std::sync::Arc::new(Probe(mode)),
    };
    let event = Event::EntityTick {
        entity: 1,
        position: [0.; 3],
        tick: 0,
    };
    for mode in 1..=3 {
        let mut world = World { reads: 0 };
        let mut ctx = Context::new(&mut world, 16);
        ctx.dispatch(&registration("test:own", 0), &event).unwrap();
        ctx.dispatch(&registration("foreign:probe", mode), &event)
            .unwrap();
        assert!(ctx.finish().is_err());
    }
}

#[test]
fn ignored_failures_cannot_publish_partial_operations() {
    for failure in 0..4 {
        let mut world = World { reads: 0 };
        let mut ctx = Context::new(&mut world, if failure == 3 { 1 } else { 8 });
        ctx.set_block([0; 3], "test:air").unwrap();
        let error = match failure {
            0 => ctx.block([-1, 0, 0]).unwrap_err(),
            1 => ctx.spawn_drop([0.0; 3], "test:stone", 129, 0).unwrap_err(),
            2 => ctx.set_block([0; 3], "test:missing").unwrap_err(),
            _ => ctx.block([0; 3]).unwrap_err(),
        };
        assert_eq!(ctx.finish().unwrap_err(), error);
    }
    let mut world = World { reads: 0 };
    let mut ctx = Context::new(&mut world, 10);
    ctx.set_block([0; 3], "test:air").unwrap();
    assert_eq!(ctx.block([-1, 0, 0]), Err(Error::Unavailable([-1, 0, 0])));
    assert_eq!(ctx.finish().unwrap_err(), Error::Unavailable([-1, 0, 0]));
}

struct Inventories(BTreeMap<InventoryId, Vec<Slot>>);
impl Snapshot for Inventories {
    fn authorize_inventory(
        &self,
        owner: InventoryId,
        namespace: Option<&str>,
    ) -> Result<(), Error> {
        if owner == InventoryId::Player(8) && namespace != Some("test") {
            return Err(Error::Invalid("profile inventory authority denied".into()));
        }
        Ok(())
    }
    fn seed(&self) -> u64 {
        23
    }
    fn tick(&self) -> u64 {
        0
    }
    fn project_entity_state(&self, _: u64, state: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(state.to_vec())
    }
    fn nearby_entities(&mut self, _: [f32; 3], _: f32) -> Result<Vec<Entity>, Error> {
        Ok(vec![])
    }
    fn entity_state(&mut self, _: u64, _: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(None)
    }
    fn validate_entity_state(&self, key: &str, _: &str, _: &[u8]) -> Result<(), Error> {
        Err(Error::UnknownContent(key.into()))
    }
    fn inventory_accepts(&self, _: InventoryId, _: usize, _: &Stack) -> bool {
        true
    }
    fn entity(&mut self, _: u64) -> Result<Option<Entity>, Error> {
        Ok(None)
    }
    fn anchored_entity_at(&mut self, _: Cell) -> Result<Option<u64>, Error> {
        Ok(None)
    }
    fn player(&self) -> Option<u128> {
        Some(7)
    }
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        Err(Error::Unavailable(cell))
    }
    fn state(&self, key: &str) -> Result<Block, Error> {
        Err(Error::UnknownContent(key.into()))
    }
    fn item_exists(&self, key: &str) -> bool {
        key == "test:stone"
    }
    fn inventory(&mut self, id: InventoryId) -> Result<Vec<Slot>, Error> {
        self.0
            .get(&id)
            .cloned()
            .ok_or(Error::InventoryUnavailable(id))
    }
    fn validate_stack(&self, stack: &Stack) -> Result<(), Error> {
        if self.item_exists(&stack.item) {
            Ok(())
        } else {
            Err(Error::UnknownContent(stack.item.clone()))
        }
    }
}

#[test]
fn transfers_preserve_components_and_failed_capacity_checks_preserve_both_sides() {
    let a = InventoryId::Player(7);
    let b = InventoryId::Entity(8);
    let tagged = Stack {
        item: "test:stone".into(),
        count: 8,
        components: Some(Components {
            version: 1,
            bytes: vec![42],
        }),
    };
    let slot = |stack| Slot {
        stack,
        insert: true,
        extract: true,
    };
    let mut world = Inventories(BTreeMap::from([
        (a, vec![slot(Some(tagged.clone())), slot(None)]),
        (
            b,
            vec![slot(Some(Stack {
                count: 127,
                ..tagged.clone()
            }))],
        ),
    ]));
    let mut ctx = Context::new(&mut world, 32);
    assert_eq!(ctx.player(), Some(a));
    assert!(!ctx.transfer(a, 0, b, 0, 2).unwrap());
    assert_eq!(ctx.inventory(a).unwrap()[0].stack, Some(tagged.clone()));
    assert!(!ctx.give(b, Stack::new("test:stone", 1)).unwrap());
    assert!(ctx.transfer(a, 0, a, 1, 3).unwrap());
    assert!(ctx.transfer(a, 1, b, 0, 1).unwrap());
    assert!(ctx.transfer(a, 0, a, 0, 5).unwrap());
    let taken = ctx.take(a, 1, 2).unwrap().unwrap();
    assert_eq!(taken.components, tagged.components);
    assert_eq!(taken.count, 2);
    ctx.spawn_stack([0.5; 3], taken.clone(), 250).unwrap();
    let plan = ctx.finish().unwrap();
    assert_eq!(plan.drops[0].stack, taken);
    assert_eq!(plan.inventories[&a][0].as_ref().unwrap().count, 5);
    assert_eq!(plan.inventories[&a][1], None);
    assert_eq!(plan.inventories[&b][0].as_ref().unwrap().count, 128);
    assert_eq!(
        plan.inventories[&b][0].as_ref().unwrap().components,
        tagged.components
    );
}

#[test]
fn cached_profile_inventory_rechecks_each_handler_authority_and_latches_denial() {
    use std::sync::Arc;
    struct Query(bool);
    impl Handler for Query {
        fn handle(&self, context: &mut Context<'_>, _: &Event) -> Result<(), Error> {
            if self.0 {
                context.inventory(InventoryId::Player(8))?;
                assert!(context.give(InventoryId::Player(7), Stack::new("test:stone", 1))?);
            } else {
                assert!(context.inventory(InventoryId::Player(8)).is_err());
            }
            Ok(())
        }
    }
    let empty = Slot {
        stack: None,
        insert: true,
        extract: true,
    };
    let mut world = Inventories(BTreeMap::from([
        (InventoryId::Player(7), vec![empty.clone()]),
        (InventoryId::Player(8), vec![empty]),
    ]));
    let event = Event::ActionRequested {
        action: "test:act".into(),
        position: [0.; 3],
        cell: None,
        entity: None,
        slot: 0,
        arguments: vec![],
    };
    let mut context = Context::new(&mut world, 16);
    for (key, grant) in [("test:allowed", true), ("other:denied", false)] {
        let registration = HandlerRegistration {
            key: key.into(),
            version: 1,
            event: EventKind::ActionRequested,
            target: None,
            handler: Arc::new(Query(grant)),
        };
        let result = context.dispatch(&registration, &event);
        result.unwrap();
    }
    assert!(
        context.finish().is_err(),
        "caught cached-owner access kept a partial grant"
    );
}

#[test]
fn staged_motion_rechecks_owner_coalesces_fields_and_rejects_caught_errors() {
    struct Probe(u8);
    impl Handler for Probe {
        fn handle(&self, ctx: &mut Context<'_>, _: &Event) -> Result<(), Error> {
            if self.0 == 0 {
                assert!(ctx.set_motion(
                    1,
                    17,
                    MotionChange {
                        velocity: Some([1.0, 0.0, 0.0]),
                        ..Default::default()
                    }
                )?);
                assert!(ctx.set_motion(
                    1,
                    17,
                    MotionChange {
                        acceleration: Some([0.0, 1.0, 0.0]),
                        ..Default::default()
                    }
                )?);
                let motion = ctx.motion(1)?.unwrap();
                assert_eq!(motion.velocity, [1.0, 0.0, 0.0]);
                assert_eq!(motion.acceleration, [0.0, 1.0, 0.0]);
            } else if self.0 == 1 {
                assert!(ctx.motion(1).is_err());
            } else {
                assert!(ctx.set_motion(1, 16, MotionChange::default()).is_err());
            }
            Ok(())
        }
    }
    let event = Event::EntityTick {
        entity: 1,
        position: [0.0; 3],
        tick: 0,
    };
    let registration = |owner: &str, probe: u8| HandlerRegistration {
        key: format!("{owner}:control"),
        version: 1,
        event: EventKind::EntityTick,
        target: None,
        handler: std::sync::Arc::new(Probe(probe)),
    };
    let mut world = World { reads: 0 };
    let mut ctx = Context::new(&mut world, 16);
    ctx.dispatch(&registration("test", 0), &event).unwrap();
    let plan = ctx.finish().unwrap();
    assert_eq!(plan.motion_commands.len(), 1);
    for (owner, probe) in [("other", 1), ("test", 2)] {
        let mut world = World { reads: 0 };
        let mut ctx = Context::new(&mut world, 16);
        ctx.dispatch(&registration("test", 0), &event).unwrap();
        ctx.dispatch(&registration(owner, probe), &event).unwrap();
        assert!(ctx.finish().is_err());
    }
}
#[test]
fn moving_spawn_references_are_local_and_never_predict_durable_ids() {
    struct Probe(std::sync::Arc<std::sync::Mutex<Option<SpawnReference>>>);
    impl Handler for Probe {
        fn handle(&self, ctx: &mut Context<'_>, _: &Event) -> Result<(), Error> {
            let old = self.0.lock().unwrap().take();
            if let Some(old) = old {
                assert!(!ctx.owns_spawn_reference(&old));
            }
            let mut spawn = MovingSpawn {
                key: "test:bolt".into(),
                position: [0.0; 3],
                velocity: [0.0; 3],
                orientation: [0.0, 0.0, 0.0, 1.0],
                angular_velocity: [0.0; 3],
                state: vec![1],
                source: None,
            };
            let reference = ctx.spawn_moving_entity(spawn.clone())?;
            spawn.velocity = [2.0, 0.0, 0.0];
            ctx.configure_spawn(&reference, spawn)?;
            assert_eq!(reference.index(), 0);
            assert!(ctx.owns_spawn_reference(&reference));
            *self.0.lock().unwrap() = Some(reference);
            Ok(())
        }
    }
    let references = std::sync::Arc::new(std::sync::Mutex::new(None));
    let registration = HandlerRegistration {
        key: "test:launch".into(),
        version: 1,
        event: EventKind::EntityTick,
        target: None,
        handler: std::sync::Arc::new(Probe(references)),
    };
    for _ in 0..2 {
        let mut world = World { reads: 0 };
        let mut ctx = Context::new(&mut world, 4);
        ctx.dispatch(
            &registration,
            &Event::EntityTick {
                entity: 1,
                position: [0.0; 3],
                tick: 0,
            },
        )
        .unwrap();
        let plan = ctx.finish().unwrap();
        assert_eq!(plan.moving_spawns.len(), 1);
        assert_eq!(plan.moving_spawns[0].velocity, [2.0, 0.0, 0.0]);
    }
}

#[test]
fn owned_contact_query_recaptures_absence_and_hides_staged_removal() {
    struct Probe;
    impl Handler for Probe {
        fn handle(&self, c: &mut Context<'_>, _: &Event) -> Result<(), Error> {
            let value = c.motion_contact(1)?.unwrap();
            assert_eq!(value.motion_revision, 17);
            assert_eq!(value.normal, [0.0, 1.0, 0.0]);
            assert_eq!(c.motion_contact(2)?, None);
            c.remove_entity(1)?;
            assert_eq!(c.motion_contact(1)?, None);
            Ok(())
        }
    }
    let mut world = World { reads: 0 };
    let mut c = Context::new(&mut world, 16);
    c.dispatch(
        &HandlerRegistration {
            key: "test:contact".into(),
            version: 1,
            event: EventKind::EntityTick,
            target: None,
            handler: std::sync::Arc::new(Probe),
        },
        &Event::EntityTick {
            entity: 1,
            position: [0.0; 3],
            tick: 0,
        },
    )
    .unwrap();
    c.finish().unwrap();
    assert_eq!(world.reads, 3);
}
