//! Frozen authoritative composition over the unchanged builtin terrain baseline.
use super::{Chunk, ChunkKey};
use crate::content::Catalog;
use bloxgloom_host_api::generation::{Context, GenerationError, Output, Registration};
use std::collections::BTreeSet;
use std::io;

pub(crate) const MAX_GENERATION_IDENTITY_BYTES: usize = 2 + 256 * (1 + 255 + 4);

#[derive(Default)]
pub(crate) struct Generator {
    contributors: Vec<Registration>,
}

impl Generator {
    pub(crate) fn new(mut contributors: Vec<Registration>) -> io::Result<Self> {
        validate(&contributors).map_err(generation_error)?;
        contributors.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(Self { contributors })
    }

    /// Canonical identity embedded in world.meta, not a separate save store.
    pub(crate) fn identity(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(self.contributors.len() as u16).to_le_bytes());
        for entry in &self.contributors {
            bytes.push(entry.key.len() as u8);
            bytes.extend_from_slice(entry.key.as_bytes());
            bytes.extend_from_slice(&entry.revision.to_le_bytes());
        }
        bytes
    }

    pub(super) fn has_contributors(&self) -> bool {
        !self.contributors.is_empty()
    }

    pub(super) fn generate(
        &self,
        key: ChunkKey,
        seed: u64,
        catalog: &Catalog,
    ) -> io::Result<Chunk> {
        compose(key, seed, catalog, &self.contributors).map_err(generation_error)
    }
}

fn generation_error(error: GenerationError) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("chunk generation failed: {error:?}"),
    )
}

/// Builds a candidate chunk without touching storage. The builtin terrain runs
/// first. Contributors run in lexical key order (independent of registration
/// order); later keys win overlaps, including over builtin terrain. An error
/// discards the entire candidate, rather than exposing a partially built chunk.
#[cfg(test)]
pub fn generate_chunk_with_contributors(
    key: ChunkKey,
    seed: u64,
    catalog: &Catalog,
    contributors: &[Registration],
) -> Result<Chunk, GenerationError> {
    validate(contributors)?;
    let mut ordered = contributors.to_vec();
    ordered.sort_by(|a, b| a.key.cmp(&b.key));
    compose(key, seed, catalog, &ordered)
}

fn validate(contributors: &[Registration]) -> Result<(), GenerationError> {
    if contributors.len() > 256 {
        return Err(GenerationError::Contributor("too many contributors".into()));
    }
    let mut seen = BTreeSet::new();
    for registration in contributors {
        registration
            .validate()
            .map_err(|error| GenerationError::Contributor(error.0))?;
        if !seen.insert(&registration.key) {
            return Err(GenerationError::Contributor(format!(
                "duplicate generation contributor {}",
                registration.key
            )));
        }
    }
    Ok(())
}

fn compose(
    key: ChunkKey,
    seed: u64,
    catalog: &Catalog,
    ordered: &[Registration],
) -> Result<Chunk, GenerationError> {
    let mut chunk = super::generate_chunk(key, seed);
    let context = Context {
        seed,
        chunk: [key.x, key.y, key.z],
    };
    for registration in ordered {
        let mut output = Output::default();
        registration.contributor.generate(context, &mut output)?;
        output.finish()?;
        // Resolve all names before applying any writes from this contributor.
        for (_, state) in output.writes() {
            if catalog.state_by_key(state).is_none() {
                return Err(GenerationError::InvalidState(state.to_owned()));
            }
        }
        for (index, state) in output.writes() {
            let block = catalog.state_by_key(state).expect("validated state key");
            chunk.blocks.set(usize::from(index), block);
        }
    }
    Ok(chunk)
}

#[cfg(test)]
mod tests;
