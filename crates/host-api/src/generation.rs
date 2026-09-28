//! Language-independent chunk generation inputs and bounded, key-based outputs.
//! Contributors are pure chunk functions: the same context and content map must
//! produce the same writes, without consulting mutable world state or I/O.

use crate::RegistrationError;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const CHUNK_SIZE: i64 = 16;
pub const MAX_WRITES: usize = 16 * 16 * 16;

/// Host-provided, read-only built-in terrain sampling. Implementations must use
/// the same seed and terrain rules as the host's chunk generator.
pub trait TerrainSamples: Send + Sync + std::fmt::Debug {
    fn height(&self, seed: u64, x: i64, z: i64) -> i64;
    /// Base terrain only: no trees, plants, registered contributors or edits.
    fn base_block(&self, seed: u64, position: [i64; 3]) -> &'static str;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleError {
    OutOfBounds,
    Unavailable,
}

#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub seed: u64,
    pub chunk: [i32; 3],
    samples: Option<&'static dyn TerrainSamples>,
}

impl PartialEq for Context {
    fn eq(&self, other: &Self) -> bool {
        self.seed == other.seed
            && self.chunk == other.chunk
            && match (self.samples, other.samples) {
                (Some(a), Some(b)) => std::ptr::eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

impl Eq for Context {}

impl Context {
    pub const fn new(seed: u64, chunk: [i32; 3]) -> Self {
        Self {
            seed,
            chunk,
            samples: None,
        }
    }

    /// The host supplies its actual built-in generator; host-api never depends
    /// on the game crate. Contributors receive a context with these samples.
    pub const fn with_samples(
        seed: u64,
        chunk: [i32; 3],
        samples: &'static dyn TerrainSamples,
    ) -> Self {
        Self {
            seed,
            chunk,
            samples: Some(samples),
        }
    }

    /// Height of the built-in terrain column at absolute X/Z, including caves
    /// only in the base-block query. Valid coordinates are those addressable by
    /// i32 chunk keys: i32::MIN * 16 through i32::MAX * 16 + 15.
    pub fn builtin_terrain_height(&self, x: i64, z: i64) -> Result<i64, SampleError> {
        if !in_world_bounds(x) || !in_world_bounds(z) {
            return Err(SampleError::OutOfBounds);
        }
        Ok(self
            .samples
            .ok_or(SampleError::Unavailable)?
            .height(self.seed, x, z))
    }

    /// Built-in base terrain state key at absolute XYZ, before decorations,
    /// extensions or edits. Uses the same coordinate bounds as height samples.
    pub fn builtin_base_block(&self, position: [i64; 3]) -> Result<&'static str, SampleError> {
        if position.iter().any(|&axis| !in_world_bounds(axis)) {
            return Err(SampleError::OutOfBounds);
        }
        Ok(self
            .samples
            .ok_or(SampleError::Unavailable)?
            .base_block(self.seed, position))
    }

    /// Absolute coordinates use i64 so even chunks at i32 limits are exact.
    pub fn world_position(&self, local: [i32; 3]) -> Result<[i64; 3], GenerationError> {
        if local
            .iter()
            .any(|&axis| !(0..CHUNK_SIZE as i32).contains(&axis))
        {
            return Err(GenerationError::OutOfBounds(local));
        }
        Ok(std::array::from_fn(|axis| {
            i64::from(self.chunk[axis]) * CHUNK_SIZE + i64::from(local[axis])
        }))
    }

    /// Fixed integer mixing, independent of host language RNG and call order.
    pub fn random_at(&self, position: [i64; 3], salt: u64) -> u64 {
        let mut value = self.seed ^ salt;
        for (axis, coordinate) in position.into_iter().enumerate() {
            value ^= (coordinate as u64).wrapping_mul(
                [
                    0x9e37_79b9_7f4a_7c15,
                    0xbf58_476d_1ce4_e5b9,
                    0x94d0_49bb_1331_11eb,
                ][axis],
            );
            value = mix(value);
        }
        value
    }
}

fn in_world_bounds(coordinate: i64) -> bool {
    (i64::from(i32::MIN) * CHUNK_SIZE..=i64::from(i32::MAX) * CHUNK_SIZE + CHUNK_SIZE - 1)
        .contains(&coordinate)
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationError {
    OutOfBounds([i32; 3]),
    WriteLimit,
    InvalidState(String),
    Contributor(String),
}

/// Sparse output; each write consumes budget even when it replaces an earlier
/// write. A writer cannot address another chunk. Writes are local XYZ positions.
#[derive(Default)]
pub struct Output {
    writes: BTreeMap<u16, String>,
    count: usize,
    error: Option<GenerationError>,
}

impl Output {
    pub fn set(&mut self, local: [i32; 3], state_key: &str) -> Result<(), GenerationError> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if local
            .iter()
            .any(|&axis| !(0..CHUNK_SIZE as i32).contains(&axis))
        {
            self.error = Some(GenerationError::OutOfBounds(local));
        } else if state_key.is_empty() || state_key.len() > 255 {
            self.error = Some(GenerationError::InvalidState(
                "state key must contain 1–255 bytes".into(),
            ));
        } else if self.count >= MAX_WRITES {
            self.error = Some(GenerationError::WriteLimit);
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        self.count += 1;
        let [x, y, z] = local;
        let index = x + 16 * (z + 16 * y);
        self.writes.insert(index as u16, state_key.to_owned());
        Ok(())
    }

    /// The host checks this even if a contributor ignored an error from `set`.
    pub fn finish(&self) -> Result<(), GenerationError> {
        self.error.clone().map_or(Ok(()), Err)
    }

    pub fn writes(&self) -> impl Iterator<Item = (u16, &str)> {
        self.writes
            .iter()
            .map(|(&index, key)| (index, key.as_str()))
    }
}

pub trait Contributor: Send + Sync {
    fn generate(&self, context: Context, output: &mut Output) -> Result<(), GenerationError>;
}

#[derive(Clone)]
pub struct Registration {
    /// Stable namespaced key, unique among contributors. Order is lexical by key;
    /// later keys overwrite earlier keys at an overlapping cell.
    pub key: String,
    /// Persisted generation identity. Must be nonzero and must change whenever
    /// the algorithm, configuration, or state-key choices change its output.
    /// Native contributors are trusted to honor this and the purity contract.
    pub revision: u32,
    pub contributor: Arc<dyn Contributor>,
}

impl Registration {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if self.revision == 0 {
            return Err(RegistrationError(
                "generation revision must be nonzero".into(),
            ));
        }
        let Some((namespace, name)) = self.key.split_once(':') else {
            return Err(RegistrationError(
                "invalid generation contributor key".into(),
            ));
        };
        if self.key.len() > 255
            || namespace.is_empty()
            || name.is_empty()
            || !namespace
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_./-".contains(&b))
        {
            return Err(RegistrationError(
                "invalid generation contributor key".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "generation/tests.rs"]
mod tests;
