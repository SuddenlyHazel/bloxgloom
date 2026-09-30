//! Exact read-only dependencies for world gameplay planners. Admission reserves
//! these keys until confirmed apply, so reads of support or empty space cannot
//! race a writer in another chunk while a journal receipt is outstanding.
use super::*;
use crate::world::{ChunkReadStamp, World};
#[derive(Clone, Debug, Default)]
pub(in crate::server) struct TerrainReads {
    pub clock: Option<crate::server::world_time::ReadStamp>,
    terrain: BTreeMap<ChunkKey, ChunkReadStamp>,
    entities: super::super::entities::EntityDependencies,
}
impl TerrainReads {
    pub fn entities(
        &mut self,
        dependencies: super::super::entities::EntityDependencies,
    ) -> io::Result<()> {
        self.entities
            .merge(dependencies)
            .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))
    }
    pub fn entities_current(&self, store: &super::super::entities::EntityStore) -> bool {
        self.entities.is_current(store)
    }
    pub fn read(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
    ) -> io::Result<Option<crate::world::BlockId>> {
        let Some(block) = world.cached_block(x, y, z) else {
            return Ok(None);
        };
        let key = crate::world::world_to_chunk(x, y, z).0;
        if let Some(stamp) = self.terrain.get(&key) {
            if !stamp.is_current() {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "terrain read changed during planning",
                ));
            }
        } else {
            if self.terrain.len() >= 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "terrain read budget exceeded",
                ));
            }
            self.terrain.insert(
                key,
                world
                    .cached_read_stamp(key)
                    .expect("resident read has authority stamp"),
            );
        }
        Ok(Some(block))
    }
    pub fn extend(&mut self, other: Self) -> io::Result<()> {
        if !self.is_current() || !other.is_current() {
            return Err(io::Error::new(ErrorKind::WouldBlock, "stale terrain read"));
        }
        self.entities(other.entities)?;
        if self.clock.is_none() {
            self.clock = other.clock;
        }
        for (key, stamp) in other.terrain {
            if !self.terrain.contains_key(&key) && self.terrain.len() >= 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "terrain read budget exceeded",
                ));
            }
            self.terrain.entry(key).or_insert(stamp);
        }
        Ok(())
    }
    pub fn is_current(&self) -> bool {
        self.terrain.values().all(ChunkReadStamp::is_current)
            && self
                .clock
                .as_ref()
                .is_none_or(crate::server::world_time::ReadStamp::is_current)
    }
    pub fn is_empty(&self) -> bool {
        self.terrain.is_empty() && self.entities.is_empty() && self.clock.is_none()
    }
    pub fn keys(&self) -> impl Iterator<Item = StateKey> + '_ {
        self.terrain
            .keys()
            .copied()
            .map(chunk_state_key)
            .chain(self.entities.keys())
            .chain(
                self.clock
                    .iter()
                    .map(|_| crate::server::world_time::state_key()),
            )
    }
}
