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
) -> Mesh {
    let mut m = Mesh {
        key: tile.key,
        revision: tile.revision,
        vertices: vec![],
        indices: vec![],
    };
    let Some([ox, oz, _, _]) = tile.key.bounds() else {
        return m;
    };
    let Some(w) = tile.key.sample_width().filter(|w| *w <= 32) else {
        return m;
    };
    if tile.columns.len() != 1024 {
        return m;
    }
    for z in 0..32 {
        for x in 0..32 {
            let c = &tile.columns[x + 32 * z];
            let px = ox + x as i32 * w;
            let pz = oz + z as i32 * w;
            for span in &c.spans {
                for (axis, side, y) in [(1, -1, span.bottom), (1, 1, span.top)] {
                    // Spans touching vertically share no visible cap.
                    if c.spans
                        .iter()
                        .any(|s| if side > 0 { s.bottom == y } else { s.top == y })
                    {
                        continue;
                    }
                    quad(
                        &mut m,
                        [px - ox, y, pz - oz],
                        [w, 0, w],
                        axis,
                        side,
                        colors.face(catalog, span.state, axis, side),
                        span.sky,
                        span.glow,
                    );
                }
                for axis in [0, 2] {
                    for side in [-1, 1] {
                        let mut start = 0;
                        let mut previous: Vec<(i32, i32)> = vec![];
                        for strip in 0..=w {
                            let mut intervals = vec![];
                            if strip < w {
                                let nx = if axis == 0 {
                                    px + if side < 0 { -1 } else { w }
                                } else {
                                    px + strip
                                };
                                let nz = if axis == 2 {
                                    pz + if side < 0 { -1 } else { w }
                                } else {
                                    pz + strip
                                };
                                intervals.push((span.bottom, span.top));
                                if let Some(nc) = find_column(tile, neighbors, nx, nz) {
                                    for s in &nc.spans {
                                        intervals = subtract(intervals, s.bottom, s.top);
                                    }
                                }
                            }
                            if intervals != previous {
                                for &(bottom, top) in &previous {
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
                                        [0, top - bottom, strip - start]
                                    } else {
                                        [strip - start, top - bottom, 0]
                                    };
                                    quad(
                                        &mut m,
                                        pos,
                                        size,
                                        axis,
                                        side,
                                        colors.face(catalog, span.state, axis, side),
                                        span.sky,
                                        span.glow,
                                    );
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
    m
}
fn find_column<'a>(
    tile: &'a LodTile,
    neighbors: &[&'a LodTile],
    x: i32,
    z: i32,
) -> Option<&'a Column> {
    std::iter::once(tile)
        .chain(neighbors.iter().copied())
        .filter_map(|t| {
            let [ox, oz, mx, mz] = t.key.bounds()?;
            let w = t.key.sample_width()?;
            if x < ox || x >= mx || z < oz || z >= mz {
                return None;
            }
            Some((
                t.key.level,
                &t.columns[((x - ox) / w + 32 * ((z - oz) / w)) as usize],
            ))
        })
        .min_by_key(|(l, _)| *l)
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
    size: [i32; 3],
    axis: usize,
    side: i32,
    color: [f32; 3],
    sky: u8,
    glow: u8,
) {
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let base = (mesh.vertices.len() / 11) as u32;
    for (a, b) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
        let mut pos = p;
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
}
