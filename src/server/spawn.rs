//! Safe origin spawn selection. Startup may load terrain synchronously; live
//! joins consult only resident authoritative chunks and request cache misses.

use super::{State, streaming};
use crate::world::{BEDROCK_Y, MAX_GENERATED_HEIGHT, World, world_to_chunk};
use std::io::{self, ErrorKind};

const HEADROOM: i32 = 32;
const MAX_SPAWN_RISE: i32 = 128;

pub(super) fn spawn_position(world: &mut World) -> io::Result<[f32; 3]> {
    let ceiling = MAX_GENERATED_HEIGHT + HEADROOM;
    let catalog = world.catalog_arc();
    let solid = |block| catalog.block_flags(block) & crate::content::SOLID != 0;
    if solid(world.get_block(0, ceiling, 0)?) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "spawn terrain exceeds scan ceiling",
        ));
    }
    for y in (BEDROCK_Y..ceiling).rev() {
        if solid(world.get_block(0, y, 0)?) {
            let position = [0.5, (y + 1) as f32, 0.5];
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
/// key is requested and the join waits for a later tick.
pub(super) fn spawn_position_cached(state: &mut State) -> io::Result<[f32; 3]> {
    let start_y = state.spawn_anchor[1] as i32;
    let catalog = state.world.catalog_arc();
    let mut missing = false;
    // Keep the historical preference for the original/upward surface, but
    // mining it must not strand later joins when a safe lower surface exists.
    for y in (start_y..start_y + MAX_SPAWN_RISE).chain((BEDROCK_Y + 1..start_y).rev()) {
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
        let position = [0.5, y as f32, 0.5];
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

fn collides_cached(state: &mut State, feet: [f32; 3]) -> io::Result<Option<bool>> {
    let mut missing = false;
    for x in [feet[0] - 0.3, feet[0] + 0.3] {
        for y in [feet[1] + 0.05, feet[1] + 0.9, feet[1] + 1.75] {
            for z in [feet[2] - 0.3, feet[2] + 0.3] {
                let cell = [x.floor() as i32, y.floor() as i32, z.floor() as i32];
                match state.world.cached_block(cell[0], cell[1], cell[2]) {
                    Some(block)
                        if state.world.catalog().block_flags(block) & crate::content::SOLID
                            != 0 =>
                    {
                        return Ok(Some(true));
                    }
                    Some(_) => {}
                    None => {
                        request_missing(state, cell)?;
                        missing = true;
                    }
                }
            }
        }
    }
    Ok((!missing).then_some(false))
}

fn request_missing(state: &mut State, cell: [i32; 3]) -> io::Result<()> {
    let (key, _) = world_to_chunk(cell[0], cell[1], cell[2]);
    let _ = streaming::request_chunk(state, key)?;
    Ok(())
}

pub(super) fn collides(world: &mut World, feet: [f32; 3]) -> io::Result<bool> {
    let catalog = world.catalog_arc();
    for x in [feet[0] - 0.3, feet[0] + 0.3] {
        for y in [feet[1] + 0.05, feet[1] + 0.9, feet[1] + 1.75] {
            for z in [feet[2] - 0.3, feet[2] + 0.3] {
                if catalog.block_flags(world.get_block(
                    x.floor() as i32,
                    y.floor() as i32,
                    z.floor() as i32,
                )?) & crate::content::SOLID
                    != 0
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
