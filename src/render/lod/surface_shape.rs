//! Reconstruct gentle dry ground while retaining wet shores and cavity boundaries.
#[cfg(test)]
use crate::world::BEDROCK_Y;
use crate::{
    content::Catalog,
    lod::{Column, LodTile},
};

pub(super) struct Shape {
    width: i32,
    tops: [Option<i32>; 1024],
    corners: [Option<f32>; 1089],
}
#[derive(Clone, Copy)]
struct Ground {
    top: i32,
    bottom: i32,
}
impl Shape {
    pub(super) fn new(
        tile: &LodTile,
        neighbors: &[&LodTile],
        catalog: &Catalog,
        enabled: bool,
    ) -> Self {
        let width = tile.key.sample_width().unwrap_or(1);
        let mut shape = Self {
            width,
            tops: [None; 1024],
            corners: [None; 1089],
        };
        if !enabled || tile.key.level < 2 || crate::render::bsl_reference::enabled() {
            return shape;
        }
        let [ox, oz, _, _] = tile.key.bounds().unwrap();
        let top = |x: i32, z: i32| {
            let wx = ox.checked_add(x.checked_mul(width)?)?;
            let wz = oz.checked_add(z.checked_mul(width)?)?;
            std::iter::once(tile)
                .chain(neighbors.iter().copied())
                .find_map(|t| {
                    if t.key.level != tile.key.level {
                        return None;
                    }
                    let [x0, z0, x1, z1] = t.key.bounds()?;
                    if wx < x0 || wx >= x1 || wz < z0 || wz >= z1 {
                        return None;
                    }
                    // Roots must retain their sampled support height. Reject that
                    // whole cell so neighboring reconstruction cannot detach a stem.
                    if t.trees.iter().any(|tree| {
                        tree.anchor[0] >= wx
                            && tree.anchor[0] < wx + width
                            && tree.anchor[2] >= wz
                            && tree.anchor[2] < wz + width
                    }) {
                        return None;
                    }
                    ground_top(
                        &t.columns[((wx - x0) / width + 32 * ((wz - z0) / width)) as usize],
                        catalog,
                        width,
                    )
                })
        };
        for z in 0..32 {
            for x in 0..32 {
                shape.tops[(x + 32 * z) as usize] = top(x, z).map(|ground| ground.top);
            }
        }
        for z in 0..=32 {
            for x in 0..=32 {
                let heights = [top(x - 1, z - 1), top(x, z - 1), top(x - 1, z), top(x, z)];
                if let [Some(a), Some(b), Some(c), Some(d)] = heights {
                    let grounds = [a, b, c, d];
                    let values = grounds.map(|g| g.top);
                    let min = *values.iter().min().unwrap();
                    let max = *values.iter().max().unwrap();
                    // One vertical metre per horizontal metre is the limit: true
                    // cliffs stay voxel cliffs rather than turning into ramps.
                    if i64::from(max) - i64::from(min) <= i64::from(width)
                        && grounds
                            .iter()
                            .all(|g| i64::from(g.bottom) <= i64::from(min) - i64::from(width))
                    {
                        shape.corners[(x + 33 * z) as usize] =
                            Some(values.map(|h| h as f32).iter().sum::<f32>() * 0.25);
                    }
                }
            }
        }
        shape
    }
    pub(super) fn cap_changes(&self, x: usize, z: usize, y: i32) -> bool {
        self.tops[x + 32 * z] == Some(y)
            && [
                x + 33 * z,
                x + 1 + 33 * z,
                x + 33 * (z + 1),
                x + 1 + 33 * (z + 1),
            ]
            .into_iter()
            .any(|i| self.corners[i].is_some_and(|h| h != y as f32))
    }
    pub(super) fn deform(&self, vertices: &mut [super::vertex::Vertex]) -> bool {
        if self.corners.iter().all(Option::is_none) {
            return false;
        }
        let mut changed = false;
        for quad in vertices.chunks_exact_mut(4) {
            let mut quad_changed = false;
            let data = quad[0].ray_surface();
            if data.packed & ((1 << 11) | (1 << 12)) != 0 {
                continue;
            }
            let axis = (data.packed & 7) as usize / 2;
            let side = if data.packed & 1 == 0 { -1.0 } else { 1.0 };
            let mut center = [0.0; 3];
            for v in quad.iter() {
                for (i, c) in center.iter_mut().enumerate() {
                    *c += v.position[i] * 0.25;
                }
            }
            center[axis] -= side * 0.002;
            let x = (center[0] / self.width as f32).floor() as i32;
            let z = (center[2] / self.width as f32).floor() as i32;
            let Some(h) = self.top(x, z) else {
                continue;
            };
            let neighbor = if axis == 0 {
                self.top(x + side as i32, z)
            } else if axis == 2 {
                self.top(x, z + side as i32)
            } else {
                None
            };
            for v in quad.iter_mut() {
                let original = v.position[1];
                let Some(target) = self.height(v.position[0], v.position[2]) else {
                    continue;
                };
                if v.position[1] > (h - self.width) as f32
                    && neighbor.is_some_and(|n| v.position[1] == n as f32)
                {
                    // The bottom of an exposed stair wall is the adjacent
                    // ground's cap. Both caps now meet at this same corner.
                    v.position[1] = target;
                } else {
                    let weight = ((v.position[1] - (h - self.width) as f32) / self.width as f32)
                        .clamp(0.0, 1.0);
                    v.position[1] += (target - h as f32) * weight;
                }
                quad_changed |= original != v.position[1];
            }
            if quad_changed {
                for v in quad.iter_mut() {
                    v.mark_reconstructed();
                }
                changed = true;
            }
        }
        changed
    }
    fn top(&self, x: i32, z: i32) -> Option<i32> {
        if !(0..32).contains(&x) || !(0..32).contains(&z) {
            return None;
        }
        self.tops[(x + 32 * z) as usize]
    }
    fn height(&self, x: f32, z: f32) -> Option<f32> {
        let fx = (x / self.width as f32).clamp(0.0, 32.0);
        let fz = (z / self.width as f32).clamp(0.0, 32.0);
        let ix = (fx.floor() as usize).min(31);
        let iz = (fz.floor() as usize).min(31);
        let tx = fx - ix as f32;
        let tz = fz - iz as f32;
        let samples = [
            (ix + 33 * iz, (1.0 - tx) * (1.0 - tz)),
            (ix + 1 + 33 * iz, tx * (1.0 - tz)),
            (ix + 33 * (iz + 1), (1.0 - tx) * tz),
            (ix + 1 + 33 * (iz + 1), tx * tz),
        ];
        let mut height = 0.0;
        for (index, weight) in samples {
            if weight == 0.0 {
                continue;
            }
            let h = self.corners[index]?;
            height += h * weight;
        }
        Some(height)
    }
}
fn ground_top(column: &Column, catalog: &Catalog, depth: i32) -> Option<Ground> {
    let top = column.spans.last()?.top;
    let mut bottom = top;
    for s in column.spans.iter().rev() {
        if s.top != bottom {
            break;
        }
        let block = catalog.block(s.state)?;
        let Some(name) = block.key.strip_prefix("bloxgloom:") else {
            break;
        };
        if !matches!(
            name,
            "grass"
                | "dirt"
                | "stone"
                | "sand"
                | "snow"
                | "moss"
                | "gravel"
                | "coarse_dirt"
                | "rooted_dirt"
                | "podzol"
                | "mycelium"
                | "mud"
                | "packed_mud"
                | "clay"
                | "red_sand"
                | "sandstone"
                | "red_sandstone"
                | "granite"
                | "diorite"
                | "andesite"
                | "deepslate"
                | "tuff"
                | "calcite"
                | "basalt"
                | "smooth_basalt"
        ) {
            break;
        }
        bottom = s.bottom;
        // Only the upper shell moves. A deep cave must not disqualify its
        // hillside when every vertex at/below this depth is unchanged. Thin
        // roofs and cave mouths cannot satisfy this solid-shell requirement.
    }
    (i64::from(top) - i64::from(bottom) >= i64::from(depth)).then_some(Ground { top, bottom })
}
#[cfg(test)]
mod tests;
