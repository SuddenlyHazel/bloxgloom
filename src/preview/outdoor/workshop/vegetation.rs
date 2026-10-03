//! Authored fixture vegetation: no procedural world or save-format changes.
use super::*;

pub(super) const TREES: [(i32, i32, i32, i32); 9] = [
    (-13, -11, 10, 4),
    (-18, -21, 12, 5),
    (-10, -27, 11, 4),
    (2, -27, 13, 5),
    (15, -25, 11, 4),
    (19, -14, 12, 4),
    (17, 0, 10, 4),
    (-15, 7, 11, 4),
    (-24, -3, 12, 5),
];

pub(super) fn plant(s: &mut Builder<'_>) {
    for z in -29i32..=20 {
        for x in -25i32..=25 {
            let in_building = (-8..=11).contains(&x) && (-25..=-5).contains(&z);
            let path = (x - if z > -4 { (z + 4) / 7 } else { 0 }).abs() <= 3;
            if in_building || path || s.block([x, 33, z]) != Some(world::AIR) {
                continue;
            }
            // Overlapping jittered patches leave quiet gaps, rather than distributing
            // the same plant uniformly across every verge. Coordinates, not a mutable
            // RNG, keep the six camera/time captures exactly matched.
            let mut density = 0;
            for cz in z.div_euclid(7) - 1..=z.div_euclid(7) + 1 {
                for cx in x.div_euclid(7) - 1..=x.div_euclid(7) + 1 {
                    let seed = hash(cx, cz);
                    let px = cx * 7 + (seed % 6) as i32;
                    let pz = cz * 7 + ((seed >> 8) % 6) as i32;
                    let radius = 2 + ((seed >> 16) % 3) as i32;
                    let distance = (x - px).pow(2) + (z - pz).pow(2);
                    if distance < radius * radius {
                        density = density.max(65 - distance * 45 / (radius * radius));
                    }
                }
            }
            let seed = hash(x, z);
            if (seed % 100) as i32 >= density {
                continue;
            }
            let species = (hash(x.div_euclid(5), z.div_euclid(5)) >> 12) % 3;
            let block = match ((seed >> 16) % 20, species) {
                (0, _) => world::YELLOW_FLOWER,
                (1..=14, 0) | (1..=5, _) => world::FERN,
                _ => world::TALL_GRASS,
            };
            s.put(
                [x, 32, z],
                if block == world::FERN {
                    world::MOSS
                } else {
                    world::GRASS
                },
            );
            s.put([x, 33, z], block);
        }
    }
    for (index, &(x, z, h, r)) in TREES.iter().enumerate() {
        tree(s, x, z, h, r, index);
    }
}

fn hash(x: i32, z: i32) -> u32 {
    let mut n =
        (x as u32).wrapping_mul(0x9e37_79b9) ^ (z as u32).wrapping_mul(0x85eb_ca6b) ^ 0xa341_316c;
    n = (n ^ (n >> 16)).wrapping_mul(0x7feb_352d);
    n ^ (n >> 15)
}

fn rotate(x: i32, z: i32, index: usize) -> (i32, i32) {
    match index % 4 {
        0 => (x, z),
        1 => (-z, x),
        2 => (-x, -z),
        _ => (z, -x),
    }
}

fn crown(dx: i32, dy: i32, dz: i32, radius: i32, index: usize) -> bool {
    let (dx, dz) = rotate(dx, dz, index);
    let r = radius as f32;
    let vertical = 0.85 + (index % 3) as f32 * 0.17;
    let p = Vec3::new(dx as f32, dy as f32 / vertical, dz as f32);
    [
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(r * 0.75, 3.2, r * 0.7)),
        (
            Vec3::new(-r * 0.55, -1.0, r * 0.2),
            Vec3::new(r * 0.7, 2.1, r * 0.65),
        ),
        (
            Vec3::new(r * 0.4, 1.3, -r * 0.3),
            Vec3::new(r * 0.65, 2.8, r * 0.6),
        ),
    ]
    .iter()
    .any(|(center, axes)| ((p - *center) / *axes).length_squared() <= 1.0)
}

fn tree(s: &mut Builder<'_>, x: i32, z: i32, h: i32, r: i32, index: usize) {
    s.fill([x, 33, z], [x, 33 + h, z], world::WOOD);
    // Branches follow the low side lobe, rotating between trees. Leaves never
    // overwrite them or existing workshop geometry.
    for distance in 1..=r / 2 {
        let (dx, dz) = rotate(-distance, 0, (4 - index % 4) % 4);
        let p = [x + dx, 31 + h + distance / 2, z + dz];
        if s.block(p) == Some(world::AIR) {
            s.put(
                p,
                if dx != 0 {
                    world::WOOD_X
                } else {
                    world::WOOD_Z
                },
            );
        }
    }
    for dy in -5..=5 {
        for dz in -r - 2..=r + 2 {
            for dx in -r - 2..=r + 2 {
                let p = [x + dx, 32 + h + dy, z + dz];
                if crown(dx, dy, dz, r, index) && s.block(p) == Some(world::AIR) {
                    s.put(p, world::LEAVES);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
