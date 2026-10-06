//! Fine transition sampling shares the same gap-preserving union as parent reduction.
use super::LodSampler;
use crate::{
    content::{CUTOUT, Catalog, FLUID, OPAQUE, PLANT},
    lod::{Column, Interval, Span},
    world::{AIR, BlockId},
};
use std::collections::BTreeMap;
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn cell(
    sampler: &mut LodSampler,
    x: i32,
    z: i32,
    width: i32,
    bottom: i32,
    top: i32,
    catalog: &Catalog,
) -> Result<BTreeMap<i32, BlockId>, String> {
    cell_for_forest(sampler, x, z, width, bottom, top, catalog, false)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn cell_for_forest(
    sampler: &mut LodSampler,
    x: i32,
    z: i32,
    width: i32,
    bottom: i32,
    top: i32,
    catalog: &Catalog,
    proxy_forest: bool,
) -> Result<BTreeMap<i32, BlockId>, String> {
    let sample = |sampler: &mut LodSampler, dx, dz| {
        (bottom..top)
            .zip(if proxy_forest {
                sampler.ground_column(i64::from(x) + dx, i64::from(z) + dz, bottom, top)
            } else {
                sampler.column(i64::from(x) + dx, i64::from(z) + dz, bottom, top)
            })
            .collect::<BTreeMap<_, _>>()
    };
    if width != 2 {
        return Ok(sample(sampler, i64::from(width / 2), i64::from(width / 2)));
    }
    let mut columns = Vec::with_capacity(4);
    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        columns.push(column(
            sample(sampler, dx, dz),
            vec![Interval { bottom, top }],
            catalog,
        )?);
    }
    let merged = crate::lod::merge_columns(&columns.iter().collect::<Vec<_>>());
    Ok(merged
        .spans
        .into_iter()
        .flat_map(|s| (s.bottom..s.top).map(move |y| (y, s.state)))
        .collect())
}
pub(super) fn column(
    states: BTreeMap<i32, BlockId>,
    coverage: Vec<Interval>,
    catalog: &Catalog,
) -> Result<Column, String> {
    let mut spans: Vec<Span> = Vec::new();
    for (y, id) in states {
        let state = catalog.state(id).ok_or("unknown builtin state")?;
        if id == AIR || state.flags & (OPAQUE | CUTOUT | FLUID) == 0 || state.flags & PLANT != 0 {
            continue;
        }
        if let Some(last) = spans.last_mut()
            && last.top == y
            && last.state == id
            && last.glow == state.emission
        {
            last.top = y + 1;
        } else {
            spans.push(Span {
                bottom: y,
                top: y + 1,
                state: id,
                sky: 0,
                glow: state.emission,
            });
        }
    }
    let mut column = Column { coverage, spans };
    crate::lod::skylight::assign(&mut column, catalog);
    Ok(column)
}
