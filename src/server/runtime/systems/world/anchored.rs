//! Expand only explicit WorldEdit proposals before the single shared decision
//! pass. Handler-created edits may not invalidate a second, unplanned anchor.
use super::*;
use crate::server::entities::{EntityLocation, PreparedEntityTransaction};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Expansion {
    pub cells: BTreeSet<[i32; 3]>,
    pub drops: Vec<gameplay::Spawn>,
    pub despawns: Vec<PreparedEntityTransaction>,
}

pub(super) struct Inputs<'a> {
    pub world: &'a mut World,
    pub entities: &'a EntityStore,
    pub lifecycles: Option<&'a crate::server::lifecycle::Registry>,
    pub reads: &'a mut TerrainReads,
    pub missing: &'a mut Vec<ChunkKey>,
    pub edits: &'a mut Vec<gameplay::Edit>,
    pub removals: &'a mut Vec<gameplay::Removal>,
    pub owners: &'a [(ChunkKey, RemovalCause)],
    pub radius: u8,
}

pub(super) fn expand(inputs: Inputs<'_>) -> io::Result<Expansion> {
    let Inputs {
        world,
        entities,
        lifecycles,
        reads,
        missing,
        edits,
        removals,
        owners,
        radius,
    } = inputs;
    let catalog = world.catalog_arc();
    let mut targets = BTreeMap::new();
    // The original proposal count is capped at 256 before entering this helper.
    for (&(x, y, z, _), &(owner, cause)) in edits.iter().zip(owners) {
        let cell = EntityCell::new(x, y, z);
        reads.entities(entities.capture_anchor_dependency(cell))?;
        let Some(id) = entities.anchored_at(cell) else {
            continue;
        };
        // In particular, do not route Burn through WorldEdit lifecycle policy.
        if cause != RemovalCause::WorldEdit {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "owner burn touches an anchor",
            ));
        }
        if !targets.contains_key(&id) && targets.len() == 32 {
            return Err(io::Error::new(
                ErrorKind::QuotaExceeded,
                "owner invalidation exceeds 32 entities",
            ));
        }
        targets.entry(id).or_insert((cell, owner));
    }
    let mut result = Expansion::default();
    let mut callbacks = Vec::new();
    for (id, (touched, owner)) in targets {
        let lifecycles = lifecycles.ok_or_else(|| {
            io::Error::new(
                ErrorKind::Unsupported,
                "owner edit has no lifecycle registry",
            )
        })?;
        reads.entities(entities.capture_entity_dependency(id))?;
        let snapshot = entities
            .snapshot(id)
            .ok_or_else(|| io::Error::new(ErrorKind::WouldBlock, "owner anchor disappeared"))?;
        let EntityLocation::Anchored {
            anchor,
            ref footprint,
            ..
        } = snapshot.location
        else {
            return Err(io::Error::other("invalid anchored location"));
        };
        let (cells, stacks) = crate::server::durable::actions::invalidation::removal(
            &catalog,
            lifecycles,
            &snapshot,
            Some(touched),
        )?;
        if cells.len() != footprint.len()
            || cells.iter().any(|(cell, _)| !footprint.contains(cell))
            || !cells.iter().any(|(cell, _)| *cell == touched)
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "registered footprint differs from entity",
            ));
        }
        // Each registered footprint/refund is bounded before construction (64
        // cells, at most 55 stacks). Retained aggregate work has tighter caps.
        if result.drops.len() + stacks.len() > 1760 {
            return Err(io::Error::new(
                ErrorKind::QuotaExceeded,
                "owner invalidation refund budget",
            ));
        }
        let mut anchor_state = None;
        for (cell, expected) in cells {
            let at = [cell.x, cell.y, cell.z];
            if !within_radius(cell.chunk(), owner, radius) {
                return Err(io::Error::new(
                    ErrorKind::Unsupported,
                    "owner footprint escaped its declared neighborhood",
                ));
            }
            reads.entities(entities.capture_anchor_dependency(cell))?;
            let actual = reads.read(world, cell.x, cell.y, cell.z)?;
            if actual.is_none() && missing.len() < 8 && !missing.contains(&cell.chunk()) {
                missing.push(cell.chunk());
            }
            if actual != Some(expected) || entities.anchored_at(cell) != Some(id) {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "owner invalidation footprint changed or unavailable",
                ));
            }
            if let Some(&(_, _, _, after)) = edits.iter().find(|&&(x, y, z, _)| [x, y, z] == at) {
                // Despawning cannot leave any of the old footprint in place,
                // nor can replacement create another uninitialized anchor.
                if after == expected
                    || catalog.anchored_for_state(after).is_some()
                    || catalog.inventory_for_state(after).is_some()
                    || lifecycles.for_state(&catalog, after).is_some()
                {
                    return Err(io::Error::new(
                        ErrorKind::Unsupported,
                        "owner replacement retains an anchored block state",
                    ));
                }
            } else {
                if edits.len() == 256 {
                    return Err(io::Error::new(
                        ErrorKind::QuotaExceeded,
                        "owner expanded edits exceed 256 cells",
                    ));
                }
                edits.push((cell.x, cell.y, cell.z, crate::world::AIR));
            }
            result.cells.insert(at);
            if cell == anchor {
                anchor_state = Some(expected);
            }
        }
        callbacks.push((
            anchor_state.ok_or_else(|| io::Error::other("registered footprint lacks anchor"))?,
            [anchor.x, anchor.y, anchor.z],
            RemovalCause::AnchoredBreak,
        ));
        let position = [anchor.x, anchor.y, anchor.z].map(|n| n as f32 + 0.5);
        result.drops.extend(
            stacks
                .into_iter()
                .map(|stack| (position, stack, std::time::Duration::from_millis(250))),
        );
        result.despawns.push(
            entities
                .prepare_despawn(id, snapshot.revision)
                .map_err(io::Error::other)?,
        );
    }
    // One anchored callback, not one primary-item harvest per footprint cell.
    // The lifecycle owns refunds; neighbors still observe every expanded edit.
    removals.retain(|(_, cell, _)| !result.cells.contains(cell));
    removals.extend(callbacks);
    Ok(result)
}
