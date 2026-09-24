use super::*;

#[test]
fn steady_window_requires_enough_samples_and_has_a_bounded_runtime() {
    assert!(validate_tick_count(299).is_err());
    assert!(validate_tick_count(300).is_ok());
    assert!(validate_tick_count(MAX_STEADY_TICKS).is_ok());
    assert!(validate_tick_count(MAX_STEADY_TICKS + 1).is_err());
}

#[test]
fn scenarios_really_cover_clustered_and_spread_player_layouts() {
    let clustered: HashSet<_> = Scenario::Clustered
        .rough_positions()
        .into_iter()
        .map(|position| world_to_chunk(position[0] as i32, 0, position[2] as i32).0)
        .collect();
    let spread: HashSet<_> = Scenario::Spread
        .rough_positions()
        .into_iter()
        .map(|position| world_to_chunk(position[0] as i32, 0, position[2] as i32).0)
        .collect();
    assert_eq!(clustered.len(), 1);
    assert_eq!(spread.len(), PLAYER_COUNT);
}
