//! Lazy live/overlay/offline inventory capture and exact profile WAL translation.
use super::*;
use crate::{
    inventory::{Inventory, InventoryStore},
    server::{Client, journal::Change, players::inventory::Cache},
};
use std::collections::{BTreeMap, HashMap};
pub(in crate::server) struct Capture<'a> {
    pub clients: &'a HashMap<u64, Client>,
    pub overlay: &'a HashMap<u128, Inventory>,
    pub revisions: &'a HashMap<u128, u64>,
    pub cache: &'a mut Cache,
}
impl WorldSnapshot<'_> {
    pub(super) fn capture_inventory(&mut self, profile: u128) -> Result<Inventory, Error> {
        if let Some(before) = self.profile_inventory_before.get(&profile) {
            return Ok(before.clone());
        }
        if self.profile_inventory_before.len() >= 7 {
            return Err(Error::Invalid(
                "profile inventory read limit exceeded".into(),
            ));
        }
        let capture = self.profile_inventories.as_mut().ok_or_else(|| {
            Error::Invalid("profile inventory unavailable in this context".into())
        })?;
        let before = if let Some(client) = capture.clients.values().find(|c| c.profile == profile) {
            client.inventory.clone()
        } else if let Some(inventory) = capture.overlay.get(&profile) {
            inventory.clone()
        } else {
            capture
                .cache
                .get(profile, capture.revisions.get(&profile).copied())?
        };
        self.reads
            .inventory(profile, before.revision)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        self.profile_inventory_before
            .insert(profile, before.clone());
        Ok(before)
    }
}
pub(super) fn prepare(
    catalog: &Catalog,
    before: BTreeMap<u128, Inventory>,
    outputs: &mut BTreeMap<
        bloxgloom_host_api::gameplay::InventoryId,
        Vec<Option<bloxgloom_host_api::gameplay::Stack>>,
    >,
) -> io::Result<Vec<Change>> {
    let mut changes = Vec::new();
    for (profile, before) in before {
        if let Some(slots) =
            outputs.remove(&bloxgloom_host_api::gameplay::InventoryId::Player(profile))
        {
            let after = inventory::apply(catalog, &before, slots).map_err(error)?;
            if after != before {
                changes.push(Change::new(
                    crate::server::durable::inventory_state_key(profile),
                    InventoryStore::encode_snapshot_with_catalog(&before, catalog)?,
                    InventoryStore::encode_snapshot_with_catalog(&after, catalog)?,
                ));
            }
        }
    }
    Ok(changes)
}
