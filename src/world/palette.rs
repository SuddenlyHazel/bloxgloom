//! Adaptive storage for one chunk's state IDs.
//!
//! A valid 4,096-cell chunk is uniform or uses a local `u8`/`u16` palette.
//! The direct variant exists only to let validation report malformed chunks
//! with a wrong cell count instead of panicking during construction.

use std::ops::Index;
use std::ops::Range;

use super::{BlockId, CHUNK_VOLUME};

#[derive(Clone, Debug)]
pub enum PalettedBlocks {
    Uniform {
        state: BlockId,
        len: usize,
    },
    Palette8 {
        palette: Vec<BlockId>,
        indices: Vec<u8>,
        counts: Vec<u16>,
    },
    Palette16 {
        palette: Vec<BlockId>,
        indices: Vec<u16>,
        counts: Vec<u16>,
    },
    InvalidLength(Vec<BlockId>),
}

/// Borrowed representation for loops that should branch once per chunk, then
/// perform only a local-index lookup for each cell.
#[derive(Clone, Copy, Debug)]
pub enum PaletteView<'a> {
    Uniform(BlockId),
    Palette8 {
        palette: &'a [BlockId],
        indices: &'a [u8],
    },
    Palette16 {
        palette: &'a [BlockId],
        indices: &'a [u16],
    },
    InvalidLength(&'a [BlockId]),
}

impl PalettedBlocks {
    pub fn uniform(state: BlockId) -> Self {
        Self::Uniform {
            state,
            len: CHUNK_VOLUME,
        }
    }

    pub fn from_blocks(blocks: Vec<BlockId>) -> Self {
        if blocks.len() != CHUNK_VOLUME {
            return Self::InvalidLength(blocks);
        }
        if blocks.iter().all(|&state| state == blocks[0]) {
            return Self::uniform(blocks[0]);
        }
        let mut palette = Vec::new();
        let mut counts = Vec::<u16>::new();
        let mut by_state = std::collections::HashMap::new();
        let mut indices = Vec::with_capacity(CHUNK_VOLUME);
        for state in blocks {
            let index = if let Some(&index) = by_state.get(&state) {
                index
            } else {
                let index = palette.len();
                palette.push(state);
                counts.push(0);
                by_state.insert(state, index);
                index
            };
            counts[index] += 1;
            indices.push(index as u16);
        }
        if palette.len() == 1 {
            Self::uniform(palette[0])
        } else if palette.len() <= 256 {
            Self::Palette8 {
                palette,
                indices: indices.into_iter().map(|index| index as u8).collect(),
                counts,
            }
        } else {
            Self::Palette16 {
                palette,
                indices,
                counts,
            }
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Uniform { len, .. } => *len,
            Self::Palette8 { indices, .. } => indices.len(),
            Self::Palette16 { indices, .. } => indices.len(),
            Self::InvalidLength(blocks) => blocks.len(),
        }
    }

    pub fn unique_states(&self) -> usize {
        match self {
            Self::Uniform { .. } => 1,
            Self::Palette8 { palette, .. } | Self::Palette16 { palette, .. } => palette.len(),
            Self::InvalidLength(blocks) => blocks
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
        }
    }

    pub fn view(&self) -> PaletteView<'_> {
        match self {
            Self::Uniform { state, .. } => PaletteView::Uniform(*state),
            Self::Palette8 {
                palette, indices, ..
            } => PaletteView::Palette8 { palette, indices },
            Self::Palette16 {
                palette, indices, ..
            } => PaletteView::Palette16 { palette, indices },
            Self::InvalidLength(blocks) => PaletteView::InvalidLength(blocks),
        }
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<BlockId> {
        self.get_ref(index).copied()
    }

    #[inline]
    fn get_ref(&self, index: usize) -> Option<&BlockId> {
        match self {
            Self::Uniform { state, len } => (index < *len).then_some(state),
            Self::Palette8 {
                palette, indices, ..
            } => indices.get(index).map(|&local| &palette[local as usize]),
            Self::Palette16 {
                palette, indices, ..
            } => indices.get(index).map(|&local| &palette[local as usize]),
            Self::InvalidLength(blocks) => blocks.get(index),
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &BlockId> {
        (0..self.len()).map(|index| self.get_ref(index).expect("index below len"))
    }

    #[cfg(test)]
    pub fn to_vec(&self) -> Vec<BlockId> {
        self.iter().copied().collect()
    }

    /// Copies a contiguous row into a flat work buffer, branching on the
    /// representation once rather than once for each voxel.
    pub fn copy_range_to(&self, range: Range<usize>, target: &mut [BlockId]) -> Option<()> {
        if range.end > self.len() || range.start > range.end || target.len() != range.len() {
            return None;
        }
        match self.view() {
            PaletteView::Uniform(state) => target.fill(state),
            PaletteView::Palette8 { palette, indices } => {
                for (out, &local) in target.iter_mut().zip(&indices[range]) {
                    *out = palette[local as usize];
                }
            }
            PaletteView::Palette16 { palette, indices } => {
                for (out, &local) in target.iter_mut().zip(&indices[range]) {
                    *out = palette[local as usize];
                }
            }
            PaletteView::InvalidLength(blocks) => target.copy_from_slice(&blocks[range]),
        }
        Some(())
    }

    /// Sets a cell without expanding the whole chunk. Removing a palette entry
    /// rewrites local indices once; ordinary edits touch one index and count.
    pub fn set(&mut self, index: usize, state: BlockId) -> Option<BlockId> {
        let previous = self.get(index)?;
        if previous == state {
            return Some(previous);
        }
        if let Self::Uniform { len, .. } = self {
            let len = *len;
            if len == 1 {
                *self = Self::Uniform { state, len };
            } else {
                let mut indices = vec![0u8; len];
                indices[index] = 1;
                *self = Self::Palette8 {
                    palette: vec![previous, state],
                    indices,
                    counts: vec![(len - 1) as u16, 1],
                };
            }
            return Some(previous);
        }
        if let Self::Palette8 {
            palette,
            indices,
            counts,
        } = self
            && palette.len() == 256
            && !palette.contains(&state)
            && counts[indices[index] as usize] > 1
        {
            *self = Self::Palette16 {
                palette: std::mem::take(palette),
                indices: std::mem::take(indices).into_iter().map(u16::from).collect(),
                counts: std::mem::take(counts),
            };
        }
        match self {
            Self::Palette8 {
                palette,
                indices,
                counts,
            } => set_palette_cell(palette, indices, counts, index, state),
            Self::Palette16 {
                palette,
                indices,
                counts,
            } => set_palette_cell(palette, indices, counts, index, state),
            Self::InvalidLength(blocks) => blocks[index] = state,
            Self::Uniform { .. } => unreachable!(),
        }
        match self {
            Self::Palette8 {
                palette, indices, ..
            } if palette.len() == 1 => {
                *self = Self::Uniform {
                    state: palette[0],
                    len: indices.len(),
                };
            }
            Self::Palette16 {
                palette,
                indices,
                counts,
            } if palette.len() <= 256 => {
                *self = Self::Palette8 {
                    palette: std::mem::take(palette),
                    indices: std::mem::take(indices)
                        .into_iter()
                        .map(|index| index as u8)
                        .collect(),
                    counts: std::mem::take(counts),
                };
                if let Self::Palette8 {
                    palette, indices, ..
                } = self
                    && palette.len() == 1
                {
                    *self = Self::Uniform {
                        state: palette[0],
                        len: indices.len(),
                    };
                }
            }
            _ => {}
        }
        Some(previous)
    }
}

trait LocalIndex: Copy + Eq {
    fn as_usize(self) -> usize;
    fn from_usize(index: usize) -> Self;
}

impl LocalIndex for u8 {
    fn as_usize(self) -> usize {
        self as usize
    }
    fn from_usize(index: usize) -> Self {
        index as Self
    }
}

impl LocalIndex for u16 {
    fn as_usize(self) -> usize {
        self as usize
    }
    fn from_usize(index: usize) -> Self {
        index as Self
    }
}

fn set_palette_cell<I: LocalIndex>(
    palette: &mut Vec<BlockId>,
    indices: &mut [I],
    counts: &mut Vec<u16>,
    cell: usize,
    state: BlockId,
) {
    let old = indices[cell].as_usize();
    if let Some(new) = palette.iter().position(|&candidate| candidate == state) {
        indices[cell] = I::from_usize(new);
        counts[new] += 1;
        counts[old] -= 1;
        if counts[old] == 0 {
            let last = palette.len() - 1;
            palette.swap_remove(old);
            counts.swap_remove(old);
            if old != last {
                for local in indices {
                    if local.as_usize() == last {
                        *local = I::from_usize(old);
                    }
                }
            }
        }
    } else if counts[old] == 1 {
        palette[old] = state;
    } else {
        let next = palette.len();
        palette.push(state);
        counts.push(1);
        indices[cell] = I::from_usize(next);
        counts[old] -= 1;
    }
}

impl From<Vec<BlockId>> for PalettedBlocks {
    fn from(blocks: Vec<BlockId>) -> Self {
        Self::from_blocks(blocks)
    }
}

impl Index<usize> for PalettedBlocks {
    type Output = BlockId;

    fn index(&self, index: usize) -> &Self::Output {
        self.get_ref(index).expect("chunk cell index out of bounds")
    }
}

impl PartialEq for PalettedBlocks {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for PalettedBlocks {}

#[cfg(test)]
#[path = "palette/tests.rs"]
mod tests;
