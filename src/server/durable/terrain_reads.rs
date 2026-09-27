//! Exact read-only chunk dependencies for block planners. Admission reserves
//! these keys until confirmed apply, so reads of support or empty space cannot
//! race a writer in another chunk while a journal receipt is outstanding.
use super::*;
use crate::world::{ChunkReadStamp, World};
#[derive(Clone, Default)]
pub(in crate::server) struct TerrainReads(BTreeMap<ChunkKey, ChunkReadStamp>);
impl TerrainReads {
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
        if let Some(stamp) = self.0.get(&key) {
            if !stamp.is_current() {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "terrain read changed during planning",
                ));
            }
        } else {
            if self.0.len() >= 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "terrain read budget exceeded",
                ));
            }
            self.0.insert(
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
        for (key, stamp) in other.0 {
            if !self.0.contains_key(&key) && self.0.len() >= 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "terrain read budget exceeded",
                ));
            }
            self.0.entry(key).or_insert(stamp);
        }
        Ok(())
    }
    pub fn is_current(&self) -> bool {
        self.0.values().all(ChunkReadStamp::is_current)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn keys(&self) -> impl Iterator<Item = StateKey> + '_ {
        self.0.keys().copied().map(chunk_state_key)
    }
}
