//! Safe origin spawn selection. Startup may load terrain synchronously; live
//! joins consult only resident authoritative chunks and request cache misses.

use super::{State, streaming};
use crate::world::{BEDROCK_Y, MAX_GENERATED_HEIGHT, World, world_to_chunk};
use std::io::{self, ErrorKind};

pub(super) fn spawn_position(world: &mut World) -> io::Result<[f32; 3]> {
    let catalog = world.catalog_arc();
    let spawn = catalog.player_rules().spawn();
    let ceiling = spawn.ceiling(MAX_GENERATED_HEIGHT);
    let solid = |block| catalog.block_flags(block) & crate::content::SOLID != 0;
    if solid(world.get_block(0, ceiling, 0)?) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "spawn terrain exceeds scan ceiling",
        ));
    }
    for y in spawn.startup_support_levels(BEDROCK_Y, MAX_GENERATED_HEIGHT) {
        if solid(world.get_block(0, y, 0)?) {
            let position = spawn.feet(y + 1);
            if !collides(world, position)? {
                return Ok(position);
            }
        }
    }
    Err(io::Error::new(
        ErrorKind::InvalidData,
        "no safe spawn at world origin",
    ))
}

/// Rechecks the startup surface, then searches upward and downward after edits.
/// A missing authoritative chunk never becomes guessed air or ground: its exact
/// key is requested; the join waits unless a later candidate is known safe.
pub(super) fn spawn_position_cached(state: &mut State) -> io::Result<[f32; 3]> {
    let start_y = state.spawn_anchor[1] as i32;
    let catalog = state.world.catalog_arc();
    let mut missing = false;
    // Keep the historical preference for the original/upward surface, but
    // mining it must not strand later joins when a safe lower surface exists.
    for y in catalog
        .player_rules()
        .spawn()
        .cached_feet_levels(start_y, BEDROCK_Y)
    {
        let support = match state.world.cached_block(0, y - 1, 0) {
            Some(block) => block,
            None => {
                request_missing(state, [0, y - 1, 0])?;
                missing = true;
                continue;
            }
        };
        if catalog.block_flags(support) & crate::content::SOLID == 0 {
            continue;
        }
        let position = catalog.player_rules().spawn().feet(y);
        match collides_cached(state, position)? {
            Some(false) => return Ok(position),
            Some(true) => {}
            None => missing = true,
        }
    }
    if missing {
        Err(io::Error::new(
            ErrorKind::WouldBlock,
            "spawn chunks are loading",
        ))
    } else {
        Err(io::Error::new(
            ErrorKind::InvalidData,
            "no safe spawn near origin surface",
        ))
    }
}

pub(super) fn collides_cached(state: &mut State, feet: [f32; 3]) -> io::Result<Option<bool>> {
    let mut missing = false;
    let collides = state.world.catalog().player_rules().body().collides(
        feet,
        |x, y, z| -> io::Result<bool> {
            let cell = [x, y, z];
            match state.world.cached_block(x, y, z) {
                Some(block) => {
                    Ok(state.world.catalog().block_flags(block) & crate::content::SOLID != 0)
                }
                None => {
                    request_missing(state, cell)?;
                    missing = true;
                    Ok(false)
                }
            }
        },
    )?;
    Ok(if collides {
        Some(true)
    } else {
        (!missing).then_some(false)
    })
}

fn request_missing(state: &mut State, cell: [i32; 3]) -> io::Result<()> {
    let (key, _) = world_to_chunk(cell[0], cell[1], cell[2]);
    let _ = streaming::request_chunk(state, key)?;
    Ok(())
}

pub(super) fn collides(world: &mut World, feet: [f32; 3]) -> io::Result<bool> {
    let catalog = world.catalog_arc();
    catalog.player_rules().body().collides(feet, |x, y, z| {
        Ok(catalog.block_flags(world.get_block(x, y, z)?) & crate::content::SOLID != 0)
    })
}
