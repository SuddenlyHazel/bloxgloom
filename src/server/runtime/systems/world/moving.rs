//! Moving spawn admission checks full body volume and global/chunk capacity.
use super::*;
use crate::content::EntityTypeId;
use crate::server::entities::EntityLocation;
use bloxgloom_host_api::motion::Record;
pub(super) struct Spawn<'a> {
    pub world: &'a mut World,
    pub store: &'a EntityStore,
    pub position: [f32; 3],
    pub entity_type: EntityTypeId,
    pub bytes: &'a [u8],
    pub players: &'a [[f32; 3]],
    pub edits: &'a [(i32, i32, i32, crate::content::BlockStateId)],
    pub reads: &'a mut TerrainReads,
    pub missing: &'a mut Vec<ChunkKey>,
}
pub(super) fn validate(
    Spawn {
        world,
        store,
        position,
        entity_type,
        bytes,
        players,
        edits,
        reads,
        missing,
    }: Spawn<'_>,
) -> io::Result<()> {
    let catalog = world.catalog_arc();
    let Some(d) = catalog.moving_entity(entity_type) else {
        return Ok(());
    };
    let record = Record::decode(bytes).map_err(|e| io::Error::new(ErrorKind::InvalidInput, e.0))?;
    let half = d.capture_half_extents();
    for x in (position[0] - half[0]).floor() as i32..=(position[0] + half[0]).ceil() as i32 - 1 {
        for y in (position[1] - half[1]).floor() as i32..=(position[1] + half[1]).ceil() as i32 - 1
        {
            for z in
                (position[2] - half[2]).floor() as i32..=(position[2] + half[2]).ceil() as i32 - 1
            {
                let Some(before) = reads.read(world, x, y, z)? else {
                    let key = crate::world::world_to_chunk(x, y, z).0;
                    if missing.len() < 8 && !missing.contains(&key) {
                        missing.push(key);
                    }
                    return Err(io::Error::new(
                        ErrorKind::WouldBlock,
                        "moving body terrain unavailable",
                    ));
                };
                let block = edits
                    .iter()
                    .find(|&&(a, b, c, _)| [a, b, c] == [x, y, z])
                    .map_or(before, |edit| edit.3);
                if catalog.block_flags(block) & crate::content::SOLID != 0 {
                    return Err(io::Error::new(
                        ErrorKind::PermissionDenied,
                        "moving body spawn obstructed",
                    ));
                }
            }
        }
    }
    let overlap = |min: [f32; 3], max: [f32; 3]| {
        (0..3).all(|i| position[i] - half[i] < max[i] && position[i] + half[i] > min[i])
    };
    if d.body.collisions.players {
        let b = catalog.player_rules().body();
        if players.iter().any(|p| {
            overlap(
                [p[0] - b.half_width, p[1], p[2] - b.half_width],
                [
                    p[0] + b.half_width,
                    p[1] + b.head_height,
                    p[2] + b.half_width,
                ],
            )
        }) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "moving spawn overlaps player",
            ));
        }
    }
    if d.body.collisions.creatures {
        let radius = half.into_iter().fold(0.0_f32, f32::max) + 4.0;
        reads.entities(
            store
                .capture_mobile_dependencies(position, radius)
                .map_err(io::Error::other)?,
        )?;
        for id in store
            .query_mobile_aabb(position.map(|v| v - radius), position.map(|v| v + radius))
            .map_err(io::Error::other)?
        {
            let Some(snapshot) = store.snapshot(id) else {
                continue;
            };
            let EntityLocation::Mobile { position: p } = snapshot.location else {
                continue;
            };
            if (0..3).any(|i| (p[i] - position[i]).abs() > radius) {
                continue;
            }
            if record.source_ticks > 0 && record.source == Some(id.get()) {
                continue;
            }
            let Some(body) = catalog.mobile_entity(snapshot.entity_type).map(|d| d.body) else {
                continue;
            };
            if overlap(
                [p[0] - body.half_width, p[1], p[2] - body.half_width],
                [
                    p[0] + body.half_width,
                    p[1] + body.height,
                    p[2] + body.half_width,
                ],
            ) {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "moving spawn overlaps creature",
                ));
            }
        }
    }
    Ok(())
}
