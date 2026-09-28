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

struct World {
    reads: usize,
}
impl Snapshot for World {
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
