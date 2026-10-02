use super::*;

#[test]
fn profiles_round_trip_preserve_material_names_and_export_volumes() {
    let mut profile = RainConfig {
        bed_gain: 0.035,
        drop_gain: 1.4,
        ..Default::default()
    };
    profile.surfaces[9].modes[0].frequency_hz = 470.0;
    let encoded = serde_json::to_string(&profile).unwrap();
    let decoded: RainConfig = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.sanitized(), profile);
    let exported: serde_json::Value = serde_json::from_str(&profile.export(0.8, 0.6, 0.9)).unwrap();
    assert_eq!(exported["format"], "bloxgloom-rain-audio-v1");
    assert_eq!(exported["material_order"][9], "Wood");
    assert_eq!(exported["rain"]["drop_gain"], 1.4_f32);
    let round_trip: RainConfig = serde_json::from_value(exported["rain"].clone()).unwrap();
    assert_eq!(round_trip.sanitized(), profile);
}

#[test]
fn invalid_profiles_cannot_reach_the_synth() {
    for bad in [f32::NAN, f32::INFINITY, -1.0, 5.0] {
        assert_eq!(
            RainConfig {
                bed_gain: bad,
                ..Default::default()
            }
            .sanitized(),
            RainConfig::default()
        );
    }
    let mut profile = RainConfig::default();
    profile.surfaces[2].click_frequency_hz = [12000.0, 1000.0];
    assert_eq!(profile.sanitized(), RainConfig::default());
    assert_eq!(
        RainConfig {
            bed_gain: 0.0,
            drop_gain: 0.0,
            ..Default::default()
        }
        .sanitized()
        .drop_gain,
        0.0
    );
}
