//! Authorization and all-or-nothing inventory planning for creative grants.

use crate::content::Catalog;
use crate::inventory::{Inventory, STACK_LIMIT};
use crate::items::ItemId;
use std::io::{self, ErrorKind};

/// One spawn, at one of twelve nearby supported locations, with a bounded
/// population query. The allocator and destination chunk serialize concurrent
/// spawns; terrain read keys remain fenced through the shared WAL receipt.
pub(super) fn plan_mossbun(
    state: &mut crate::server::State,
    profile: u128,
    position: [f32; 3],
    tick: u64,
) -> io::Result<crate::server::entities::PreparedEntityTransaction> {
    use crate::server::entities::{EntityLocation, EntityPayload, EntitySpawn, mossbun};
    if state.admin_profile != Some(profile) || profile == 0 {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "admin access denied",
        ));
    }
    let view =
        super::entity::capture_view_for_plan(state, &EntityLocation::Mobile { position }, 1)?;
    let entity_type = state
        .world
        .catalog()
        .entity_type_id_by_key("bloxgloom:mossbun")
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "mossbun type unavailable"))?;
    for [dx, dz] in [[2.0, 0.0], [0.0, 2.0], [-2.0, 0.0], [0.0, -2.0]] {
        for dy in [0.0, 1.0, -1.0] {
            let candidate = [
                position[0].floor() + 0.5 + dx,
                position[1].floor() + dy,
                position[2].floor() + 0.5 + dz,
            ];
            if !mossbun::spawn_clear(&view, candidate).map_err(io::Error::other)? {
                continue;
            }
            let chunk = crate::server::entities::position_to_cell(candidate)
                .map_err(io::Error::other)?
                .chunk();
            // Fail closed on a crowded page; do not collect the whole population.
            let nearby = state
                .entities
                .public_views_for_chunk_bounded(chunk, 32)
                .map_err(|_| io::Error::new(ErrorKind::QuotaExceeded, "spawn chunk is crowded"))?;
            if nearby
                .iter()
                .filter(|e| e.entity_type == entity_type)
                .count()
                >= 16
            {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "at most 16 mossbuns per spawn chunk",
                ));
            }
            let mut prepared = state
                .entities
                .prepare_spawn_batch(vec![EntitySpawn::Mobile {
                    entity_type,
                    position: candidate,
                    payload: EntityPayload::new(mossbun::Mossbun::default()),
                    spawn_tick: tick,
                }])
                .map_err(io::Error::other)?;
            for (key, _) in view.revisions() {
                prepared.add_read_key(super::super::chunk_state_key(*key));
            }
            return Ok(prepared);
        }
    }
    Err(io::Error::new(
        ErrorKind::InvalidInput,
        "no clear supported spot nearby",
    ))
}

pub(super) fn plan_grant(
    admin_profile: Option<u128>,
    profile: u128,
    inventory: &Inventory,
    item: ItemId,
    count: u16,
    catalog: &Catalog,
) -> io::Result<Option<Inventory>> {
    if admin_profile != Some(profile) || profile == 0 {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "admin access denied",
        ));
    }
    if catalog.item(item).is_none() || !(1..=STACK_LIMIT).contains(&count) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid admin grant",
        ));
    }
    let mut next = inventory.clone();
    if next.insert_with_catalog(item, count, catalog) != 0 {
        return Ok(None);
    }
    Ok(Some(next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_rejects_unauthorized_and_never_partially_fills() {
        let catalog = Catalog::builtins();
        let item = catalog.items().next().unwrap().id;
        let original = Inventory::default();
        assert_eq!(
            plan_grant(None, 17, &original, item, 128, &catalog)
                .unwrap_err()
                .kind(),
            ErrorKind::PermissionDenied
        );
        let granted = plan_grant(Some(17), 17, &original, item, 128, &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(granted.slots[0].as_ref().unwrap().count, 128);
        assert_eq!(original.slots[0], None);
        let mut full = granted;
        for slot in &mut full.slots[1..] {
            *slot = Some(crate::inventory::Stack::new(item, 128));
        }
        assert!(
            plan_grant(Some(17), 17, &full, item, 1, &catalog)
                .unwrap()
                .is_none()
        );
        assert_eq!(full.revision, 1);
    }
}
