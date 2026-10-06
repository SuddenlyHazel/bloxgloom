//! Canonical lossless material palette shared by wire sizing and serialization.
use super::{LodTile, TreeFeature};
use crate::content::BlockStateId;

// Key9 +revision8 +error4 +columns2 +palette count2/width1 +trees2.
pub(crate) const HEADER_BYTES: usize = 28;

pub(crate) fn states(tile: &LodTile) -> Vec<BlockStateId> {
    let mut states = tile
        .columns
        .iter()
        .flat_map(|column| column.spans.iter().map(|span| span.state))
        .collect::<Vec<_>>();
    states.sort_unstable();
    states.dedup();
    states
}

pub(crate) fn index_width(count: usize) -> usize {
    if count <= 256 { 1 } else { 2 }
}

pub(crate) fn encoded_bytes(tile: &LodTile) -> usize {
    let count = states(tile).len();
    let span_bytes = 9 + index_width(count);
    HEADER_BYTES
        + count * 4
        + tile.trees.len() * TreeFeature::WIRE_BYTES
        + tile
            .columns
            .iter()
            .map(|column| 4 + column.coverage.len() * 8 + column.spans.len() * span_bytes)
            .sum::<usize>()
}
