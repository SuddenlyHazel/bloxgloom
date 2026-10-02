use super::FaceColors;
use crate::{
    content::Catalog,
    lod::{Column, LodTile, TileKey},
};
#[derive(Clone, Debug)]
pub(crate) struct Mesh {
    pub key: TileKey,
    pub revision: u64,
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub(super) coverage: super::coverage::Coverage,
}
impl Mesh {
    pub(crate) fn byte_len(&self) -> usize {
        (self.vertices.len() + self.indices.len()) * 4
    }
}
/// Exact retained span surfaces. Side boundaries are split into unit strips:
/// neighbor samples can have any LOD width, without skirts closing air gaps.
pub(crate) fn mesh(
    tile: &LodTile,
    neighbors: &[&LodTile],
    catalog: &Catalog,
    colors: &FaceColors,
) -> Result<Mesh, String> {
    tile.validate(catalog)?;
    let mut m = Mesh {
        key: tile.key,
        revision: tile.revision,
        vertices: vec![],
        indices: vec![],
        coverage: super::coverage::Coverage::from_tile(tile),
    };
    let Some([ox, oz, _, _]) = tile.key.bounds() else {
        return Err("unsupported LOD mesh bounds or level".into());
    };
    let Some(w) = tile.key.sample_width().filter(|w| *w <= 32) else {
        return Err("unsupported LOD mesh bounds or level".into());
    };
    if tile.columns.len() != 1024 {
        return Err("unsupported LOD mesh bounds or level".into());
    }
    let mut caps = std::collections::BTreeMap::new();
    for z in 0..32 {
        for x in 0..32 {
            let c = &tile.columns[x + 32 * z];
            let px = ox + x as i32 * w;
            let pz = oz + z as i32 * w;
            for span in &c.spans {
                for (side, y) in [(-1, span.bottom), (1, span.top)] {
                    // Spans touching vertically share no visible cap.
                    if c.spans
                        .iter()
                        .any(|s| if side > 0 { s.bottom == y } else { s.top == y })
                    {
                        continue;
                    }
                    caps.entry((
                        y,
                        span.state,
                        side,
                        if side < 0
                            && catalog
                                .state(span.state)
                                .is_some_and(|s| s.flags & crate::content::OPAQUE != 0)
                        {
                            0
                        } else {
                            span.sky
                        },
                        span.glow,
                    ))
                    .or_insert([false; 1024])[x + 32 * z] = true;
                }
                for axis in [0, 2] {
                    for side in [-1, 1] {
                        let mut start = 0;
                        let mut previous: Vec<(i32, i32, u8)> = vec![];
                        let edge = if axis == 0 {
                            (x == 0 && side < 0) || (x == 31 && side > 0)
                        } else {
                            (z == 0 && side < 0) || (z == 31 && side > 0)
                        };
                        let step =
                            if !edge || neighbors.iter().all(|n| n.key.level >= tile.key.level) {
                                w
                            } else {
                                1
                            };
                        for strip in (0..=w).step_by(step as usize) {
                            let mut intervals = vec![];
                            let mut neighbor = None;
                            if strip < w {
                                let nx = if axis == 0 {
                                    px.checked_add(if side < 0 { -1 } else { w })
                                } else {
                                    px.checked_add(strip)
                                };
                                let nz = if axis == 2 {
                                    pz.checked_add(if side < 0 { -1 } else { w })
                                } else {
                                    pz.checked_add(strip)
                                };
                                intervals.push((span.bottom, span.top));
                                neighbor = nx
                                    .zip(nz)
                                    .and_then(|(nx, nz)| find_column(tile, neighbors, nx, nz));
                                if let Some(nc) = neighbor {
                                    if !edge {
                                        for s in &nc.spans {
                                            intervals = subtract(intervals, s.bottom, s.top);
                                        }
                                    } else {
                                        // Keep the exterior walls until the neighbor is
                                        // actually drawable. Split their geometry at
                                        // neighbor edges without removing coverage.
                                        for s in &nc.spans {
                                            for y in [s.bottom, s.top] {
                                                intervals = intervals
                                                    .into_iter()
                                                    .flat_map(|(b, t)| {
                                                        if b < y && y < t {
                                                            vec![(b, y), (y, t)]
                                                        } else {
                                                            vec![(b, t)]
                                                        }
                                                    })
                                                    .collect();
                                            }
                                        }
                                    }
                                }
                            }
                            let intervals = side_light(intervals, neighbor, span.sky, catalog);
                            if intervals != previous {
                                for &(bottom, top, sky) in &previous {
                                    let pos = if axis == 0 {
                                        [
                                            px - ox + if side > 0 { w } else { 0 },
                                            bottom,
                                            pz - oz + start,
                                        ]
                                    } else {
                                        [
                                            px - ox + start,
                                            bottom,
                                            pz - oz + if side > 0 { w } else { 0 },
                                        ]
                                    };
                                    let size = if axis == 0 {
                                        [
                                            0,
                                            i64::from(top) - i64::from(bottom),
                                            i64::from(strip - start),
                                        ]
                                    } else {
                                        [
                                            i64::from(strip - start),
                                            i64::from(top) - i64::from(bottom),
                                            0,
                                        ]
                                    };
                                    quad(
                                        &mut m,
                                        pos,
                                        size,
                                        axis,
                                        side,
                                        colors.face(catalog, span.state, axis, side),
                                        sky,
                                        span.glow,
                                    )?;
                                }
                                start = strip;
                                previous = intervals;
                            }
                        }
                    }
                }
            }
        }
    }
    // Greedy coplanar caps preserve each material/light boundary and every
    // retained opening while avoiding thousands of redundant flat roof quads.
    for ((y, state, side, sky, glow), mut cells) in caps {
        for z in 0..32 {
            for x in 0..32 {
                if !cells[x + 32 * z] {
                    continue;
                }
                let mut dx = 1;
                while x + dx < 32 && cells[x + dx + 32 * z] {
                    dx += 1;
                }
                let mut dz = 1;
                while z + dz < 32 && (x..x + dx).all(|xx| cells[xx + 32 * (z + dz)]) {
                    dz += 1;
                }
                for zz in z..z + dz {
                    for xx in x..x + dx {
                        cells[xx + 32 * zz] = false;
                    }
                }
                quad(
                    &mut m,
                    [x as i32 * w, y, z as i32 * w],
                    [dx as i64 * i64::from(w), 0, dz as i64 * i64::from(w)],
                    1,
                    side,
                    colors.face(catalog, state, 1, side),
                    sky,
                    glow,
                )?;
            }
        }
    }
    Ok(m)
}
// The outside column's top occluder divides outdoor cliff walls from faces
// looking into a retained roof/cavity. In particular, a sky-lit roof span
// never makes its interior wall faces fully sky-lit.
fn side_light(
    intervals: Vec<(i32, i32)>,
    neighbor: Option<&Column>,
    sky: u8,
    catalog: &Catalog,
) -> Vec<(i32, i32, u8)> {
    let Some(top) = neighbor.and_then(|n| {
        n.spans.iter().rev().find(|s| {
            catalog
                .state(s.state)
                .is_some_and(|state| state.flags & crate::content::OPAQUE != 0)
        })
    }) else {
        return intervals.into_iter().map(|(b, t)| (b, t, sky)).collect();
    };
    intervals
        .into_iter()
        .flat_map(|(b, t)| {
            let mut out = vec![];
            if b < top.top {
                out.push((b, t.min(top.top), 0));
            }
            if t > top.top {
                out.push((b.max(top.top), t, sky.max(top.sky)));
            }
            out
        })
        .collect()
}
fn find_column<'a>(
    tile: &'a LodTile,
    neighbors: &[&'a LodTile],
    x: i32,
    z: i32,
) -> Option<&'a Column> {
    let lookup = |t: &'a LodTile| {
        let [ox, oz, mx, mz] = t.key.bounds()?;
        let w = t.key.sample_width()?;
        if x < ox || x >= mx || z < oz || z >= mz {
            return None;
        }
        t.columns.get(((x - ox) / w + 32 * ((z - oz) / w)) as usize)
    };
    if let Some(c) = lookup(tile) {
        return Some(c);
    }
    neighbors
        .iter()
        .filter_map(|t| lookup(t).map(|c| (t.key.level, c)))
        .min_by_key(|(level, _)| *level)
        .map(|(_, c)| c)
}

fn subtract(source: Vec<(i32, i32)>, bottom: i32, top: i32) -> Vec<(i32, i32)> {
    source
        .into_iter()
        .flat_map(|(b, t)| {
            let mut r = Vec::with_capacity(2);
            if top <= b || bottom >= t {
                r.push((b, t));
            } else {
                if b < bottom {
                    r.push((b, bottom));
                }
                if top < t {
                    r.push((top, t));
                }
            }
            r
        })
        .collect()
}
#[allow(clippy::too_many_arguments)]
fn quad(
    mesh: &mut Mesh,
    p: [i32; 3],
    size: [i64; 3],
    axis: usize,
    side: i32,
    color: [f32; 3],
    sky: u8,
    glow: u8,
) -> Result<(), String> {
    if mesh.byte_len() + 200 > 8 * 1024 * 1024 {
        return Err("LOD mesh exceeds 8 MiB geometry budget".into());
    }
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let base = (mesh.vertices.len() / 11) as u32;
    for (a, b) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
        let mut pos = p.map(i64::from);
        pos[u] += a * size[u];
        pos[v] += b * size[v];
        let mut n = [0.0; 3];
        n[axis] = side as f32;
        mesh.vertices.extend(pos.map(|x| x as f32));
        mesh.vertices.extend(n);
        mesh.vertices.extend(color);
        mesh.vertices
            .extend([f32::from(sky) / 15.0, f32::from(glow) / 15.0]);
    }
    mesh.indices.extend(if side > 0 {
        [base, base + 1, base + 2, base, base + 2, base + 3]
    } else {
        [base, base + 2, base + 1, base, base + 3, base + 2]
    });
    Ok(())
}
