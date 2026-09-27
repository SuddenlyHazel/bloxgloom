//! Translation from public gameplay plans to existing authoritative participants.
use super::durable::TerrainReads;
use crate::content::Catalog;
use crate::world::{BlockId, ChunkKey, PreparedEdit, World};
use bloxgloom_host_api::gameplay::{Block, Cell, Context, Error, Snapshot};
use std::io;

pub(super) fn block(catalog: &Catalog, id: BlockId) -> Result<Block, Error> {
    let state = catalog
        .state(id)
        .ok_or_else(|| Error::Host("unknown stored block state".into()))?;
    let definition = catalog
        .block_type(state.block_type)
        .ok_or_else(|| Error::Host("unknown stored block type".into()))?;
    Ok(Block {
        state: state.key.clone(),
        block_type: definition.key.to_string(),
        primary_item: catalog
            .primary_block_item(id)
            .and_then(|id| catalog.item(id))
            .map(|item| item.key.to_string()),
    })
}

pub(super) fn error(error: Error) -> io::Error {
    let kind = match error {
        Error::Unavailable(_) => io::ErrorKind::WouldBlock,
        Error::Host(_) => io::ErrorKind::InvalidData,
        _ => io::ErrorKind::InvalidInput,
    };
    io::Error::new(kind, error)
}

struct WorldSnapshot<'a> {
    world: &'a mut World,
    reads: &'a mut TerrainReads,
    requested: &'a mut Vec<ChunkKey>,
}
impl Snapshot for WorldSnapshot<'_> {
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        let [x, y, z] = cell;
        let id = self
            .reads
            .read(self.world, x, y, z)
            .map_err(|e| match e.kind() {
                io::ErrorKind::WouldBlock => Error::Unavailable(cell),
                io::ErrorKind::QuotaExceeded => Error::BudgetExceeded,
                _ => Error::Host(e.to_string()),
            })?;
        let Some(id) = id else {
            let key = crate::world::world_to_chunk(x, y, z).0;
            if !self.requested.contains(&key) {
                self.requested.push(key);
            }
            return Err(Error::Unavailable(cell));
        };
        block(self.world.catalog(), id)
    }
    fn state(&self, key: &str) -> Result<Block, Error> {
        let catalog = self.world.catalog();
        block(
            catalog,
            catalog
                .state_by_key(key)
                .ok_or_else(|| Error::UnknownContent(key.into()))?,
        )
    }
    fn item_exists(&self, key: &str) -> bool {
        self.world.catalog().item_by_key(key).is_some()
    }
}

/// Shared staged terrain path. Even blind writes acquire terrain dependencies;
/// the caller merges them into the same CommitAction as inventory/entities.
pub(super) fn prepare_edits(
    world: &mut World,
    reads: &mut TerrainReads,
    edits: &[(i32, i32, i32, BlockId)],
) -> io::Result<Vec<PreparedEdit>> {
    let catalog = world.catalog_arc();
    let mut requested = Vec::new();
    let mut snapshot = WorldSnapshot {
        world,
        reads,
        requested: &mut requested,
    };
    let mut context = Context::new(&mut snapshot, edits.len());
    for &(x, y, z, state) in edits {
        let state = catalog
            .state(state)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown block state"))?;
        context.set_block([x, y, z], &state.key).map_err(error)?;
    }
    let plan = context.finish().map_err(error)?;
    let edits = plan
        .blocks
        .into_iter()
        .map(|([x, y, z], key)| {
            catalog
                .state_by_key(&key)
                .map(|id| (x, y, z, id))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "staged state disappeared")
                })
        })
        .collect::<io::Result<Vec<_>>>()?;
    world.prepare_edits(&edits)
}

pub(super) type Edit = (i32, i32, i32, BlockId);
pub(super) type Spawn = ([f32; 3], crate::items::ItemId, u16, std::time::Duration);
pub(super) type Removal = (BlockId, Cell, bloxgloom_host_api::gameplay::RemovalCause);

pub(super) struct WorldPlan {
    pub edits: Vec<Edit>,
    pub prepared: Vec<PreparedEdit>,
    pub drops: Vec<Spawn>,
}

/// Invoke decision owners with one shared overlay. Every terrain/drop effect is
/// translated into participants of the caller's existing CommitAction.
pub(super) fn plan_removals(
    world: &mut World,
    reads: &mut TerrainReads,
    requested: &mut Vec<ChunkKey>,
    edits: &[Edit],
    removals: &[Removal],
    seed: u64,
) -> io::Result<WorldPlan> {
    use bloxgloom_host_api::gameplay::{Event, EventKind, cell_random};
    let catalog = world.catalog_arc();
    // Preparation is invisible. Its per-chunk version is stable random input
    // for existing harvest behavior; an expanded overlay is prepared below.
    let prepared = world.prepare_edits(edits)?;
    let mut snapshot = WorldSnapshot {
        world,
        reads,
        requested,
    };
    let mut context = Context::new(&mut snapshot, 4096);
    for &(x, y, z, state) in edits {
        let state = catalog
            .state(state)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown block state"))?;
        context.set_block([x, y, z], &state.key).map_err(error)?;
    }
    for &(id, cell, cause) in removals {
        let previous = block(&catalog, id).map_err(error)?;
        let Some(handler) = catalog.gameplay_handler(EventKind::BlockRemoved, &previous.block_type)
        else {
            continue;
        };
        let key = crate::world::world_to_chunk(cell[0], cell[1], cell[2]).0;
        let version = prepared
            .iter()
            .find(|edit| edit.key == key)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "removal outside staged edits")
            })?
            .new_version;
        let event = Event::BlockRemoved {
            cell,
            previous,
            cause,
            random: cell_random(seed, cell, version),
        };
        handler.handler.handle(&mut context, &event).map_err(|e| {
            let e = error(e);
            io::Error::new(e.kind(), format!("{}: {e}", handler.key))
        })?;
    }
    let plan = context.finish().map_err(error)?;
    let final_edits = plan
        .blocks
        .into_iter()
        .map(|([x, y, z], key)| {
            catalog
                .state_by_key(&key)
                .map(|id| (x, y, z, id))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "staged state disappeared")
                })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut original = edits.to_vec();
    original.sort_by_key(|&(x, y, z, _)| [x, y, z]);
    let prepared = if final_edits == original {
        prepared
    } else {
        world.prepare_edits(&final_edits)?
    };
    let drops = plan
        .drops
        .into_iter()
        .map(|drop| {
            catalog
                .item_by_key(&drop.item)
                .map(|item| {
                    (
                        drop.position,
                        item,
                        drop.count,
                        std::time::Duration::from_millis(u64::from(drop.pickup_delay_ms)),
                    )
                })
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "staged item disappeared")
                })
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(WorldPlan {
        edits: final_edits,
        prepared,
        drops,
    })
}
