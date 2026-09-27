use super::*;
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, gameplay::*};
use std::sync::Arc;

struct HarvestExtension;
struct HarvestHandler;
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
        registrar.gameplay_handler(HandlerRegistration {
            key: "test:harvest".into(),
            version: 1,
            event: EventKind::BlockRemoved,
            target: Some("bloxgloom:stone".into()),
            handler: Arc::new(HarvestHandler),
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
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
