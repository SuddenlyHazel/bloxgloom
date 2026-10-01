use super::*;
#[test]
fn low_lightning_rates_do_not_acquire_a_24_bit_probability_floor() {
    assert!(!lightning_hit(0, 0.0));
    assert!(!lightning_hit(0, 0.0001));
    assert!(lightning_hit(0, 0.001));
    assert!(!lightning_hit(1, 0.001));
    assert!(!lightning_hit(u32::MAX, 30.0));
}
#[test]
fn automatic_lightning_skips_distant_cells_without_admitting_a_voice() {
    let mut engine = Procedural::new(3);
    engine.weather = Weather {
        lightning: 1.0,
        distance: 1_000_000.0,
        ..Weather::default()
    };
    engine.frame = 1;
    engine.next(Preset::Storm);
    assert_eq!(engine.thunder.active_voices(), 0);
    assert_eq!(engine.thunder.rejected(), 0);
}
