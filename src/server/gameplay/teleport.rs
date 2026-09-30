//! Teleport collision reads fence final terrain, including this plan's edits.
use super::*;
use bloxgloom_host_api::gameplay::{PlayerOperation, PlayerOperationKind};

pub(super) fn validate(
    world: &mut World,
    reads: &mut TerrainReads,
    requested: &mut Vec<ChunkKey>,
    operations: &[PlayerOperation],
    edits: &[(i32, i32, i32, BlockId)],
) -> io::Result<()> {
    let catalog = world.catalog_arc();
    for operation in operations {
        let PlayerOperationKind::Teleport(position) = operation.kind else {
            continue;
        };
        if position
            .iter()
            .any(|v| !v.is_finite() || v.abs() >= 1_000_000.0)
            || position[1] <= crate::world::BEDROCK_Y as f32
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid player teleport position",
            ));
        }
        let blocked = catalog
            .player_rules()
            .body()
            .collides(position, |x, y, z| {
                let Some(before) = reads.read(world, x, y, z)? else {
                    let key = crate::world::world_to_chunk(x, y, z).0;
                    if !requested.contains(&key) {
                        requested.push(key);
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "teleport terrain unavailable",
                    ));
                };
                let block = edits
                    .iter()
                    .find(|&&(ex, ey, ez, _)| [ex, ey, ez] == [x, y, z])
                    .map_or(before, |e| e.3);
                Ok::<_, io::Error>(catalog.block_flags(block) & crate::content::SOLID != 0)
            })?;
        if blocked {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "teleport destination is obstructed",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "teleport/tests.rs"]
mod tests;
