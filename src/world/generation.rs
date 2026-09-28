//! Frozen authoritative composition of builtin and registered contributors.
use super::{AIR, BlockId, CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkKey};
use crate::content::Catalog;
use bloxgloom_host_api::generation::{
    Context, Contributor, GenerationError, Output, Registration, TerrainSamples,
};
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

/// Builds a candidate chunk without touching storage. The builtin contributor runs
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
    let mut blocks = vec![AIR; CHUNK_VOLUME];
    let context = Context::with_samples(seed, [key.x, key.y, key.z], &BUILTIN_SAMPLES);
    apply(&Builtin, context, &mut blocks, catalog)?;
    for registration in ordered {
        apply(
            registration.contributor.as_ref(),
            context,
            &mut blocks,
            catalog,
        )?;
    }
    Ok(Chunk::from_blocks(key, 0, blocks))
}

fn apply(
    contributor: &dyn Contributor,
    context: Context,
    blocks: &mut [BlockId],
    catalog: &Catalog,
) -> Result<(), GenerationError> {
    let mut output = Output::default();
    contributor.generate(context, &mut output)?;
    output.finish()?;
    // Resolve all names before applying any writes from this contributor.
    let writes = output
        .writes()
        .map(|(index, state)| {
            catalog
                .state_by_key(state)
                .map(|block| (usize::from(index), block))
                .ok_or_else(|| GenerationError::InvalidState(state.to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (index, block) in writes {
        blocks[index] = block;
    }
    Ok(())
}

/// Terrain and vegetation share one bounded output so trees can retain their
/// original collision rules against the terrain and each other.
struct Builtin;

#[derive(Debug)]
struct BuiltinSamples;
static BUILTIN_SAMPLES: BuiltinSamples = BuiltinSamples;

impl TerrainSamples for BuiltinSamples {
    fn height(&self, seed: u64, x: i64, z: i64) -> i64 {
        super::terrain::terrain_height(x, z, seed)
    }

    fn base_block(&self, seed: u64, [x, y, z]: [i64; 3]) -> &'static str {
        let block = if y <= i64::from(super::BEDROCK_Y) {
            super::STONE
        } else if y > i64::from(super::MAX_GENERATED_HEIGHT) {
            AIR
        } else {
            let column = super::terrain::terrain_column(x, z, seed);
            super::terrain::generated_block_in_column(x, y, z, column, seed)
        };
        builtin_state_key(block).expect("built-in terrain returns registered built-in states")
    }
}

impl Contributor for Builtin {
    fn generate(&self, context: Context, output: &mut Output) -> Result<(), GenerationError> {
        let key = ChunkKey {
            x: context.chunk[0],
            y: context.chunk[1],
            z: context.chunk[2],
        };
        for (index, block) in super::terrain::generate_blocks(key, context.seed)
            .into_iter()
            .enumerate()
        {
            if block != AIR {
                output.set(
                    [
                        (index % CHUNK_SIZE) as i32,
                        (index / (CHUNK_SIZE * CHUNK_SIZE)) as i32,
                        ((index / CHUNK_SIZE) % CHUNK_SIZE) as i32,
                    ],
                    builtin_state_key(block)?,
                )?;
            }
        }
        Ok(())
    }
}

fn builtin_state_key(block: BlockId) -> Result<&'static str, GenerationError> {
    const KEYS: [&str; 16] = [
        "bloxgloom:air",
        "bloxgloom:grass",
        "bloxgloom:dirt",
        "bloxgloom:stone",
        "bloxgloom:sand",
        "bloxgloom:snow",
        "bloxgloom:moss",
        "bloxgloom:gravel",
        "bloxgloom:glowstone",
        "bloxgloom:wood[axis=y]",
        "bloxgloom:leaves",
        "bloxgloom:red_flower",
        "bloxgloom:yellow_flower",
        "bloxgloom:blue_flower",
        "bloxgloom:fern",
        "bloxgloom:tall_grass",
    ];
    KEYS.get(block.0 as usize).copied().ok_or_else(|| {
        GenerationError::Contributor(format!("unknown builtin generation block ID {}", block.0))
    })
}

/// The standalone builtin preview follows the same contributor/composition path
/// as authoritative generation, with no extension registrations.
pub fn generate_chunk(key: ChunkKey, seed: u64) -> Chunk {
    compose(key, seed, crate::content::catalog(), &[]).expect("builtin generation is valid")
}

#[cfg(test)]
mod tests;
