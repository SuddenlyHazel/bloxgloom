//! Union occupancy retains narrow bridges; majority material resolves overlaps.
//! Coverage is an intersection, so partial children cannot manufacture known air.
use super::{Column, Interval, LodTile, Span, TILE_COLUMNS, TILE_SIZE, TileKey};
use crate::content::Catalog;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn merge(columns: &[&Column]) -> Column {
    let Some(first) = columns.first() else {
        return Column::default();
    };
    let mut coverage = first.coverage.clone();
    for c in &columns[1..] {
        coverage = coverage
            .iter()
            .flat_map(|a| {
                c.coverage.iter().filter_map(move |b| {
                    let bottom = a.bottom.max(b.bottom);
                    let top = a.top.min(b.top);
                    (bottom < top).then_some(Interval { bottom, top })
                })
            })
            .collect();
    }
    let mut out = Column {
        coverage,
        spans: Vec::new(),
    };
    for interval in &out.coverage {
        let mut edges = BTreeSet::from([interval.bottom, interval.top]);
        for c in columns {
            for s in &c.spans {
                if s.bottom < interval.top && s.top > interval.bottom {
                    edges.insert(s.bottom.max(interval.bottom));
                    edges.insert(s.top.min(interval.top));
                }
            }
        }
        let edges: Vec<_> = edges.into_iter().collect();
        for pair in edges.windows(2) {
            let bottom = pair[0];
            let top = pair[1];
            let mut votes = BTreeMap::new();
            for c in columns {
                if let Some(s) = c.spans.iter().find(|s| s.bottom <= bottom && s.top >= top) {
                    let v = votes.entry(s.state).or_insert((0_usize, 0_u8, 0_u8));
                    v.0 += 1;
                    v.1 = v.1.max(s.sky);
                    v.2 = v.2.max(s.glow);
                }
            }
            // Stable ties prefer the smaller save identity, independent of traversal order.
            if let Some((state, (_, sky, glow))) = votes
                .into_iter()
                .max_by(|a, b| a.1.0.cmp(&b.1.0).then_with(|| b.0.cmp(&a.0)))
            {
                let span = Span {
                    bottom,
                    top,
                    state,
                    sky,
                    glow,
                };
                if let Some(last) = out.spans.last_mut()
                    && last.top == bottom
                    && last.state == state
                    && last.sky == sky
                    && last.glow == glow
                {
                    last.top = top;
                } else {
                    out.spans.push(span);
                }
            }
        }
    }
    out
}

pub fn reduce_parent(
    key: TileKey,
    revision: u64,
    children: [&LodTile; 4],
    catalog: &Catalog,
) -> Result<LodTile, String> {
    let expected = key.children().ok_or("invalid parent key")?;
    for (i, c) in children.iter().enumerate() {
        c.validate(catalog)?;
        if c.key != expected[i] {
            return Err("LOD parent children do not match".into());
        }
    }
    let mut columns = Vec::with_capacity(TILE_COLUMNS);
    for z in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let child = children[(x / 16) + 2 * (z / 16)];
            let xx = (x % 16) * 2;
            let zz = (z % 16) * 2;
            columns.push(merge(&[
                &child.columns[xx + zz * TILE_SIZE],
                &child.columns[xx + 1 + zz * TILE_SIZE],
                &child.columns[xx + (zz + 1) * TILE_SIZE],
                &child.columns[xx + 1 + (zz + 1) * TILE_SIZE],
            ]));
        }
    }
    let geometric_error = children
        .iter()
        .map(|c| c.geometric_error)
        .max()
        .unwrap_or(0)
        .saturating_add(key.sample_width().unwrap_or(1) as u32);
    let tile = LodTile {
        key,
        revision,
        columns,
        geometric_error,
    };
    tile.into_render_summary(catalog)
}
