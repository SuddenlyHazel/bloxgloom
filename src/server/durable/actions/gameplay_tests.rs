use super::*;
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, gameplay::*};
use std::sync::Arc;

struct HarvestExtension;
struct HarvestHandler;
struct ChestBreak;
impl Handler for ChestBreak {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::BlockRemoved { cell, cause, .. } = event;
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
        let Event::BlockRemoved { cell, cause, .. } = event;
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
