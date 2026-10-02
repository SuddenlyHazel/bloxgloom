use super::*;
#[test]
fn held_weather_and_gusts_are_live_without_advancing_cells() {
    let mut c = Preview {
        manual: true,
        ..Default::default()
    };
    c.climate.gust_intensity = 0.0;
    c.fixed.rain_mm_h = 75.0;
    c.fixed.wind_m_s = 12.0;
    c.fixed.temperature_c = 30.0;
    let mut s = Storm::with_config(42, c);
    let position = s.cells[0].position;
    for _ in 0..200 {
        let w = s.tick();
        assert_eq!(w.rain, 75.0);
        assert_eq!(w.wind, 12.0);
        assert_eq!(w.temperature, 30.0);
    }
    assert_eq!(position, s.cells[0].position);
    c.climate.gust_intensity = 1.0;
    s.configure(c);
    assert!((s.tick().wind - 12.0).abs() > 0.0);
}
#[test]
fn shape_and_climate_controls_change_a_seeded_storm() {
    let mut a = Storm::new(42);
    let mut b = Storm::new(42);
    let initial = a.tick();
    assert_eq!(initial, b.tick());
    assert!(initial.rain > 0.0);
    let mut c = Preview::default();
    c.shape.peak_rain_min_mm_h = 0.1;
    c.shape.peak_rain_max_mm_h = 0.1;
    c.shape.outflow_min_m_s = 0.0;
    c.shape.outflow_max_m_s = 0.0;
    c.climate.breeze_m_s = 0.0;
    c.climate.gust_intensity = 0.0;
    c.shape.lightning_min_per_min = 0.0;
    c.shape.lightning_max_per_min = 0.0;
    c.climate.temperature_c = 40.0;
    b.configure(c);
    let changed = b.tick();
    assert!(changed.rain < initial.rain);
    assert_eq!(changed.wind, 0.0);
    assert_eq!(changed.lightning, 0.0);
    assert!(changed.temperature > initial.temperature);
}
