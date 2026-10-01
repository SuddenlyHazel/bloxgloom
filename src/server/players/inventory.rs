//! Committed profile inventory participants use the existing inventory WAL domain.
mod cache;
pub(in crate::server) use cache::Cache;

use crate::{
    inventory::{Inventory, InventoryStore},
    server::{
        State,
        durable::{PublishEffects, inventory_state_key},
        journal::Change,
    },
};
use std::io;
/// Decode only the explicit inventory participants. Owner state and other
/// related domains retain their existing publication paths.
pub(in crate::server) fn decode(
    changes: &[Change],
    catalog: &crate::content::Catalog,
) -> io::Result<Vec<(u128, Inventory)>> {
    let mut inventories = Vec::new();
    for change in changes
        .iter()
        .filter(|c| c.key.domain == "bloxgloom:inventory")
    {
        let bytes: [u8; 16] = change
            .key
            .bytes
            .as_slice()
            .try_into()
            .map_err(|_| io::Error::other("invalid profile inventory key"))?;
        let profile = u128::from_le_bytes(bytes);
        let before = InventoryStore::decode_snapshot_with_catalog(&change.before, catalog)?;
        let after = InventoryStore::decode_snapshot_with_catalog(&change.after, catalog)?;
        if profile == 0
            || before.slots == after.slots
            || before.revision.checked_add(1) != Some(after.revision)
        {
            return Err(io::Error::other(
                "invalid committed profile inventory transition",
            ));
        }
        inventories.push((profile, after));
    }
    Ok(inventories)
}
pub(in crate::server) fn install(
    state: &mut State,
    profile: u128,
    inventory: &Inventory,
) -> io::Result<()> {
    let bytes = InventoryStore::encode_snapshot_with_catalog(inventory, state.world.catalog())?;
    state
        .durability
        .inventory_revisions
        .insert(profile, inventory.revision);
    state
        .durability
        .inventory_overlay
        .insert(profile, inventory.clone());
    state
        .profile_inventory_cache
        .remember(profile, inventory.clone());
    for client in state.clients.values_mut().filter(|c| c.profile == profile) {
        client.inventory = inventory.clone();
    }
    state
        .durability
        .remember_checkpoint(inventory_state_key(profile), bytes);
    Ok(())
}
/// Each affected live profile receives its own inventory after the shared WAL
/// receipt. Reliable publication uses the same queue-pressure policy as actors.
pub(in crate::server) fn publish(state: &mut State, inventories: Vec<(u128, Inventory)>) {
    for (profile, inventory) in inventories {
        let client_id = state
            .clients
            .iter()
            .find(|(_, c)| c.profile == profile)
            .map(|(id, _)| *id);
        state.notifications.enqueue(
            state.world.catalog(),
            &[],
            None,
            Some(profile),
            Some(&inventory),
        );
        state.durability.publish_queue.push(PublishEffects {
        spawned: vec![],
            client_id,
            profile: Some(profile),
            action_id: None,
            accepted: true,
            reason: String::new(),
            inventory: Some(inventory),
            deltas: vec![],
            entity_commit: None,
            pickups: vec![],
            fire_bursts: vec![],
        });
    }
}
