//! Persistent, owner-local background systems. Callbacks run on immutable input
//! and return bounded replacement state through the host's atomic owner journal.
//! Chunk owners may opt into a captured authoritative chunk view. Effects are
//! still a separate capability; reading terrain never grants write authority.
use crate::RegistrationError;
use crate::gameplay::{Block, Cell, Error};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Owner {
    Chunk([i32; 3]),
    Entity(u64),
    Profile(u128),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Partition {
    Chunk,
    Entity,
    Profile,
}

#[derive(Clone, Debug)]
pub struct Seed {
    pub owner: Owner,
    pub data: Vec<u8>,
}

pub struct Context<'a> {
    pub owner: Owner,
    pub revision: u64,
    pub tick: u64,
    pub data: &'a [u8],
    /// Available only for systems that declare `read_radius_chunks`.
    pub world: Option<&'a dyn WorldRead>,
}

/// Authoritative, bounded world input captured before the owner job starts.
/// An out-of-scope or unloaded cell is unavailable, never procedural air.
pub trait WorldRead {
    fn block(&self, cell: Cell) -> Result<Block, Error>;
}

impl Context<'_> {
    pub fn block(&self, cell: Cell) -> Result<Block, Error> {
        self.world.ok_or(Error::Unavailable(cell))?.block(cell)
    }
}

pub struct Plan {
    pub data: Vec<u8>,
    /// Absolute deadline, strictly later than the input tick.
    pub next_tick: u64,
    /// Durable, bounded invitations for another registered owner to run
    /// sooner. A wake carries no payload and cannot replace a world effect.
    pub wakes: Vec<Wake>,
    /// Bounded conditional block transitions. Sources must lie in this
    /// chunk owner's cells and require a captured world view. The host runs
    /// removal, placement and neighbor decisions before WAL admission.
    pub edits: Vec<BlockEdit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockEdit {
    pub cell: Cell,
    /// Exact namespaced block-state preimage observed through `Context::block`.
    pub before: String,
    /// Namespaced replacement state, resolved against the frozen catalog.
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wake {
    pub system: String,
    pub owner: Owner,
}

/// Pure and retryable. Validation runs for seeds, recovered values, and outputs.
/// Canonical bytes are the persistence contract; no Rust object identity is saved.
pub trait Behavior: Send + Sync + 'static {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError>;
    fn plan(&self, context: &Context<'_>) -> Result<Plan, RegistrationError>;
}

#[derive(Clone)]
pub struct System {
    pub key: String,
    pub schema: u64,
    pub partition: Partition,
    pub max_state_bytes: u32,
    pub max_jobs_per_tick: u16,
    /// Capture the authoritative owner chunk and optional immediate neighbors
    /// for read-only worker queries. `None` disables world reads; `Some(0)`
    /// captures only the owner; `Some(1)` captures all 27 surrounding chunks.
    /// The host bounds the job count and defers missing chunks before dispatch.
    pub read_radius_chunks: Option<u8>,
    /// Explicit simulation-phase dependencies, resolved at startup.
    pub after: Vec<String>,
    /// Initial durable owners. Recovered values always win over these seeds.
    pub seeds: Vec<Seed>,
    pub behavior: Arc<dyn Behavior>,
}

impl std::fmt::Debug for System {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("System")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl System {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let valid_key = |key: &str| {
            let mut parts = key.split(':');
            let valid = |s: &str| {
                !s.is_empty()
                    && s.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b)
                    })
            };
            key.len() <= 128
                && parts.next().is_some_and(valid)
                && parts.next().is_some_and(valid)
                && parts.next().is_none()
        };
        if !valid_key(&self.key)
            || !(1..=65536).contains(&self.max_state_bytes)
            || !(1..=16384).contains(&self.max_jobs_per_tick)
            || self.after.len() > 64
            || self
                .after
                .iter()
                .any(|key| !valid_key(key) || key == &self.key)
            || self.seeds.len() > 16384
            || (self.read_radius_chunks.is_some() && self.partition != Partition::Chunk)
            || self.read_radius_chunks.is_some_and(|radius| {
                radius > 1 || self.max_jobs_per_tick > if radius == 0 { 64 } else { 8 }
            })
        {
            return Err(RegistrationError("invalid owner-system declaration".into()));
        }
        let mut seen = std::collections::BTreeSet::new();
        for seed in &self.seeds {
            let matches = matches!(
                (self.partition, seed.owner),
                (Partition::Chunk, Owner::Chunk(_))
                    | (Partition::Entity, Owner::Entity(_))
                    | (Partition::Profile, Owner::Profile(_))
            );
            if !matches
                || !seen.insert(seed.owner)
                || seed.data.len() > self.max_state_bytes as usize
            {
                return Err(RegistrationError("invalid owner-system seed".into()));
            }
            self.behavior.validate(&seed.data)?;
        }
        Ok(())
    }

    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        fn text(out: &mut Vec<u8>, s: &str) {
            out.extend((s.len() as u32).to_le_bytes());
            out.extend(s.as_bytes());
        }
        // Preserve existing declarations' fingerprints; only systems opting
        // into the new read contract require a new manifest identity.
        let mut out = vec![match self.read_radius_chunks {
            None => 1,
            Some(0) => 2,
            Some(_) => 3,
        }];
        text(&mut out, &self.key);
        out.extend(self.schema.to_le_bytes());
        out.push(match self.partition {
            Partition::Chunk => 0,
            Partition::Entity => 1,
            Partition::Profile => 2,
        });
        out.extend(self.max_state_bytes.to_le_bytes());
        out.extend(self.max_jobs_per_tick.to_le_bytes());
        if let Some(radius @ 1..) = self.read_radius_chunks {
            out.push(radius);
        }
        let mut after = self.after.iter().collect::<Vec<_>>();
        after.sort();
        out.extend((after.len() as u32).to_le_bytes());
        for key in after {
            text(&mut out, key);
        }
        let mut seeds = self.seeds.iter().collect::<Vec<_>>();
        seeds.sort_by_key(|s| s.owner);
        out.extend((seeds.len() as u32).to_le_bytes());
        for seed in seeds {
            match seed.owner {
                Owner::Chunk(c) => {
                    out.push(0);
                    for v in c {
                        out.extend(v.to_le_bytes());
                    }
                }
                Owner::Entity(id) => {
                    out.push(1);
                    out.extend(id.to_le_bytes());
                }
                Owner::Profile(id) => {
                    out.push(2);
                    out.extend(id.to_le_bytes());
                }
            }
            out.extend((seed.data.len() as u32).to_le_bytes());
            out.extend(&seed.data);
        }
        out
    }
}
