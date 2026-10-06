//! The actual raster and secondary-ray helpers use one directional irradiance.
use super::*;

#[test]
fn gpu_primary_secondary_diffuse_share_directional_irradiance_units() {
    let fixture = Fixture::new(&Catalog::builtins());
    let rows = fixture.run(vec![], 11, 0.0, 12, 48);
    let albedo = [0.6f32, 0.36, 0.34];
    let solar = [1.728f32, 1.608, 1.344];
    let share = 0.15 + 0.65 * 0.653;
    for case in 0..12 {
        let angle = case % 6;
        let cosine = [1.0f32, 0.5, 0.0, -1.0, -0.5, 1.0][angle];
        let sky = if angle == 5 { 0.0 } else { 1.0 };
        for (channel, irradiance) in solar.into_iter().enumerate() {
            let r = if case >= 6 {
                albedo[channel] * (1.0 - share)
            } else {
                albedo[channel]
            };
            let t = if case >= 6 {
                albedo[channel].powf(0.2) * share
            } else {
                0.0
            };
            let scale = irradiance * sky * 0.96 * std::f32::consts::FRAC_1_PI;
            let expected_secondary = (r * cosine.max(0.0) + t * (-cosine).max(0.0)) * scale;
            // Raster botanical backscatter retains its established forward lobe.
            // Units agree; nonzero wrap/forward shaping is not a Lambert parity claim.
            let forward = 0.35 + 0.65 * (-cosine).max(0.0).powi(4);
            let expected_primary = (r * cosine.max(0.0) + t * (-cosine).max(0.0) * forward) * scale;
            let expected_thin = albedo[channel] * cosine.max(0.0) * scale;
            for (variant, expected) in [
                expected_primary,
                expected_secondary,
                expected_thin,
                irradiance,
            ]
            .into_iter()
            .enumerate()
            {
                assert!(
                    (rows[case * 4 + variant][channel] - expected).abs() < 0.00002,
                    "case={case} variant={variant} channel={channel}: {:?} expected {expected}",
                    rows[case * 4 + variant]
                );
            }
            if cosine >= 0.0 || cosine == -1.0 {
                assert!(
                    (rows[case * 4][channel] - rows[case * 4 + 1][channel]).abs() < 0.00002,
                    "unwrapped opaque/front and normal-back botanical lobes must share units"
                );
            }
        }
    }
}
