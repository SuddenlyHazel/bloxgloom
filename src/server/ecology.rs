//! Gradual builtin vegetation rules. All edits use normal durable admission.
pub(in crate::server) mod rules;
mod schedule;
use super::{
    State,
    durable::{DurableRequest, MAX_DEFERRED_DURABLE_ACTIONS, TerrainReads},
    simulation::TickId,
    streaming,
};
use crate::world::{self, ChunkKey};
pub(in crate::server) use rules::Rule;
pub(in crate::server) use schedule::Runtime;
use std::{collections::BTreeSet, io};

pub(in crate::server) fn advance(state: &mut State, tick: TickId) -> io::Result<()> {
    if tick.get() % 50 == 1 {
        let mut keys = BTreeSet::new();
        for client in state.clients.values() {
            let radius = i32::from(client.radius.min(2));
            for dy in -2..=2 {
                for dz in -radius..=radius {
                    for dx in -radius..=radius {
                        if let (Some(x), Some(y), Some(z)) = (
                            client.center.x.checked_add(dx),
                            client.center.y.checked_add(dy),
                            client.center.z.checked_add(dz),
                        ) {
                            let key = ChunkKey { x, y, z };
                            if state.world.cached_version(key).is_some() {
                                keys.insert(key);
                            }
                        }
                    }
                }
            }
        }
        state.ecology.refresh(keys);
    }
    let sun_up = rules::daylight(state.world_time.now());
    let mut missing = Vec::new();
    for cell in state.ecology.samples() {
        // Non-ecological voxels do not need a read-stamp allocation.
        if !state
            .world
            .cached_block(cell[0], cell[1], cell[2])
            .is_some_and(|block| {
                matches!(block, world::DIRT | world::GRASS)
                    || crate::content::jg_rtx::is_leaf(state.world.catalog(), block)
            })
        {
            continue;
        }
        let mut reads = TerrainReads::default();
        match rules::check(&mut state.world, &mut reads, &mut missing, cell, sun_up) {
            Ok(rule) => state.ecology.observe(cell, rule, tick.get(), state.seed),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
    }
    // At most one missing chunk per check; the loader has its own fixed budget.
    for key in missing {
        let _ = streaming::request_chunk(state, key)?;
    }
    for _ in 0..schedule::DUE_PER_TICK {
        if state.durability.queued.len() >= MAX_DEFERRED_DURABLE_ACTIONS {
            break;
        }
        let Some((cell, rule)) = state.ecology.take_due(tick.get()) else {
            break;
        };
        state
            .durability
            .queued
            .push_back(DurableRequest::Ecology { cell, rule });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
