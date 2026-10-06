//! Bounded, absolute-coordinate river channels and enclosed lake/pond basins.
//! Water is generated source terrain; no fluid simulation or gameplay ownership.
use super::{Column, base_column, lattice_hash, noise2, smooth};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    River,
    Lake,
    Pond,
    Ocean,
}
#[derive(Clone, Copy)]
struct Site {
    x: f64,
    z: f64,
    rx: f64,
    rz: f64,
    level: i64,
    depth: f64,
}
pub(super) struct Sampler {
    seed: u64,
    sites: HashMap<(i64, i64, u8), Option<Site>>,
}
impl Sampler {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            seed,
            sites: HashMap::new(),
        }
    }
    pub(super) fn column(&mut self, x: i64, z: i64) -> Column {
        let mut column = base_column(x, z, self.seed);
        // Preserve a dry, level-safe starter area. A smooth outer apron avoids
        // a square seam where the river passes the protected spawn column.
        let spawn_distance = ((x as f64).powi(2) + (z as f64).powi(2)).sqrt();
        if spawn_distance < 24.0 {
            return column;
        }
        if column.height < super::landforms::SEA_LEVEL {
            column.water_level = Some(super::landforms::SEA_LEVEL);
            column.water_kind = Some(Kind::Ocean);
            column.shore = column.height >= super::landforms::SEA_LEVEL - 3;
            return column;
        }
        if let Some((distance, width, depth)) = self.river(x, z) {
            carve(
                &mut column,
                distance / width,
                1.0 + 14.0 / width,
                16,
                depth,
                Kind::River,
                spawn_distance,
            );
            return column;
        }
        for (kind, size) in [(Kind::Lake, 256), (Kind::Pond, 80)] {
            let k = (x.div_euclid(size), z.div_euclid(size), kind as u8);
            let site = *self
                .sites
                .entry(k)
                .or_insert_with(|| site(k.0, k.1, size, kind, self.seed));
            let Some(site) = site else {
                continue;
            };
            let dx = (x as f64 - site.x) / site.rx;
            let dz = (z as f64 - site.z) / site.rz;
            let distance = (dx * dx + dz * dz).sqrt();
            // Small outline variation; cached sites retain a constant water plane.
            let outline = 1.0 + noise2(x, z, 13, self.seed ^ 0x8572_330f) * 0.08;
            if distance < 1.28 * outline {
                carve(
                    &mut column,
                    distance / outline,
                    1.28,
                    site.level,
                    site.depth,
                    kind,
                    spawn_distance,
                );
                return column;
            }
        }
        column
    }
    fn river(&self, x: i64, z: i64) -> Option<(f64, f64, f64)> {
        let field = super::landforms::drainage(x, z, self.seed);
        // Reuse the valley field that suppressed mountain uplift. Its zero
        // contours turn, branch and cross regional boundaries without lanes.
        let width = 5.5 + (noise2(x, z, 192, self.seed ^ 0x71aa_0993) + 1.0) * 2.0;
        let distance = field * 240.0;
        (distance < width + 14.0).then_some((distance, width, 3.0))
    }
}
fn site(cx: i64, cz: i64, size: i64, kind: Kind, seed: u64) -> Option<Site> {
    let hash = lattice_hash(seed ^ 0x10ad_cafe ^ kind as u64, cx, 0, cz);
    if hash % 100 >= if kind == Kind::Lake { 52 } else { 38 } {
        return None;
    }
    let x = cx * size + size / 2 + ((hash >> 8) % (size as u64 / 3)) as i64 - size / 6;
    let z = cz * size + size / 2 + ((hash >> 20) % (size as u64 / 3)) as i64 - size / 6;
    let rx = if kind == Kind::Lake {
        28.0 + ((hash >> 32) % 24) as f64
    } else {
        6.0 + ((hash >> 32) % 7) as f64
    };
    let rz = rx * (0.65 + ((hash >> 40) % 31) as f64 / 100.0);
    let rim = [(-rx, 0.0), (rx, 0.0), (0.0, -rz), (0.0, rz)]
        .map(|(dx, dz)| base_column(x + dx as i64, z + dz as i64, seed).height);
    let level = (rim.into_iter().min().unwrap() - 1).max(10);
    Some(Site {
        x: x as f64,
        z: z as f64,
        rx,
        rz,
        level,
        depth: if kind == Kind::Lake {
            4.0 + ((hash >> 48) % 4) as f64
        } else {
            1.0 + ((hash >> 48) % 3) as f64
        },
    })
}
fn carve(
    column: &mut Column,
    distance: f64,
    outer: f64,
    level: i64,
    depth: f64,
    kind: Kind,
    spawn_distance: f64,
) {
    let mut target;
    if distance < 1.0 {
        let depth = ((1.0 - distance * distance).sqrt() * depth)
            .round()
            .max(1.0) as i64;
        target = level - depth;
    } else {
        let t = smooth(((distance - 1.0) / (outer - 1.0)).clamp(0.0, 1.0));
        target = (level as f64 * (1.0 - t) + column.height as f64 * t).round() as i64;
        // A narrow dry rim contains the source water even where the natural
        // slope dips between site samples. Blend back into terrain outside it.
        if distance < 1.08 {
            target = target.max(level);
        }
    }
    if spawn_distance < 40.0 {
        let t = smooth((spawn_distance - 24.0) / 16.0);
        target = (column.height as f64 * (1.0 - t) + target as f64 * t).round() as i64;
    }
    column.height = target;
    if distance < 1.0 && target < level {
        column.water_level = Some(level);
        column.water_kind = Some(kind);
    }
    column.shore = distance < 1.15;
}
#[cfg(test)]
mod tests;
