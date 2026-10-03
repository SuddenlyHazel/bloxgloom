//! Bounded dominant-emitter color and incoming transport direction.
//! Catalog emission is scalar today: normalized material reflectance is an
//! interim emitter-color convention, not a spectral/emission-color API.
//! An emissive material with zero reflectance falls back to neutral white.
use super::{BlockId, CHUNK_SIZE, Catalog, LightField, MAX_LIGHT, PLANE, SIDE, VOLUME, index};

#[derive(Clone, Copy, Default)]
pub(super) struct LocalLight {
    pub color: [u8; 3],
    pub direction: [i8; 3],
}

fn source_color(catalog: &Catalog, block: BlockId) -> [u8; 3] {
    let color = catalog.reflectance(block);
    let peak = *color.iter().max().unwrap();
    if peak == 0 {
        [255; 3]
    } else {
        color.map(|c| (u16::from(c) * 255 / u16::from(peak)) as u8)
    }
}

fn neighbors(at: usize) -> impl Iterator<Item = (usize, usize, i16)> {
    let x = at % SIDE;
    let z = at / SIDE % SIDE;
    let y = at / PLANE;
    [
        (x > 0, at.wrapping_sub(1), 0, -1),
        (x + 1 < SIDE, at + 1, 0, 1),
        (y > 0, at.wrapping_sub(PLANE), 1, -1),
        (y + 1 < SIDE, at + PLANE, 1, 1),
        (z > 0, at.wrapping_sub(SIDE), 2, -1),
        (z + 1 < SIDE, at + SIDE, 2, 1),
    ]
    .into_iter()
    .filter_map(|(valid, at, axis, sign)| valid.then_some((at, axis, sign)))
}

pub(super) fn build(blocks: &[BlockId], glow: &[u8], catalog: &Catalog) -> Option<Vec<LocalLight>> {
    if !glow.iter().any(|&level| level > 0) {
        return None;
    }
    let mut field = vec![LocalLight::default(); VOLUME];
    // A descending scalar-light DAG visits each lit voxel exactly once. Ties
    // choose lexicographically greatest RGB, independent of source queue order,
    // chunk-relative indices, or hash iteration. No equal-strength requeues.
    let mut buckets: [Vec<u32>; 16] = std::array::from_fn(|_| Vec::new());
    for (at, &level) in glow.iter().enumerate() {
        if level > 0 {
            buckets[usize::from(level.min(MAX_LIGHT))].push(at as u32);
        }
    }
    for level in (1..=MAX_LIGHT).rev() {
        for &at in &buckets[usize::from(level)] {
            let at = at as usize;
            let emitter = catalog.emission(blocks[at]) == level;
            let own = if emitter {
                source_color(catalog, blocks[at])
            } else {
                [0; 3]
            };
            // Scalar transport cannot enter opaque emitters: their own light
            // remains their own color even beside a brighter different source.
            if super::is_opaque(catalog, blocks[at]) {
                field[at].color = own;
                continue;
            }
            let mut color = own;
            for (neighbor, _, _) in neighbors(at) {
                if glow[neighbor] == level + 1 {
                    color = color.max(field[neighbor].color);
                }
            }
            let mut direction = [0i16; 3];
            let mut count = i16::from(emitter && color == own);
            for (neighbor, axis, sign) in neighbors(at) {
                if glow[neighbor] == level + 1 && field[neighbor].color == color {
                    direction[axis] += sign * 127;
                    count += 1;
                }
            }
            field[at] = LocalLight {
                color,
                // Mean of unit incoming edges, not normalized: opposing paths
                // lose confidence. A bend points through its opening, never
                // directly toward a hidden emitter across an opaque wall.
                direction: direction.map(|value| (value / count.max(1)) as i8),
            };
        }
    }
    Some(field)
}

impl LightField {
    fn average_local(&self, samples: impl Iterator<Item = (usize, f32)>) -> ([f32; 3], [f32; 3]) {
        let Some(field) = &self.local else {
            return ([0.0; 3], [0.0; 3]);
        };
        let mut color = [0.0; 3];
        let mut direction = [0.0; 3];
        let mut energy = 0.0;
        for (at, weight) in samples {
            let light = weight * f32::from(self.glow[at]) / 15.0;
            energy += light;
            for channel in 0..3 {
                color[channel] += light * f32::from(field[at].color[channel]) / 255.0;
                direction[channel] += light * f32::from(field[at].direction[channel]) / 127.0;
            }
        }
        if energy > 0.0 {
            direction = direction.map(|v| v / energy);
        }
        (color, direction)
    }

    /// Linear local RGB irradiance and energy-weighted incoming direction.
    /// Direction length retains confidence; do not normalize after interpolation.
    pub(crate) fn corner_local(
        &self,
        axes: [usize; 3],
        side: i32,
        slice: usize,
        corner: [usize; 2],
    ) -> ([f32; 3], [f32; 3]) {
        let [axis, u, v] = axes;
        self.average_local([-1isize, 0].into_iter().flat_map(|du| {
            [-1isize, 0].into_iter().map(move |dv| {
                let mut point = [CHUNK_SIZE; 3];
                point[axis] = (CHUNK_SIZE + slice)
                    .checked_add_signed(if side > 0 { 1 } else { -1 })
                    .unwrap();
                point[u] = (CHUNK_SIZE + corner[0]).checked_add_signed(du).unwrap();
                point[v] = (CHUNK_SIZE + corner[1]).checked_add_signed(dv).unwrap();
                (index(point[0], point[1], point[2]), 0.25)
            })
        }))
    }

    pub(crate) fn spatial_local(&self, local: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let position = local.map(|p| (p + CHUNK_SIZE as f32 - 0.5).clamp(0.0, (SIDE - 2) as f32));
        let base = position.map(|p| p.floor() as usize);
        let fraction = std::array::from_fn::<_, 3, _>(|axis| position[axis] - base[axis] as f32);
        self.average_local((0..8).map(|corner| {
            let offsets = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
            let weight = (0..3)
                .map(|axis| {
                    if offsets[axis] == 0 {
                        1.0 - fraction[axis]
                    } else {
                        fraction[axis]
                    }
                })
                .product();
            (
                index(
                    base[0] + offsets[0],
                    base[1] + offsets[1],
                    base[2] + offsets[2],
                ),
                weight,
            )
        }))
    }
}
