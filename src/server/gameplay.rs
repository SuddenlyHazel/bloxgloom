//! Translation from public gameplay plans to existing authoritative participants.
use super::durable::TerrainReads;
use crate::content::Catalog;
use crate::world::{BlockId, PreparedEdit, World};
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
}
impl Snapshot for WorldSnapshot<'_> {
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        let [x, y, z] = cell;
        let id = self
            .reads
            .read(self.world, x, y, z)
            .map_err(|e| Error::Host(e.to_string()))?
            .ok_or(Error::Unavailable(cell))?;
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
        self.world.catalog().items().any(|item| item.key == key)
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
    let mut snapshot = WorldSnapshot { world, reads };
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
