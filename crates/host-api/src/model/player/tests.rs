use super::*;
#[test]
fn player_metadata_round_trips_and_rejects_truncation_unknown_versions_and_nonfinite_values() {
    let player = PlayerModel {
        idle: Some("Idle".into()),
        run: Some("Run".into()),
        first_person_hide: vec!["Head".into()],
        first_person_offset: [0.1, 0.0, 0.2],
        ..Default::default()
    };
    let bytes = player.encode().unwrap();
    assert_eq!(PlayerModel::decode(&bytes).unwrap(), player);
    for n in 0..bytes.len() {
        assert!(PlayerModel::decode(&bytes[..n]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(PlayerModel::decode(&trailing).is_err());
    let mut bad = player.clone();
    bad.crossfade_s = f32::NAN;
    assert!(bad.encode().is_err());
    bad = player;
    bad.first_person_hide.push("Head".into());
    assert!(bad.encode().is_err());
}
