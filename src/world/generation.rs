//! Opt-in composition over the unchanged builtin terrain baseline.
use super::{Chunk, ChunkKey};
use crate::content::Catalog;
use bloxgloom_host_api::generation::{Context, GenerationError, Output, Registration};
use std::collections::BTreeSet;

/// Builds a candidate chunk without touching storage. The builtin terrain runs
/// first. Contributors run in lexical key order (independent of registration
/// order); later keys win overlaps, including over builtin terrain. An error
/// discards the entire candidate, rather than exposing a partially built chunk.
/// This is opt-in until world edit baselines and persisted generator identity
/// can use the same composition as authoritative chunk loading.
#[allow(
    dead_code,
    reason = "Live generation waits for persistent baseline support"
)]
pub fn generate_chunk_with_contributors(
    key: ChunkKey,
    seed: u64,
    catalog: &Catalog,
    contributors: &[Registration],
) -> Result<Chunk, GenerationError> {
    if contributors.len() > 256 {
        return Err(GenerationError::Contributor("too many contributors".into()));
    }
    let mut ordered = contributors.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.key.cmp(&b.key));
    let mut seen = BTreeSet::new();
    for registration in &ordered {
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
