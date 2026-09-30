//! Bounded, authoritative owner-neighborhood capture for public worker jobs.
//! Every observed chunk is fenced until the owner's WAL receipt. Missing
//! chunks are requested asynchronously by the coordinator, never synthesized.
use super::{OwnerEffectPatch, OwnerKey, OwnerPatch, TerrainReads};
use crate::server::durable::{BlockDelta, CommitAction};
use crate::server::effects::CellCoord;
use crate::server::entities::{CellCoord as EntityCell, EntitySnapshot, EntityStore};
use crate::server::gameplay::{self, OperationInput, Participants};
use crate::world::{CHUNK_SIZE, Chunk, ChunkKey, World};
use bloxgloom_host_api::gameplay::RemovalCause;
use std::io::{self, ErrorKind};
use std::sync::Arc;

#[path = "world/anchored.rs"]
mod anchored;
#[path = "world/entities.rs"]
mod entities;

/// Capture complete entity pages alongside terrain so a worker can inspect
/// package-owned private state without racing a concurrent spawn or update.
pub(super) fn capture_entities(
    entities: &EntityStore,
    catalog: &crate::content::Catalog,
    chunks: &[Arc<Chunk>],
    system_key: &str,
    reads: &mut TerrainReads,
) -> io::Result<Vec<EntitySnapshot>> {
    let keys = chunks.iter().map(|chunk| chunk.key);
    reads.entities(
        entities
            .capture_dependencies(keys.clone(), 128)
            .map_err(|error| io::Error::new(ErrorKind::WouldBlock, error))?,
    )?;
    let namespace = system_key.split_once(':').map_or("", |(owner, _)| owner);
    let mut snapshots = Vec::new();
    let mut captured_bytes = 0usize;
    for key in keys {
        if let Some(page) = entities.chunk_pages().get(&key) {
            for entity_id in &page.entity_ids {
                let snapshot = entities.snapshot(*entity_id).ok_or_else(|| {
                    io::Error::new(ErrorKind::WouldBlock, "captured owner entity vanished")
                })?;
                let entity_key = catalog
                    .entity_type(snapshot.entity_type)
                    .ok_or_else(|| io::Error::other("unknown captured entity type"))?
                    .key
                    .as_ref();
                if entity_key
                    .split_once(':')
                    .is_some_and(|(owner, _)| owner == namespace)
                    && matches!(
                        &snapshot.location,
                        crate::server::entities::EntityLocation::Mobile { .. }
                    )
                    && catalog.gameplay_entity(entity_key).is_some()
                {
                    let state_bytes = snapshot
                        .private_payload
                        .downcast_ref::<Vec<u8>>()
                        .ok_or_else(|| io::Error::other("invalid captured entity payload"))?;
                    captured_bytes = captured_bytes.saturating_add(state_bytes.len());
                    if captured_bytes > 65_536 {
                        return Err(io::Error::new(
                            ErrorKind::WouldBlock,
                            "owner entity capture exceeds 64 KiB",
                        ));
                    }
                    snapshots.push(snapshot);
                }
            }
        }
    }
    snapshots.sort_by_key(|snapshot| snapshot.id);
    snapshots.dedup_by_key(|snapshot| snapshot.id);
    Ok(snapshots)
}

pub(super) fn capture(
    world: &mut World,
    reads: &mut TerrainReads,
    owner: OwnerKey,
    radius: u8,
    missing: &mut Vec<ChunkKey>,
) -> io::Result<Option<Vec<Arc<Chunk>>>> {
    let center = owner.as_chunk().ok_or_else(|| {
        io::Error::new(ErrorKind::InvalidData, "world-reading owner is not a chunk")
    })?;
    let width = 2 * usize::from(radius) + 1;
    let mut chunks = Vec::with_capacity(width * width * width);
    for dx in -i32::from(radius)..=i32::from(radius) {
        for dy in -i32::from(radius)..=i32::from(radius) {
            for dz in -i32::from(radius)..=i32::from(radius) {
                let key = match (
                    center.x.checked_add(dx),
                    center.y.checked_add(dy),
                    center.z.checked_add(dz),
                ) {
                    (Some(x), Some(y), Some(z)) => ChunkKey { x, y, z },
                    _ => {
                        return Err(io::Error::new(
                            ErrorKind::InvalidInput,
                            "owner neighborhood outside world coordinates",
                        ));
                    }
                };
                let cell = [key.x, key.y, key.z].map(|n| n.checked_mul(CHUNK_SIZE as i32));
                let [Some(x), Some(y), Some(z)] = cell else {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "owner neighborhood outside world coordinates",
                    ));
                };
                if let Some(chunk) = world.cached_arc_chunk(key) {
                    if reads.read(world, x, y, z)?.is_none() {
                        return Err(io::Error::new(
                            ErrorKind::WouldBlock,
                            "captured chunk left the authoritative cache",
                        ));
                    }
                    chunks.push(chunk);
                } else if missing.len() < 8 && !missing.contains(&key) {
                    missing.push(key);
                }
            }
        }
    }
    Ok((chunks.len() == width * width * width).then_some(chunks))
}

/// Translate public owner proposals through the shared edit/neighbor planner.
/// Generated entities and drops share the owner's WAL record and receipt. There
/// is no player actor, so player inventory/pickup effects remain unsupported.
pub(super) struct EditInputs<'a> {
    pub world: &'a mut World,
    pub entities: &'a EntityStore,
    pub lifecycles: Option<&'a crate::server::lifecycle::Registry>,
    pub players: &'a [[f32; 3]],
    pub seed: u64,
    pub tick: u64,
    pub system_key: &'a str,
    pub radius: Option<u8>,
    pub patches: &'a [OwnerPatch],
    pub reads: &'a mut TerrainReads,
    pub missing: &'a mut Vec<ChunkKey>,
}

pub(super) fn plan_edits(inputs: EditInputs<'_>) -> io::Result<Option<CommitAction>> {
    let EditInputs {
        world,
        entities,
        lifecycles,
        players,
        seed,
        tick,
        system_key,
        radius,
        patches,
        reads,
        missing,
    } = inputs;
    let mut edits = Vec::new();
    let mut direct_drops = Vec::new();
    let mut direct_entities = Vec::new();
    let mut removals = Vec::new();
    let mut owners = Vec::new();
    let mut edit_owners = Vec::new();
    let mut edited = std::collections::BTreeSet::new();
    let catalog = world.catalog_arc();
    let direct_changes =
        entities::plan_changes(entities, &catalog, system_key, radius, patches, reads)?;
    for patch in patches {
        if !OwnerEffectPatch::entity_spawns(patch).is_empty() {
            let Some(owner) = patch.owner().as_chunk() else {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner entity requires chunk owner",
                ));
            };
            let Some(radius) = radius else {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner entity requires world capture",
                ));
            };
            let namespace = system_key.split_once(':').map_or("", |(owner, _)| owner);
            for spawn in OwnerEffectPatch::entity_spawns(patch) {
                if direct_entities.len() >= 256
                    || spawn.state.len() > 1024
                    || spawn
                        .position
                        .iter()
                        .any(|v| !v.is_finite() || !(-1_000_000.0..=1_000_000.0).contains(v))
                    || spawn.key.split_once(':').map_or("", |(owner, _)| owner) != namespace
                {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "invalid owner entity spawn",
                    ));
                }
                let [x, y, z] = spawn.position.map(|v| v.floor() as i32);
                let (key, _) = crate::world::world_to_chunk(x, y, z);
                if !within_radius(key, owner, radius) {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "owner entity escaped its neighborhood",
                    ));
                }
                let definition = catalog.gameplay_entity(&spawn.key).ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidInput,
                        "owner entity has no general schema",
                    )
                })?;
                if spawn.state.len() > usize::from(definition.max_state_bytes)
                    || definition.state.validate(&spawn.state).is_err()
                    || !definition
                        .state
                        .public(&spawn.state)
                        .is_ok_and(|view| view.len() <= 4096)
                {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "invalid owner entity state",
                    ));
                }
                let entity_type = catalog.entity_type_id_by_key(&spawn.key).ok_or_else(|| {
                    io::Error::new(ErrorKind::InvalidInput, "unknown owner entity type")
                })?;
                direct_entities.push((spawn.position, entity_type, spawn.state.clone()));
            }
        }
        if !OwnerEffectPatch::drops(patch).is_empty() {
            let Some(owner) = patch.owner().as_chunk() else {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner drop requires chunk owner",
                ));
            };
            let Some(radius) = radius else {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner drop requires world capture",
                ));
            };
            for drop in OwnerEffectPatch::drops(patch) {
                if direct_drops.len() >= 256
                    || !(1..=128).contains(&drop.count)
                    || drop
                        .position
                        .iter()
                        .any(|v| !v.is_finite() || *v < -1_000_000.0 || *v > 1_000_000.0)
                {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "invalid owner drop",
                    ));
                }
                let [x, y, z] = drop.position.map(|v| v.floor() as i32);
                let (key, _) = crate::world::world_to_chunk(x, y, z);
                if !within_radius(key, owner, radius) {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "owner drop escaped its neighborhood",
                    ));
                }
                let item = catalog.item_by_key(&drop.item).ok_or_else(|| {
                    io::Error::new(ErrorKind::InvalidInput, "unknown owner drop item")
                })?;
                direct_drops.push((
                    drop.position,
                    crate::inventory::Stack::new(item, drop.count),
                    std::time::Duration::from_millis(u64::from(drop.pickup_delay_ms)),
                ));
            }
        }
        let proposals = OwnerEffectPatch::world_edits(patch);
        if proposals.is_empty() {
            continue;
        }
        let Some(owner) = patch.owner().as_chunk() else {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "world edit requires a chunk owner",
            ));
        };
        let Some(radius) = radius else {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "world edit requires captured terrain",
            ));
        };
        owners.push(owner);
        for edit in proposals {
            if edits.len() >= 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "owner wave exceeds 256 block edits",
                ));
            }
            let [x, y, z] = edit.cell;
            let key = crate::world::world_to_chunk(x, y, z).0;
            if !within_radius(key, owner, radius) {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner edit escaped its declared neighborhood",
                ));
            }
            // Overlapping captures do not grant last-worker-wins semantics.
            // Reject ambiguous multi-owner writes before gameplay or WAL staging.
            if !edited.insert(edit.cell) {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "owner wave edits the same cell more than once",
                ));
            }
            let previous = world.cached_block(x, y, z).ok_or_else(|| {
                io::Error::new(ErrorKind::WouldBlock, "owner edit chunk no longer resident")
            })?;
            if gameplay::block(&catalog, previous)
                .map_err(gameplay::error)?
                .state
                != edit.before
            {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "owner edit preimage changed",
                ));
            }
            let after = catalog.state_by_key(&edit.after).ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidInput, "unknown owner edit block state")
            })?;
            let cause = match OwnerEffectPatch::edit_cause(patch) {
                bloxgloom_host_api::system::EditCause::WorldEdit => RemovalCause::WorldEdit,
                bloxgloom_host_api::system::EditCause::Burn => {
                    if previous == crate::world::AIR || after != crate::world::AIR {
                        return Err(io::Error::new(
                            ErrorKind::InvalidInput,
                            "owner burn must remove a non-air block to air",
                        ));
                    }
                    RemovalCause::Burn
                }
            };
            edits.push((x, y, z, after));
            edit_owners.push((owner, cause));
            if previous != crate::world::AIR {
                removals.push((previous, edit.cell, cause));
            }
        }
    }
    if edits.is_empty()
        && direct_drops.is_empty()
        && direct_entities.is_empty()
        && direct_changes.is_empty()
    {
        return Ok(None);
    }
    let radius = radius.expect("edits require a world view");
    let world_edit = edit_owners
        .iter()
        .any(|(_, cause)| *cause == RemovalCause::WorldEdit);
    let anchored = if world_edit {
        anchored::expand(anchored::Inputs {
            world,
            entities,
            lifecycles,
            reads,
            missing,
            edits: &mut edits,
            removals: &mut removals,
            owners: &edit_owners,
            radius,
        })?
    } else {
        // Burn retains its existing gameplay dispatch and anchored rejection.
        anchored::Expansion::default()
    };
    let mut requested = Vec::new();
    let planned = gameplay::plan_removals(
        world,
        reads,
        &mut requested,
        OperationInput {
            edits: &edits,
            removals: &removals,
            seed,
            tick,
            action: None,
        },
        Participants {
            players: &[],
            action_id: None,
            clock: None,
            actor: None,
            actor_position: None,
            admin: false,
            entities,
        },
    );
    for key in requested {
        if missing.len() == 8 {
            break;
        }
        if !missing.contains(&key) {
            missing.push(key);
        }
    }
    let mut planned = planned?;
    if planned.inventory.is_some() || !planned.drop_takes.is_empty() {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "owner block edit has no player inventory/pickup participant",
        ));
    }
    if anchored.cells.iter().any(|cell| {
        edits
            .iter()
            .find(|&&(x, y, z, _)| [x, y, z] == *cell)
            .is_none_or(|edit| !planned.edits.contains(edit))
    }) {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "gameplay handler rewrote an invalidated footprint",
        ));
    }
    for &(x, y, z, after) in &planned.edits {
        let (key, _) = crate::world::world_to_chunk(x, y, z);
        if !owners
            .iter()
            .any(|owner| within_radius(key, *owner, radius))
        {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "gameplay effect escaped the owner read neighborhood",
            ));
        }
        if world_edit {
            reads.entities(entities.capture_anchor_dependency(EntityCell::new(x, y, z)))?;
        }
        if entities.anchored_at(EntityCell::new(x, y, z)).is_some()
            && !anchored.cells.contains(&[x, y, z])
        {
            return Err(io::Error::new(
                ErrorKind::Unsupported,
                "owner block edit would orphan an anchored entity",
            ));
        }
        if catalog.block_flags(after) & crate::content::SOLID != 0
            && players.iter().any(|position| {
                crate::server::block_intersects_player(
                    catalog.player_rules().body(),
                    [x, y, z],
                    *position,
                )
            })
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "owner block edit overlaps a player",
            ));
        }
    }
    planned.drops.extend(anchored.drops);
    planned.drops.extend(direct_drops);
    for (position, entity_type, state) in direct_entities {
        let [x, y, z] = position.map(|v| v.floor() as i32);
        let block = if let Some(&(_, _, _, block)) = planned
            .edits
            .iter()
            .find(|&&(ex, ey, ez, _)| [ex, ey, ez] == [x, y, z])
        {
            block
        } else {
            let Some(block) = reads.read(world, x, y, z)? else {
                let key = crate::world::world_to_chunk(x, y, z).0;
                if missing.len() < 8 && !missing.contains(&key) {
                    missing.push(key);
                }
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "owner entity terrain unavailable",
                ));
            };
            block
        };
        if catalog.block_flags(block) & crate::content::SOLID != 0 {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "owner entity spawn obstructed",
            ));
        }
        planned
            .entity_spawns
            .push(crate::server::entities::EntitySpawn::Mobile {
                entity_type,
                position,
                payload: crate::server::entities::EntityPayload::new(state),
                spawn_tick: tick,
            });
    }
    planned.entity_updates.extend(anchored.despawns);
    planned.entity_updates.extend(direct_changes);
    // Preflight the shared merge planner's spatial traversal before it collects
    // candidates. Dense pages fail the bounded dependency capture, rather than
    // scanning an arbitrarily large population and truncating afterwards.
    for (position, _, _) in &planned.drops {
        reads.entities(
            entities
                .capture_mobile_dependencies(*position, 1.0)
                .map_err(io::Error::other)?,
        )?;
    }
    let participants = crate::server::drops::plan_stack_spawns_with_extra(
        entities,
        &catalog,
        &planned.drops,
        planned.entity_spawns,
        tick,
        crate::server::drops::unix_ms(),
    )?;
    let participants = gameplay::combine_entities(entities, participants, planned.entity_updates)?;
    // Planning and admission run consecutively on the coordinator, with no
    // intervening apply. Admission fences every dependency until the receipt;
    // publication validates again before installing any participant.
    if !reads.entities_current(entities) {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "stale owner entity read",
        ));
    }
    if let Some(transaction) = &participants {
        entities
            .validate_prepared(transaction)
            .map_err(io::Error::other)?;
    }
    let deltas = planned
        .edits
        .iter()
        .filter_map(|&(x, y, z, block)| {
            let (key, local) = crate::world::world_to_chunk(x, y, z);
            let version = planned
                .prepared
                .iter()
                .find(|edit| edit.key == key)?
                .new_version;
            Some(BlockDelta {
                key,
                local: local.map(|v| v as u8),
                version,
                block,
            })
        })
        .collect();
    Ok(Some(CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: reads.clone(),
        inventory_before: None,
        inventory: None,
        world_edits: planned.prepared,
        deltas,
        changed_cells: planned
            .edits
            .iter()
            .map(|&(x, y, z, _)| CellCoord::new(x, y, z))
            .collect(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        player_publication: None,
        entities: participants,
    }))
}

fn within_radius(key: ChunkKey, owner: ChunkKey, radius: u8) -> bool {
    let radius = i64::from(radius);
    (i64::from(key.x) - i64::from(owner.x)).abs() <= radius
        && (i64::from(key.y) - i64::from(owner.y)).abs() <= radius
        && (i64::from(key.z) - i64::from(owner.z)).abs() <= radius
}
