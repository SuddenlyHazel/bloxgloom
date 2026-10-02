profile! { FixedWeather {
    rain_mm_h: 10.0, "Rain intensity", |_: &Self| 0.0, |_: &Self| 200.0, " mm/h";
    wind_m_s: 3.0, "Mean wind", |_: &Self| 0.0, |_: &Self| 40.0, " m/s";
    wind_bearing_rad: 0.0, "Wind bearing", |_: &Self| -std::f32::consts::PI, |_: &Self| std::f32::consts::PI, " rad";
    temperature_c: 25.0, "Temperature", |_: &Self| -10.0, |_: &Self| 45.0, " °C";
    daylight: 0.5, "Daylight", |_: &Self| 0.0, |_: &Self| 1.0, "";
    lightning_per_min: 0.0, "Lightning rate", |_: &Self| 0.0, |_: &Self| 30.0, " /min";
    distance_m: 5000.0, "Lightning cell distance", |_: &Self| 200.0, |_: &Self| 30000.0, " m";
    angle_rad: 0.0, "Lightning cell bearing", |_: &Self| -std::f32::consts::PI, |_: &Self| std::f32::consts::PI, " rad";
}}
profile! { Climate {
    time_scale: 60.0, "Storm time scale", |_: &Self| 1.0, |_: &Self| 600.0, "×";
    temperature_c: 25.0, "Climate temperature", |_: &Self| -10.0, |_: &Self| 45.0, " °C";
    min_severity: 0.2, "Minimum severity", |_: &Self| 0.0, |s: &Self| s.max_severity.min(1.0), "";
    max_severity: 0.8, "Maximum severity", |s: &Self| s.min_severity, |_: &Self| 1.0, "";
    storms_per_hour: 0.5, "Storm frequency", |_: &Self| 0.0, |_: &Self| 4.0, " /h";
    cell_speed_m_s: 10.0, "Cell travel speed", |_: &Self| 3.0, |_: &Self| 30.0, " m/s";
    breeze_m_s: 2.0, "Background breeze", |_: &Self| 0.0, |_: &Self| 15.0, " m/s";
    gust_intensity: 0.3, "Gust intensity", |_: &Self| 0.0, |_: &Self| 1.0, "";
    gust_time_s: 4.0, "Gust duration", |_: &Self| 0.5, |_: &Self| 30.0, " s";
}}
profile! { StormShape {
    peak_rain_min_mm_h: 2.0, "Peak rain minimum", |_: &Self| 0.1, |s: &Self| s.peak_rain_max_mm_h.min(200.0), " mm/h";
    peak_rain_max_mm_h: 150.0, "Peak rain maximum", |s: &Self| s.peak_rain_min_mm_h, |_: &Self| 200.0, " mm/h";
    core_along_m: 3000.0, "Core depth", |_: &Self| 500.0, |_: &Self| 20000.0, " m";
    core_across_m: 10000.0, "Core width", |_: &Self| 500.0, |_: &Self| 50000.0, " m";
    tail_share: 0.1, "Trailing rain share", |_: &Self| 0.0, |_: &Self| 1.0, "";
    tail_length_m: 15000.0, "Tail length", |_: &Self| 1000.0, |_: &Self| 50000.0, " m";
    tail_width_m: 15000.0, "Tail width", |_: &Self| 1000.0, |_: &Self| 50000.0, " m";
    front_min_m: 4000.0, "Front minimum", |_: &Self| 0.0, |s: &Self| s.front_max_m.min(20000.0), " m";
    front_max_m: 8000.0, "Front maximum", |s: &Self| s.front_min_m, |_: &Self| 20000.0, " m";
    front_edge_m: 1500.0, "Front edge", |_: &Self| 100.0, |_: &Self| 10000.0, " m";
    outflow_min_m_s: 4.0, "Outflow minimum", |_: &Self| 0.0, |s: &Self| s.outflow_max_m_s.min(40.0), " m/s";
    outflow_max_m_s: 24.0, "Outflow maximum", |s: &Self| s.outflow_min_m_s, |_: &Self| 40.0, " m/s";
    outflow_decay_m: 6000.0, "Outflow decay distance", |_: &Self| 500.0, |_: &Self| 50000.0, " m";
    outflow_width_m: 12000.0, "Outflow width", |_: &Self| 1000.0, |_: &Self| 50000.0, " m";
    cooling_min_c: 3.0, "Cooling minimum", |_: &Self| 0.0, |s: &Self| s.cooling_max_c.min(20.0), " °C";
    cooling_max_c: 10.0, "Cooling maximum", |s: &Self| s.cooling_min_c, |_: &Self| 20.0, " °C";
    cooling_decay_m: 25000.0, "Cooling decay distance", |_: &Self| 1000.0, |_: &Self| 100000.0, " m";
    cooling_width_m: 15000.0, "Cooling width", |_: &Self| 1000.0, |_: &Self| 50000.0, " m";
    cooling_s: 240.0, "Cooling time", |_: &Self| 10.0, |_: &Self| 3600.0, " s";
    warming_s: 2400.0, "Warming time", |_: &Self| 60.0, |_: &Self| 36000.0, " s";
    lightning_min_per_min: 0.5, "Lightning minimum", |_: &Self| 0.0, |s: &Self| s.lightning_max_per_min.min(30.0), " /min";
    lightning_max_per_min: 12.0, "Lightning maximum", |s: &Self| s.lightning_min_per_min, |_: &Self| 30.0, " /min";
    build_share: 0.3, "Growth share", |_: &Self| 0.05, |s: &Self| (1.0-s.decay_share).min(0.9), "";
    decay_share: 0.35, "Decay share", |_: &Self| 0.05, |s: &Self| (1.0-s.build_share).min(0.95), "";
    approach_m: 40000.0, "Track approach", |_: &Self| 10000.0, |_: &Self| 100000.0, " m";
    miss_m: 10000.0, "Track miss distance", |_: &Self| 0.0, |_: &Self| 30000.0, " m";
    heading_spread_rad: 0.35, "Heading spread", |_: &Self| 0.0, |_: &Self| std::f32::consts::PI, " rad";
}}
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preview {
    /// Overrides any selected local preset. Never overrides server weather.
    pub manual: bool,
    pub fixed: FixedWeather,
    pub climate: Climate,
    pub shape: StormShape,
}
impl Preview {
    pub(crate) fn valid(self) -> bool {
        self.fixed.valid() && self.climate.valid() && self.shape.valid()
    }
    pub(crate) fn sanitized(mut self) -> Self {
        self.fixed = self.fixed.sanitized();
        self.climate = self.climate.sanitized();
        self.shape = self.shape.sanitized();
        self
    }
}
