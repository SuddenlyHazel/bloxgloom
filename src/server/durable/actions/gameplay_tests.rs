use super::*;
use bloxgloom_host_api::actions::{Action, Operation, Request, Target};
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, gameplay::*};
use std::sync::Arc;

#[path = "gameplay_anchor_tests.rs"]
mod gameplay_anchor_tests;

struct HarvestExtension;
struct UseExtension;
struct ObservingExtension(std::sync::mpsc::Sender<Committed>);
struct NoPickupExtension;
struct RejectPickup;
struct UncreditedPickupExtension;
struct UncreditedPickup;
impl Handler for RejectPickup {
    fn handle(&self, _: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        if !matches!(event, Event::PickupRequested { .. }) {
            return Err(Error::Invalid("expected pickup decision".into()));
        }
        Ok(())
    }
}
impl Extension for NoPickupExtension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        UseExtension.register(registrar)?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:no_pickup".into(),
            version: 1,
            event: EventKind::PickupRequested,
            target: Some("bloxgloom:drop".into()),
            handler: Arc::new(RejectPickup),
        })
    }
}
impl Handler for UncreditedPickup {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::PickupRequested { drops, .. } = event else {
            return Err(Error::Invalid("expected pickup decision".into()));
        };
        context.take(InventoryId::Entity(drops[0].0), 0, 1)?;
        Ok(())
    }
}
impl Extension for UncreditedPickupExtension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        UseExtension.register(registrar)?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:uncredited_pickup".into(),
            version: 1,
            event: EventKind::PickupRequested,
            target: Some("bloxgloom:drop".into()),
            handler: Arc::new(UncreditedPickup),
        })
    }
}
struct UseObserver(std::sync::mpsc::Sender<Committed>);
impl Observer for UseObserver {
    fn on_commit(&self, event: &Committed) {
        let _ = self.0.send(event.clone());
    }
}
impl Extension for ObservingExtension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        UseExtension.register(registrar)?;
        registrar.gameplay_observer(ObserverRegistration {
            key: "test:observe".into(),
            version: 1,
            observer: Arc::new(UseObserver(self.0.clone())),
        })
    }
}
struct UseStick;
struct CollectDrop;
impl Handler for CollectDrop {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested { position, .. } = event else {
            return Err(Error::Invalid("expected collect action".into()));
        };
        let actor = context
            .player()
            .ok_or_else(|| Error::Invalid("actor missing".into()))?;
        let drop = context
            .nearby_entities(*position, 3.0)?
            .into_iter()
            .find(|entity| entity.entity_type == "bloxgloom:drop")
            .ok_or_else(|| Error::Invalid("drop missing".into()))?;
        let owner = InventoryId::Entity(drop.id);
        let stack = context.inventory(owner)?[0]
            .stack
            .clone()
            .ok_or_else(|| Error::Invalid("empty drop".into()))?;
        let Some(taken) = context.take(owner, 0, stack.count)? else {
            return Err(Error::Invalid("drop pickup delay is active".into()));
        };
        if !context.give(actor, taken)? {
            return Err(Error::Invalid("actor inventory full".into()));
        }
        Ok(())
    }
}
struct TickMarker;
impl Handler for TickMarker {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::EntityTick {
            entity,
            position,
            tick,
        } = event
        else {
            return Err(Error::Invalid("expected due entity event".into()));
        };
        if *tick != context.tick() {
            return Err(Error::Invalid("wrong logical tick".into()));
        }
        match context.entity_state(*entity)? {
            Some(state) if state == [1] => {
                assert!(context.update_entity(*entity, &[2])?);
                assert!(context.schedule_entity(*entity, Some(3))?);
                let cell = [
                    position[0].floor() as i32,
                    position[1].floor() as i32,
                    position[2].floor() as i32 + 1,
                ];
                context.set_block(cell, "bloxgloom:sand")?;
                context.spawn_drop(*position, "bloxgloom:seeds", 1, 250)
            }
            Some(state) if state == [2] => {
                assert!(context.schedule_entity(*entity, None)?);
                Ok(())
            }
            _ => Err(Error::Invalid("unexpected scheduled state".into())),
        }
    }
}
struct MarkUse;
impl Handler for MarkUse {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested {
            entity: Some(id),
            cell: Some(_),
            ..
        } = event
        else {
            return Err(Error::Invalid("expected identified entity use".into()));
        };
        if context.entity_state(*id)? != Some(vec![1]) {
            return Err(Error::Invalid("marker changed".into()));
        }
        if !context.update_entity(*id, &[2])? {
            return Err(Error::Invalid("marker gone".into()));
        }
        Ok(())
    }
}
impl Handler for UseStick {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested {
            action,
            position,
            cell,
            entity,
            slot,
            arguments,
        } = event
        else {
            return Err(Error::Invalid("expected item use".into()));
        };
        if action != "test:use_stick"
            || cell.is_some()
            || entity.is_some()
            || !matches!(arguments.as_slice(), [] | [1] | [2])
        {
            return Err(Error::Invalid("invalid item use target".into()));
        }
        let actor = context
            .player()
            .ok_or_else(|| Error::Invalid("actor missing".into()))?;
        let used = context
            .take(actor, usize::from(*slot), 1)?
            .ok_or_else(|| Error::Invalid("item absent".into()))?;
        let cell = [
            position[0].floor() as i32 + 2,
            position[1].floor() as i32,
            position[2].floor() as i32,
        ];
        if context.block(cell)?.block_type != "bloxgloom:air" {
            return Err(Error::Invalid("use target obstructed".into()));
        }
        context.set_block(cell, "bloxgloom:sand")?;
        context.spawn_stack(
            position.map(|n| n + 0.5),
            used,
            if arguments == &[2] { 0 } else { 250 },
        )?;
        if arguments == &[1] {
            context.spawn_drop(
                [position[0] + 100.0, position[1], position[2]],
                "bloxgloom:stick",
                1,
                0,
            )?;
        }
        context.spawn_entity("test:marker", position.map(|n| n + 0.5), &[1])
    }
}
impl Extension for UseExtension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.gameplay_entity(EntityDefinition {
            key: "test:marker".into(),
            schema_version: 1,
            schema_fingerprint: 0x123456,
            max_state_bytes: 1,
            initial_delay_ticks: Some(2),
            state: Arc::new(MarkerState),
        })?;
        registrar.action(Action {
            key: "test:use_stick".into(),
            version: 1,
            label: "USE STICK".into(),
            target: Target::Item("bloxgloom:stick".into()),
            operation: Operation::Gameplay,
            panel: None,
            command: None,
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:use_stick".into(),
            version: 1,
            event: EventKind::ActionRequested,
            target: Some("test:use_stick".into()),
            handler: Arc::new(UseStick),
        })?;
        registrar.action(Action {
            key: "test:mark_use".into(),
            version: 1,
            label: "MARK".into(),
            target: Target::Entity("test:marker".into()),
            operation: Operation::Gameplay,
            panel: None,
            command: None,
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:mark_use".into(),
            version: 1,
            event: EventKind::ActionRequested,
            target: Some("test:mark_use".into()),
            handler: Arc::new(MarkUse),
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:marker_tick".into(),
            version: 1,
            event: EventKind::EntityTick,
            target: Some("test:marker".into()),
            handler: Arc::new(TickMarker),
        })?;
        registrar.action(Action {
            key: "test:collect_drop".into(),
            version: 1,
            label: "COLLECT".into(),
            target: Target::Empty,
            operation: Operation::Gameplay,
            panel: None,
            command: None,
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:collect_drop".into(),
            version: 1,
            event: EventKind::ActionRequested,
            target: Some("test:collect_drop".into()),
            handler: Arc::new(CollectDrop),
        })
    }
}
struct PlacementExtension;
struct SupportExtension;
struct SoilUse;
impl Handler for SoilUse {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested { position, .. } = event else {
            return Err(Error::Invalid("not a use action".into()));
        };
        let below = [
            position[0].floor() as i32 + 2,
            position[1].floor() as i32,
            position[2].floor() as i32,
        ];
        context.set_block(below, "bloxgloom:air")
    }
}
struct FlowerNeighbor;
struct FlowerRemovalNeighbor;
struct SandNeighborRemoval;
impl Handler for FlowerNeighbor {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::NeighborChanged {
            cell,
            changed,
            previous,
            current,
        } = event
        else {
            return Err(Error::Invalid("not a neighbor decision".into()));
        };
        if cell[1] != changed[1] + 1 || !previous.supports_plant || current.supports_plant {
            return Err(Error::Invalid("wrong support transition".into()));
        }
        context.set_block(*cell, "bloxgloom:air")
    }
}
impl Handler for FlowerRemovalNeighbor {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::NeighborChanged {
            cell,
            changed,
            previous,
            current,
        } = event
        else {
            return Err(Error::Invalid("not a neighbor decision".into()));
        };
        if cell[0] == changed[0] + 1
            && cell[1] == changed[1]
            && cell[2] == changed[2]
            && previous.block_type == "bloxgloom:red_flower"
            && current.block_type == "bloxgloom:air"
        {
            context.set_block(*cell, "bloxgloom:air")?;
        }
        Ok(())
    }
}
impl Handler for SandNeighborRemoval {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::BlockRemoved {
            cell,
            previous,
            cause,
            ..
        } = event
        else {
            return Err(Error::Invalid("not a removal decision".into()));
        };
        if previous.block_type != "bloxgloom:sand" || *cause != RemovalCause::WorldEdit {
            return Err(Error::Invalid("adjacent edit is not support loss".into()));
        }
        context.spawn_drop(
            cell.map(|coordinate| coordinate as f32 + 0.5),
            "bloxgloom:sand",
            1,
            250,
        )
    }
}
impl Extension for SupportExtension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.action(Action {
            key: "test:remove_soil".into(),
            version: 1,
            label: "REMOVE SOIL".into(),
            target: Target::Item("bloxgloom:stick".into()),
            operation: Operation::Gameplay,
            panel: None,
            command: None,
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:remove_soil".into(),
            version: 1,
            event: EventKind::ActionRequested,
            target: Some("test:remove_soil".into()),
            handler: Arc::new(SoilUse),
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:flower_support".into(),
            version: 1,
            event: EventKind::NeighborChanged,
            target: Some("bloxgloom:red_flower".into()),
            handler: Arc::new(FlowerNeighbor),
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:flower_removal_neighbor".into(),
            version: 1,
            event: EventKind::NeighborChanged,
            target: Some("bloxgloom:sand".into()),
            handler: Arc::new(FlowerRemovalNeighbor),
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:sand_neighbor_removal".into(),
            version: 1,
            event: EventKind::BlockRemoved,
            target: Some("bloxgloom:sand".into()),
            handler: Arc::new(SandNeighborRemoval),
        })
    }
}
struct SandPlacement;
impl Handler for SandPlacement {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::BlockPlaced {
            cell,
            previous,
            placed,
        } = event
        else {
            return Err(Error::Invalid("expected placement".into()));
        };
        if previous.block_type != "bloxgloom:air" || placed.block_type != "bloxgloom:sand" {
            return Err(Error::Invalid("unexpected placement states".into()));
        }
        let next = [cell[0] + 1, cell[1], cell[2]];
        if context.block(next)?.block_type != "bloxgloom:air" {
            return Err(Error::Invalid("neighbour obstructed".into()));
        }
        context.set_block(next, "bloxgloom:sand")?;
        if !context.give(
            context.player().unwrap(),
            bloxgloom_host_api::gameplay::Stack::new("bloxgloom:seeds", 2),
        )? {
            return Err(Error::Invalid("inventory full".into()));
        }
        Ok(())
    }
}
impl Extension for PlacementExtension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:sand_placement".into(),
            version: 1,
            event: EventKind::BlockPlaced,
            target: Some("bloxgloom:sand".into()),
            handler: Arc::new(SandPlacement),
        })
    }
}
struct HarvestHandler;
struct ChestBreak;
impl Handler for ChestBreak {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::BlockRemoved { cell, cause, .. } = event else {
            return Err(Error::Invalid("expected removal".into()));
        };
        if *cause != RemovalCause::AnchoredBreak {
            return Err(Error::Invalid("expected anchored break".into()));
        }
        let id = context
            .anchored_entity_at(*cell)?
            .ok_or_else(|| Error::Invalid("missing chest at removal".into()))?;
        if context.entity(id)?.is_none() {
            return Err(Error::Invalid("chest vanished".into()));
        }
        context.spawn_drop(cell.map(|n| n as f32 + 0.5), "bloxgloom:stick", 1, 250)
    }
}
struct MarkerState;
impl EntityState for MarkerState {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data.len() == 1 && data[0] <= 2 {
            Ok(())
        } else {
            Err(RegistrationError(
                "marker state must be one byte <= 2".into(),
            ))
        }
    }
    fn public(&self, data: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(data.to_vec())
    }
}
impl Handler for HarvestHandler {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::BlockRemoved { cell, cause, .. } = event else {
            return Err(Error::Invalid("expected removal".into()));
        };
        assert_eq!(*cause, RemovalCause::Break);
        assert_eq!(context.block(*cell)?.block_type, "bloxgloom:air");
        let next = [cell[0] + 1, cell[1], cell[2]];
        assert!(context.entity(u64::MAX)?.is_none());
        assert!(context.anchored_entity_at(next)?.is_none());
        let player = context.player().unwrap();
        if !context.give(
            player,
            bloxgloom_host_api::gameplay::Stack::new("bloxgloom:seeds", 3),
        )? {
            return Err(Error::Invalid("harvest reward inventory full".into()));
        }
        if context.block(next)?.block_type == "bloxgloom:air" {
            context.set_block(next, "bloxgloom:sand")?;
        }
        if cell[2] == 0 {
            context.spawn_entity("test:marker", cell.map(|n| n as f32 + 0.5), &[1])?;
        } else if cell[2] == 2 || cell[2] == 3 {
            let found = context.nearby_entities(cell.map(|n| n as f32 + 0.5), 4.0)?;
            let marker = found
                .iter()
                .find(|entity| entity.entity_type == "test:marker")
                .ok_or_else(|| Error::Invalid("marker missing".into()))?;
            if cell[2] == 2 {
                assert_eq!(context.entity_state(marker.id)?, Some(vec![1]));
                assert!(context.update_entity(marker.id, &[2])?);
                assert_eq!(context.entity_state(marker.id)?, Some(vec![2]));
                assert_eq!(context.entity(marker.id)?.unwrap().data, vec![2]);
            } else {
                assert_eq!(context.entity_state(marker.id)?, Some(vec![2]));
                assert!(context.remove_entity(marker.id)?);
                assert_eq!(context.entity_state(marker.id)?, None);
                assert!(context.entity(marker.id)?.is_none());
            }
        }
        if let Some(chest) = context.anchored_entity_at([cell[0], cell[1], cell[2] + 1])? {
            assert_eq!(
                context.entity(chest)?.unwrap().entity_type,
                "bloxgloom:chest"
            );
            if !context.transfer(player, 0, InventoryId::Entity(chest), 0, 1)? {
                return Err(Error::Invalid("harvest chest is full".into()));
            }
        }
        context.spawn_drop(cell.map(|n| n as f32 + 0.5), "bloxgloom:stick", 2, 500)
    }
}
impl Extension for HarvestExtension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.gameplay_entity(EntityDefinition {
            key: "test:marker".into(),
            schema_version: 1,
            schema_fingerprint: 0x123456,
            max_state_bytes: 1,
            initial_delay_ticks: None,
            state: Arc::new(MarkerState),
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:harvest".into(),
            version: 1,
            event: EventKind::BlockRemoved,
            target: Some("bloxgloom:stone".into()),
            handler: Arc::new(HarvestHandler),
        })?;
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:chest_break".into(),
            version: 1,
            event: EventKind::BlockRemoved,
            target: Some("bloxgloom:chest".into()),
            handler: Arc::new(ChestBreak),
        })
    }
}
fn open(path: &std::path::Path) -> State {
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&HarvestExtension)
            .unwrap();
    crate::server::server_state_with_startup(23, path.to_owned(), 8, startup).unwrap()
}

#[test]
fn semantic_item_use_composes_world_inventory_drop_entity_and_receipt() {
    let path = temp_save_dir("gameplay-use");
    let y = MAX_GENERATED_HEIGHT + 32;
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&UseExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STICK, 2));
    let peer = add_test_client(&mut state, [14.5, y as f32, 0.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let request = Request {
        key: "test:use_stick".into(),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    let message = |payload| ClientMessage::EntityInteract {
        action_id: epoch | 1,
        target: [i32::MAX, y, 0],
        payload,
    };
    let edit = message(request.encode().unwrap());
    assert_eq!(
        plan_durable_request(&mut state, &edit_request(edit.clone()), TickId::new(10))
            .err()
            .unwrap()
            .kind(),
        ErrorKind::WouldBlock
    );
    state.world.edit(16, y, 0, AIR).unwrap();
    let mut wrong = request.clone();
    wrong.entity = 1;
    assert!(
        plan_durable_request(
            &mut state,
            &edit_request(message(wrong.encode().unwrap())),
            TickId::new(10)
        )
        .is_err()
    );
    wrong = request.clone();
    wrong.arguments = vec![1];
    assert_eq!(
        plan_durable_request(
            &mut state,
            &edit_request(message(wrong.encode().unwrap())),
            TickId::new(10)
        )
        .err()
        .unwrap()
        .kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        2
    );
    settle_live_action(&mut state, 11, edit.clone());
    settle_live_action(&mut state, 12, edit);
    assert_eq!(state.world.cached_block(16, y, 0), Some(crate::world::SAND));
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        1
    );
    let marker = state
        .entities
        .query_mobile_aabb([14.0, y as f32, 0.0], [16.0, y as f32 + 2.0, 2.0])
        .unwrap()
        .into_iter()
        .find(|id| {
            state.entities.public_view(*id).is_some_and(|view| {
                view.entity_type
                    == state
                        .world
                        .catalog()
                        .entity_type_id_by_key("test:marker")
                        .unwrap()
            })
        })
        .unwrap();
    assert_eq!(state.entities.public_view(marker).unwrap().payload, vec![1]);
    let mark = Request {
        key: "test:mark_use".into(),
        version: 1,
        slot: 0,
        inventory_revision: state.clients[&1].inventory.revision,
        entity: marker.get(),
        entity_revision: state.entities.snapshot(marker).unwrap().revision,
        arguments: vec![],
    };
    let targeted = |request: &Request| ClientMessage::EntityInteract {
        action_id: epoch | 2,
        target: [15, y, 1],
        payload: request.encode().unwrap(),
    };
    let mut stale = mark.clone();
    stale.entity_revision += 1;
    assert!(
        plan_durable_request(&mut state, &edit_request(targeted(&stale)), TickId::new(13)).is_err()
    );
    settle_live_action(&mut state, 13, targeted(&mark));
    assert_eq!(state.entities.public_view(marker).unwrap().payload, vec![2]);
    drop(peer);
    drop(state);
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&UseExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    assert_eq!(state.world.get_block(16, y, 0).unwrap(), crate::world::SAND);
    assert_eq!(
        state.inventory_store.load(17).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    assert_eq!(state.entities.public_view(marker).unwrap().payload, vec![2]);
    assert_eq!(
        crate::server::drops::nearby(&state.entities, [15.0, y as f32 + 1.0, 1.0])
            .iter()
            .filter(|drop| drop.item == STICK)
            .map(|drop| drop.count)
            .sum::<u16>(),
        1
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn scheduled_general_entity_commits_world_state_and_next_due_atomically() {
    let path = temp_save_dir("gameplay-schedule");
    let y = MAX_GENERATED_HEIGHT + 32;
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&UseExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    state.world.edit(16, y, 14, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STICK, 1));
    let peer = add_test_client(&mut state, [14.5, y as f32, 14.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let use_stick = Request {
        key: "test:use_stick".into(),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    settle_live_action(
        &mut state,
        11,
        ClientMessage::EntityInteract {
            action_id: epoch | 1,
            target: [i32::MAX, y, 14],
            payload: use_stick.encode().unwrap(),
        },
    );
    let marker = state
        .entities
        .query_mobile_aabb([14.0, y as f32, 14.0], [16.0, y as f32 + 2.0, 16.0])
        .unwrap()
        .into_iter()
        .find(|id| {
            state.entities.public_view(*id).is_some_and(|view| {
                view.entity_type
                    == state
                        .world
                        .catalog()
                        .entity_type_id_by_key("test:marker")
                        .unwrap()
            })
        })
        .unwrap();
    assert_eq!(state.entities.snapshot(marker).unwrap().next_tick, Some(13));
    assert!(
        plan_durable_request(
            &mut state,
            &DurableRequest::EntityTick { id: marker },
            TickId::new(12)
        )
        .unwrap()
        .is_none()
    );
    let missing = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: marker },
        TickId::new(13),
    )
    .err()
    .unwrap();
    assert_eq!(missing.kind(), ErrorKind::WouldBlock, "{missing}");
    state.world.edit(15, y, 16, AIR).unwrap();
    state
        .durability
        .queued
        .push_back(DurableRequest::EntityTick { id: marker });
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(13),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() && state.durability.queued.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        state.world.cached_block(15, y, 16),
        Some(crate::world::SAND)
    );
    assert_eq!(state.entities.public_view(marker).unwrap().payload, vec![2]);
    assert_eq!(state.entities.snapshot(marker).unwrap().next_tick, Some(16));
    drop(peer);
    drop(state);
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&UseExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    assert_eq!(
        state.world.get_block(15, y, 16).unwrap(),
        crate::world::SAND
    );
    assert_eq!(state.entities.snapshot(marker).unwrap().next_tick, Some(16));
    state
        .durability
        .queued
        .push_back(DurableRequest::EntityTick { id: marker });
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(16),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() && state.durability.queued.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(state.entities.snapshot(marker).unwrap().next_tick, None);
    assert!(!state.entities.due_entities(100, 8).contains(&marker));
    let seeds = crate::server::drops::nearby(&state.entities, [15.0, y as f32 + 0.5, 15.0]);
    assert_eq!(
        seeds
            .iter()
            .filter(|drop| drop.item == crate::items::SEEDS)
            .map(|drop| drop.count)
            .sum::<u16>(),
        1
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn semantic_use_neighbor_support_and_harvest_share_one_receipt() {
    let path = temp_save_dir("gameplay-neighbor-support");
    let y = 95;
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&SupportExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    state.world.edit(16, y, 0, crate::world::GRASS).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STICK, 1));
    let peer = add_test_client(&mut state, [14.5, y as f32, 0.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let request = Request {
        key: "test:remove_soil".into(),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    let command = ClientMessage::EntityInteract {
        action_id: epoch | 1,
        target: [i32::MAX, y, 0],
        payload: request.encode().unwrap(),
    };
    assert_eq!(
        plan_durable_request(&mut state, &edit_request(command.clone()), TickId::new(10))
            .err()
            .unwrap()
            .kind(),
        ErrorKind::WouldBlock
    );
    state
        .world
        .edit(16, y + 1, 0, crate::world::RED_FLOWER)
        .unwrap();
    state.world.edit(17, y + 1, 0, crate::world::SAND).unwrap();
    for [x, dy, z] in [[15, 0, 0], [17, 0, 0], [16, -1, 0], [16, 0, -1], [16, 0, 1]] {
        state.world.get_block(x, y + dy, z).unwrap();
    }
    // The chained flower and sand transitions read their own six neighbors.
    // This unit harness does not run the normal chunk-streaming worker.
    for [x, dy, z] in [
        [15, 1, 0],
        [16, 2, 0],
        [16, 1, -1],
        [16, 1, 1],
        [18, 1, 0],
        [17, 2, 0],
        [17, 1, -1],
        [17, 1, 1],
    ] {
        state.world.get_block(x, y + dy, z).unwrap();
    }
    settle_live_action(&mut state, 11, command.clone());
    settle_live_action(&mut state, 12, command);
    assert_eq!(state.world.cached_block(16, y, 0), Some(AIR));
    assert_eq!(state.world.cached_block(16, y + 1, 0), Some(AIR));
    assert_eq!(state.world.cached_block(17, y + 1, 0), Some(AIR));
    let flowers = crate::server::drops::nearby(&state.entities, [16.5, y as f32 + 1.5, 0.5]);
    assert_eq!(
        flowers
            .iter()
            .filter(|item| item.item == crate::items::ItemId::new(crate::world::RED_FLOWER.get()))
            .map(|item| item.count)
            .sum::<u16>(),
        1
    );
    assert_eq!(
        flowers
            .iter()
            .filter(|item| item.item == crate::items::ItemId::new(crate::world::SAND.get()))
            .map(|item| item.count)
            .sum::<u16>(),
        1,
        "chained neighbor removal must harvest exactly once"
    );
    drop(peer);
    drop(state);
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&SupportExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    assert_eq!(state.world.get_block(16, y, 0).unwrap(), AIR);
    assert_eq!(state.world.get_block(16, y + 1, 0).unwrap(), AIR);
    assert_eq!(state.world.get_block(17, y + 1, 0).unwrap(), AIR);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn world_drop_is_exact_extraction_inventory_with_atomic_player_credit() {
    let path = temp_save_dir("gameplay-drop-inventory");
    let y = MAX_GENERATED_HEIGHT + 32;
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&UseExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    state.world.edit(16, y, 0, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STICK, 2));
    let peer = add_test_client(&mut state, [14.5, y as f32, 0.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let send = |number: u128, key: &str, revision: u64, arguments: Vec<u8>| {
        let request = Request {
            key: key.into(),
            version: 1,
            slot: 0,
            inventory_revision: revision,
            entity: 0,
            entity_revision: 0,
            arguments,
        };
        ClientMessage::EntityInteract {
            action_id: epoch | number,
            target: [i32::MAX, y, 0],
            payload: request.encode().unwrap(),
        }
    };
    settle_live_action(&mut state, 11, send(1, "test:use_stick", 0, vec![2]));
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        1
    );
    let command = send(2, "test:collect_drop", 1, vec![]);
    settle_live_action(&mut state, 12, command.clone());
    settle_live_action(&mut state, 13, command);
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        2
    );
    let nearby = crate::server::drops::nearby(&state.entities, [15.0, y as f32, 1.0]);
    assert!(nearby.iter().all(|item| item.item != STICK));
    drop(peer);
    drop(state);
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&UseExtension)
            .unwrap();
    let state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    assert_eq!(
        state.inventory_store.load(17).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    assert!(
        crate::server::drops::nearby(&state.entities, [15.0, y as f32, 1.0])
            .iter()
            .all(|item| item.item != STICK)
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn registered_pickup_owner_can_decline_without_consuming_an_eligible_drop() {
    let path = temp_save_dir("gameplay-pickup-owner");
    let y = MAX_GENERATED_HEIGHT + 32;
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&NoPickupExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    state.world.edit(16, y, 0, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STICK, 2));
    let peer = add_test_client(&mut state, [14.5, y as f32, 0.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let request = Request {
        key: "test:use_stick".into(),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments: vec![2],
    };
    settle_live_action(
        &mut state,
        11,
        ClientMessage::EntityInteract {
            action_id: epoch | 1,
            target: [i32::MAX, y, 0],
            payload: request.encode().unwrap(),
        },
    );
    assert!(
        plan_durable_request(
            &mut state,
            &DurableRequest::Pickup { id: 1 },
            TickId::new(12)
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        1
    );
    assert!(
        crate::server::drops::nearby(&state.entities, [15.0, y as f32, 1.0])
            .iter()
            .any(|item| item.item == STICK && item.count == 1)
    );
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn automatic_pickup_rejects_uncredited_take_before_wal_admission() {
    let path = temp_save_dir("gameplay-pickup-credit");
    let y = MAX_GENERATED_HEIGHT + 32;
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&UncreditedPickupExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    state.world.edit(16, y, 0, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STICK, 2));
    let peer = add_test_client(&mut state, [14.5, y as f32, 0.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let request = Request {
        key: "test:use_stick".into(),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments: vec![2],
    };
    settle_live_action(
        &mut state,
        11,
        ClientMessage::EntityInteract {
            action_id: epoch | 1,
            target: [i32::MAX, y, 0],
            payload: request.encode().unwrap(),
        },
    );
    let before = crate::server::drops::nearby(&state.entities, [15.0, y as f32, 1.0]);
    assert_eq!(
        plan_durable_request(
            &mut state,
            &DurableRequest::Pickup { id: 1 },
            TickId::new(12)
        )
        .err()
        .unwrap()
        .kind(),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        1
    );
    let after = crate::server::drops::nearby(&state.entities, [15.0, y as f32, 1.0]);
    assert_eq!(after.len(), before.len());
    assert!(
        before.iter().zip(after).all(
            |(a, b)| (a.id, a.item, a.count, a.position) == (b.id, b.item, b.count, b.position)
        )
    );
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn committed_observers_see_public_results_only_after_receipt_without_replay() {
    let path = temp_save_dir("gameplay-committed-observer");
    let y = MAX_GENERATED_HEIGHT + 32;
    let (sender, receiver) = std::sync::mpsc::channel();
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&ObservingExtension(sender.clone()))
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    let mut manifest = crate::content::ContentManifest::from_catalog(state.world.catalog());
    manifest
        .entries
        .iter_mut()
        .find(|entry| entry.kind == b'O' && entry.key == "test:observe")
        .unwrap()
        .schema_fingerprint ^= 1;
    assert!(manifest.resolve_catalog(state.world.catalog()).is_err());
    state.world.edit(16, y, 0, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STICK, 1));
    let peer = add_test_client(&mut state, [14.5, y as f32, 0.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let request = Request {
        key: "test:use_stick".into(),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    let command = ClientMessage::EntityInteract {
        action_id: epoch | 1,
        target: [i32::MAX, y, 0],
        payload: request.encode().unwrap(),
    };
    assert!(receiver.try_recv().is_err());
    settle_live_action(&mut state, 11, command.clone());
    let observed = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(observed.inventory, Some((17, 1)));
    assert!(
        observed
            .blocks
            .iter()
            .any(|block| block.cell == [16, y, 0] && block.state == "bloxgloom:sand")
    );
    assert!(observed.entities.iter().any(|change| matches!(change, CommittedEntity::Spawned(entity) if entity.entity_type == "test:marker" && entity.data == [1])));
    assert!(observed.entities.iter().any(|change| matches!(change, CommittedEntity::Spawned(entity) if entity.entity_type == "bloxgloom:drop")));
    settle_live_action(&mut state, 12, command);
    assert!(
        receiver.try_recv().is_err(),
        "duplicate receipt must not redeliver a commit"
    );
    drop(peer);
    drop(state);
    let (sender, receiver) = std::sync::mpsc::channel();
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&ObservingExtension(sender))
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    assert_eq!(state.world.get_block(16, y, 0).unwrap(), crate::world::SAND);
    assert!(
        receiver.try_recv().is_err(),
        "advisory notifications are not replayed on restart"
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn placement_decision_stages_neighbour_and_reward_once_across_seam() {
    let path = temp_save_dir("gameplay-placement");
    let y = MAX_GENERATED_HEIGHT + 32;
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&PlacementExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    state.world.edit(15, y, 0, AIR).unwrap();
    let sand_item = state
        .world
        .catalog()
        .primary_block_item(crate::world::SAND)
        .unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(sand_item, 1));
    let peer = add_test_client(&mut state, [14.5, y as f32, -2.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let edit = ClientMessage::Edit {
        action_id: epoch | 1,
        x: 15,
        y,
        z: 0,
        block: crate::world::SAND,
        slot: 0,
    };
    let unavailable =
        plan_durable_request(&mut state, &edit_request(edit.clone()), TickId::new(10));
    assert_eq!(unavailable.err().unwrap().kind(), ErrorKind::WouldBlock);
    state.world.edit(16, y, 0, AIR).unwrap();
    settle_live_action(&mut state, 10, edit.clone());
    settle_live_action(&mut state, 11, edit);
    assert_eq!(state.world.cached_block(15, y, 0), Some(crate::world::SAND));
    assert_eq!(state.world.cached_block(16, y, 0), Some(crate::world::SAND));
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().item,
        crate::items::SEEDS
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        2
    );
    drop(peer);
    drop(state);
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&PlacementExtension)
            .unwrap();
    let mut state = crate::server::server_state_with_startup(23, path.clone(), 8, startup).unwrap();
    assert_eq!(state.world.get_block(16, y, 0).unwrap(), crate::world::SAND);
    assert_eq!(
        state.inventory_store.load(17).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn gameplay_decision_ownership_and_manifest_versions_are_enforced() {
    let mut catalog = crate::content::Catalog::builtins();
    crate::server::lifecycle::Registration::install(&HarvestExtension, &mut catalog).unwrap();
    let fingerprint = catalog.fingerprint();
    assert!(
        crate::server::lifecycle::Registration::install(&HarvestExtension, &mut catalog).is_err()
    );
    assert_eq!(catalog.fingerprint(), fingerprint);
    let mut manifest = crate::content::ContentManifest::from_catalog(&catalog);
    let handler = manifest
        .entries
        .iter_mut()
        .find(|entry| entry.kind == b'G' && entry.key == "test:harvest")
        .unwrap();
    handler.schema_fingerprint ^= 1;
    assert!(manifest.resolve_catalog(&catalog).is_err());
}

#[test]
fn registered_gameplay_combines_seam_edits_and_drops_and_recovers_once() {
    let path = temp_save_dir("shared-gameplay-restart");
    let y = MAX_GENERATED_HEIGHT + 32;
    let mut state = open(&path);
    state.world.edit(15, y, 0, crate::world::STONE).unwrap();
    let peer = add_test_client(&mut state, [14.5, y as f32, -2.5], Inventory::default());
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let message = ClientMessage::Edit {
        action_id: epoch | 2,
        x: 15,
        y,
        z: 0,
        block: AIR,
        slot: 0,
    };
    let unavailable =
        plan_durable_request(&mut state, &edit_request(message.clone()), TickId::new(10));
    assert_eq!(unavailable.err().unwrap().kind(), ErrorKind::WouldBlock);
    assert!(
        state.clients[&1]
            .inventory
            .slots
            .iter()
            .all(Option::is_none)
    );
    assert_eq!(
        state.world.cached_block(15, y, 0),
        Some(crate::world::STONE)
    );
    assert!(crate::server::drops::nearby(&state.entities, [15.5, y as f32, 0.5]).is_empty());
    state.world.edit(16, y, 0, AIR).unwrap();
    state.world.edit(15, y, 1, AIR).unwrap();
    state.clients.get_mut(&1).unwrap().inventory.slots[0] =
        Some(crate::inventory::Stack::new(crate::content::CHEST_ITEM, 1));
    settle_live_action(
        &mut state,
        8,
        ClientMessage::Edit {
            action_id: epoch | 1,
            x: 15,
            y,
            z: 1,
            block: crate::content::CHEST_STATE,
            slot: 0,
        },
    );
    let chest_id = state
        .entities
        .anchored_at(CellCoord::new(15, y, 1))
        .unwrap();
    let planned = plan_durable_request(&mut state, &edit_request(message.clone()), TickId::new(10))
        .unwrap()
        .unwrap();
    assert_eq!(planned.world_edits.len(), 2);
    assert!(planned.terrain_reads.entities_current(&state.entities));
    assert!(planned.entities.is_some());
    assert_eq!(
        state.world.cached_block(15, y, 0),
        Some(crate::world::STONE)
    );
    assert_eq!(state.world.cached_block(16, y, 0), Some(AIR));
    settle_live_action(&mut state, 10, message.clone());
    settle_live_action(&mut state, 11, message);
    assert!(!planned.terrain_reads.entities_current(&state.entities));
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        2
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().item,
        crate::items::SEEDS
    );
    assert_eq!(state.world.cached_block(15, y, 0), Some(AIR));
    assert_eq!(state.world.cached_block(16, y, 0), Some(crate::world::SAND));
    drop(peer);
    drop(state);
    let mut state = open(&path);
    assert_eq!(state.world.get_block(15, y, 0).unwrap(), AIR);
    let inventory = state.inventory_store.load(17).unwrap();
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 2);
    let chest = state.entities.snapshot(chest_id).unwrap();
    let chest = chest
        .private_payload
        .downcast_ref::<crate::server::entities::container::ContainerPayload>()
        .unwrap();
    assert_eq!(chest.slots[0].as_ref().unwrap().count, 1);
    assert_eq!(chest.slots[0].as_ref().unwrap().item, crate::items::SEEDS);
    assert_eq!(
        inventory.slots[0].as_ref().unwrap().item,
        crate::items::SEEDS
    );
    assert_eq!(state.world.get_block(16, y, 0).unwrap(), crate::world::SAND);
    let drops = crate::server::drops::nearby(&state.entities, [15.5, y as f32 + 0.5, 0.5]);
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].item, STICK);
    assert_eq!(drops[0].count, 2);
    let markers = state
        .entities
        .query_mobile_aabb([15.0, y as f32, 0.0], [16.0, y as f32 + 1.0, 1.0])
        .unwrap();
    assert!(
        markers
            .iter()
            .any(|id| state
                .entities
                .public_view(*id)
                .is_some_and(|view| view.entity_type
                    == state
                        .world
                        .catalog()
                        .entity_type_id_by_key("test:marker")
                        .unwrap()))
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn generic_entity_spawn_update_remove_composes_with_breaks_and_recovers() {
    let path = temp_save_dir("general-gameplay-entity");
    let y = MAX_GENERATED_HEIGHT + 32;
    let mut state = open(&path);
    for z in [0, 2, 3] {
        state.world.edit(0, y, z, crate::world::STONE).unwrap();
        state.world.edit(1, y, z, AIR).unwrap();
    }
    let peer = add_test_client(&mut state, [0.5, y as f32, 1.5], Inventory::default());
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let command = |z, sequence| ClientMessage::Edit {
        action_id: epoch | sequence,
        x: 0,
        y,
        z,
        block: AIR,
        slot: 0,
    };
    settle_live_action(&mut state, 10, command(0, 1));
    let id = state
        .entities
        .query_mobile_aabb([0.0, y as f32, 0.0], [1.0, y as f32 + 1.0, 1.0])
        .unwrap()
        .into_iter()
        .find(|id| {
            state.entities.public_view(*id).is_some_and(|view| {
                view.entity_type
                    == state
                        .world
                        .catalog()
                        .entity_type_id_by_key("test:marker")
                        .unwrap()
            })
        })
        .unwrap();
    assert_eq!(state.entities.public_view(id).unwrap().payload, vec![1]);
    settle_live_action(&mut state, 11, command(2, 2));
    assert_eq!(state.entities.public_view(id).unwrap().payload, vec![2]);
    settle_live_action(&mut state, 12, command(3, 3));
    assert!(state.entities.snapshot(id).is_none());
    drop(peer);
    drop(state);
    let mut state = open(&path);
    assert!(state.entities.snapshot(id).is_none());
    for z in [0, 2, 3] {
        assert_eq!(state.world.get_block(0, y, z).unwrap(), AIR);
    }
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn anchored_break_uses_registered_removal_without_duplicating_refunds() {
    let path = temp_save_dir("gameplay-anchored-break");
    let y = MAX_GENERATED_HEIGHT + 32;
    let mut state = open(&path);
    state.world.edit(0, y, 0, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(crate::content::CHEST_ITEM, 1));
    let peer = add_test_client(&mut state, [0.5, y as f32, -2.5], inventory);
    let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
    let edit = |sequence, block| ClientMessage::Edit {
        action_id: epoch | sequence,
        x: 0,
        y,
        z: 0,
        block,
        slot: 0,
    };
    settle_live_action(&mut state, 10, edit(1, crate::content::CHEST_STATE));
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(0, y, 0))
            .is_some()
    );
    settle_live_action(&mut state, 11, edit(2, AIR));
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(0, y, 0))
            .is_none()
    );
    drop(peer);
    drop(state);
    let mut state = open(&path);
    assert_eq!(state.world.get_block(0, y, 0).unwrap(), AIR);
    let drops = crate::server::drops::nearby(&state.entities, [0.5, y as f32 + 0.5, 0.5]);
    assert_eq!(
        drops
            .iter()
            .filter(|item| item.item == crate::content::CHEST_ITEM)
            .map(|item| item.count)
            .sum::<u16>(),
        1
    );
    assert_eq!(
        drops
            .iter()
            .filter(|item| item.item == STICK)
            .map(|item| item.count)
            .sum::<u16>(),
        1
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
