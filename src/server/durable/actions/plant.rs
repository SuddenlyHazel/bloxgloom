//! Natural two-cell plants participate in the ordinary voxel transaction. They
//! consume one item, harvest once, and need no separately persisted entity.
use super::*;
use std::collections::BTreeMap;

pub(super) fn expand(
    state: &mut State,
    reads: &mut TerrainReads,
    at: [i32; 3],
    block: BlockId,
) -> io::Result<(
    Vec<crate::server::gameplay::Edit>,
    Vec<crate::server::gameplay::Removal>,
)> {
    let catalog = state.world.catalog_arc();
    let [x, y, z] = at;
    let mut edits = BTreeMap::from([(at, block)]);
    if let Some((lower, upper)) = crate::content::jg_rtx::tall_pair(&catalog, block) {
        if block != lower {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "place the lower plant state",
            ));
        }
        let above = [
            x,
            y.checked_add(1)
                .ok_or_else(|| io::Error::other("plant height overflow"))?,
            z,
        ];
        let previous = read(state, reads, above)?;
        if catalog.block_flags(previous) & crate::content::REPLACEABLE == 0 {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "plant needs two free cells",
            ));
        }
        edits.insert(above, upper);
    }
    // Clearing/replacing either half clears its peer, unless that cell is
    // already part of the new plant. Both reads are fenced by TerrainReads.
    let mut removals = BTreeMap::new();
    let initial = edits.keys().copied().collect::<Vec<_>>();
    for cell in initial {
        let old = read(state, reads, cell)?;
        if catalog.block_flags(old) & crate::content::PLANT != 0 {
            removals.insert(cell, old);
        }
        if let Some((lower, upper)) = crate::content::jg_rtx::tall_pair(&catalog, old) {
            let peer = [
                cell[0],
                cell[1]
                    .checked_add(if old == lower { 1 } else { -1 })
                    .ok_or_else(|| io::Error::other("plant height overflow"))?,
                cell[2],
            ];
            let previous = read(state, reads, peer)?;
            if previous == if old == lower { upper } else { lower } {
                edits.entry(peer).or_insert(AIR);
                removals.insert(peer, previous);
            }
        }
    }
    let cause = if block == AIR {
        RemovalCause::Break
    } else {
        RemovalCause::Replacement
    };
    Ok((
        edits
            .into_iter()
            .map(|([x, y, z], id)| (x, y, z, id))
            .collect(),
        removals
            .into_iter()
            .map(|(cell, id)| (id, cell, cause))
            .collect(),
    ))
}

fn read(state: &mut State, reads: &mut TerrainReads, [x, y, z]: [i32; 3]) -> io::Result<BlockId> {
    cached_block_with_reads(state, reads, x, y, z, "plant peer chunk is not resident")
}
