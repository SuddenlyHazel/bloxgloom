//! Continuous regional controls shared by terrain, ecology and drainage.
//! These are authored noise curves, not an erosion simulation.
use super::{Biome, Column, lerp, noise2, smooth};

pub(super) const SEA_LEVEL: i64 = 16;

/// Zero contours give connected, turning channels; a second field adds streams
/// that meet the primary network. Absolute coordinates make every seam exact.
pub(super) fn drainage(x: i64, z: i64, seed: u64) -> f64 {
    let warp_x = (noise2(x, z, 384, seed ^ 0x2a97_106b) * 56.0) as i64;
    let warp_z = (noise2(x, z, 384, seed ^ 0x568b_c91a) * 56.0) as i64;
    let main = noise2(x + warp_x, z + warp_z, 256, seed ^ 0x713a_c951).abs();
    let tributary = noise2(x - warp_z, z + warp_x, 128, seed ^ 0xc4b1_095e).abs();
    // Tributaries stop where their catchment is dry, rather than covering the
    // whole world with a uniform grid of channels.
    let catchment = noise2(x, z, 512, seed ^ 0x715e_a51d);
    main.min(tributary + (-catchment - 0.05).max(0.0))
}

fn curve(value: f64, points: &[(f64, f64)]) -> f64 {
    for pair in points.windows(2) {
        if value <= pair[1].0 {
            let t = smooth(((value - pair[0].0) / (pair[1].0 - pair[0].0)).clamp(0.0, 1.0));
            return lerp(pair[0].1, pair[1].1, t);
        }
    }
    points.last().unwrap().1
}

pub(super) fn column(x: i64, z: i64, seed: u64) -> Column {
    let continent = noise2(x, z, 768, seed ^ 0x42ab_51a4);
    let temperature = noise2(x, z, 384, seed ^ 0x8179_e6f2);
    let moisture = noise2(x, z, 320, seed ^ 0x6a03_d2e1);
    let uplift = noise2(x, z, 512, seed ^ 0x9b57_2a13);
    let erosion = noise2(x, z, 288, seed ^ 0x0efa_329c);
    let hills = noise2(x, z, 72, seed ^ 0xd88a_4f9b);
    let detail = noise2(x, z, 24, seed ^ 0x7c14_1583);
    let raw_ridge = noise2(x, z, 128, seed ^ 0xe7d2_391f);
    let ridge = 1.0 - raw_ridge.abs();
    let mountain = smooth(((uplift + 0.08) / 0.70).clamp(0.0, 1.0));
    let inland = smooth(((continent + 0.42) / 0.42).clamp(0.0, 1.0));
    let base = curve(
        continent,
        &[
            (-1.0, -8.0),
            (-0.48, 10.0),
            (-0.28, 19.0),
            (0.10, 27.0),
            (1.0, 36.0),
        ],
    );
    let valley = smooth((drainage(x, z, seed) / 0.20).clamp(0.0, 1.0));
    let plateau = smooth(((erosion - 0.08) / 0.30).clamp(0.0, 1.0));
    let relief = mountain * inland * valley;
    let peaks = ridge.powi(3) * (46.0 - plateau * 22.0);
    let terrace = curve(
        erosion,
        &[(-1.0, 0.0), (0.05, 0.0), (0.25, 22.0), (1.0, 22.0)],
    );
    let aridity =
        ((temperature + 0.10) * 1.5).clamp(0.0, 1.0) * ((-moisture + 0.10) * 1.5).clamp(0.0, 1.0);
    // Directional dunes keep broad windward shoulders and narrow crests.
    let dune = noise2(x + z / 3, z / 2, 48, seed ^ 0xb62d_7a35);
    let dunes = (1.0 - dune.abs()).powi(3) * 5.0 - 1.2;
    let mut elevation = base
        + hills * (3.5 + relief * 4.0)
        + detail * (0.8 + relief * 1.8)
        + relief * (peaks + terrace)
        + aridity * dunes;
    // A broad, smooth starter meadow retains safe authoritative spawn searching.
    let distance = ((x as f64).powi(2) + (z as f64).powi(2)).sqrt();
    if distance < 48.0 {
        elevation = lerp(
            23.0 + hills * 1.5,
            elevation,
            smooth(((distance - 16.0) / 32.0).clamp(0.0, 1.0)),
        );
    }
    let height = elevation.round().clamp(-10.0, 88.0) as i64;
    let biome = if relief > 0.48 && height > 38 {
        Biome::Highland
    } else if temperature < -0.23 || height > 74 {
        Biome::Tundra
    } else if temperature > 0.12 && moisture < -0.12 {
        Biome::Desert
    } else if moisture > 0.08 {
        Biome::Forest
    } else {
        Biome::Plains
    };
    // Ridge/hill derivatives estimate exposed faces without four extra complete
    // terrain evaluations per column. Ecological slope is intentionally coarse.
    let gradient = |scale, salt| {
        let gx =
            (noise2(x + 2, z, scale, seed ^ salt) - noise2(x - 2, z, scale, seed ^ salt)) / 4.0;
        let gz =
            (noise2(x, z + 2, scale, seed ^ salt) - noise2(x, z - 2, scale, seed ^ salt)) / 4.0;
        (gx * gx + gz * gz).sqrt()
    };
    let slope = gradient(128, 0xe7d2_391f) * relief * ridge.powi(2) * 138.0
        + gradient(72, 0xd88a_4f9b) * (3.5 + relief * 4.0);
    Column {
        height,
        rocky: slope > 0.58 || (biome == Biome::Highland && height > 58),
        biome,
        water_level: None,
        water_kind: None,
        shore: false,
        rock_region: (continent * 8.0).round() as i64,
        strata_offset: (hills * 7.0).round() as i64,
        temperature,
        moisture,
        slope,
    }
}
