profile! { WindProfile {
    stereo_width: 0.5, "Stereo width", |_: &Self| 0.0, |_: &Self| 1.0, "";
    brightness: 1.0, "Brightness", |_: &Self| 0.25, |_: &Self| 4.0, "×";
    rumble: 1.0, "Rumble", |_: &Self| 0.0, |_: &Self| 2.0, "×";
    balance: 0.5, "Directional balance", |_: &Self| 0.0, |_: &Self| 1.0, "";
}}
profile! { CricketTone {
    gain: 1.0, "Cricket volume", |_: &Self| 0.0, |_: &Self| 4.0, "×";
    call_rate_scale: 0.5, "Call rate scale", |_: &Self| 0.1, |_: &Self| 2.0, "×";
    pitch_hz: 4500.0, "Pitch", |_: &Self| 2000.0, |_: &Self| 8000.0, " Hz";
    pitch_variation: 0.35, "Pitch variation", |_: &Self| 0.0, |_: &Self| 1.0, "";
    min_temperature_c: 13.0, "Minimum temperature", |_: &Self| -10.0, |_: &Self| 45.0, " °C";
    max_rain_mm_h: 0.5, "Maximum rain", |_: &Self| 0.0, |_: &Self| 200.0, " mm/h";
    max_wind_m_s: 8.0, "Maximum wind", |_: &Self| 0.0, |_: &Self| 40.0, " m/s";
}}
profile! { CicadaTone {
    gain: 1.0, "Cicada volume", |_: &Self| 0.0, |_: &Self| 4.0, "×";
    pitch_hz: 5000.0, "Pitch", |_: &Self| 1000.0, |_: &Self| 10000.0, " Hz";
    click_rate_scale: 1.0, "Click rate scale", |_: &Self| 0.5, |_: &Self| 1.5, "×";
    chorus: 0.35, "Chorus", |_: &Self| 0.0, |_: &Self| 1.0, "";
    min_temperature_c: 22.0, "Minimum temperature", |_: &Self| -10.0, |_: &Self| 45.0, " °C";
    max_rain_mm_h: 0.5, "Maximum rain", |_: &Self| 0.0, |_: &Self| 200.0, " mm/h";
}}
profile! { ListenerProfile {
    width_m: 0.18, "Ear spacing", |_: &Self| 0.0, |_: &Self| 0.5, " m";
    head_amount: 1.0, "Head shadow", |_: &Self| 0.0, |_: &Self| 1.0, "";
    rear_amount: 1.0, "Rear filtering", |_: &Self| 0.0, |_: &Self| 1.0, "";
}}
profile! { ThunderProfile {
    gain: 1.0, "Thunder volume", |_: &Self| 0.0, |_: &Self| 4.0, "×";
    reverb_gain: 0.5, "Thunder reverb", |_: &Self| 0.0, |_: &Self| 1.0, "";
    reverb_decay_s: 3.5, "Thunder reverb decay", |_: &Self| 0.5, |_: &Self| 20.0, " s";
    scatter_m: 4000.0, "Preview strike scatter", |_: &Self| 0.0, |_: &Self| 10000.0, " m";
    distance_m: 1200.0, "Test strike distance", |_: &Self| 200.0, |_: &Self| 15000.0, " m";
    angle_rad: 0.7, "Test strike bearing", |_: &Self| -std::f32::consts::PI, |_: &Self| std::f32::consts::PI, " rad";
}}
profile! { AmbientReverb {
    rain_decay_s: 0.65, "Impact decay", |_: &Self| 0.1, |_: &Self| 10.0, " s";
    insect_decay_s: 0.65, "Insect decay", |_: &Self| 0.1, |_: &Self| 10.0, " s";
    rain_damping: 0.16, "Impact damping", |_: &Self| 0.005, |_: &Self| 1.0, "";
    insect_damping: 0.16, "Insect damping", |_: &Self| 0.005, |_: &Self| 1.0, "";
    rain_return: 0.12, "Impact return", |_: &Self| 0.0, |_: &Self| 1.0, "";
    insect_return: 0.12, "Insect return", |_: &Self| 0.0, |_: &Self| 1.0, "";
}}
profile! { Placement {
    stereo_width: 1.0, "Preview source spread", |_: &Self| 0.0, |_: &Self| 1.0, "";
    min_distance_m: 0.25, "Near audible distance", |_: &Self| 0.25, |s: &Self| s.max_distance_m.min(100.0), " m";
    max_distance_m: 100.0, "Far audible distance", |s: &Self| s.min_distance_m, |_: &Self| 100.0, " m";
}}
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CricketProfile {
    pub tone: CricketTone,
    pub placement: Placement,
}
impl CricketProfile {
    pub(crate) fn valid(self) -> bool {
        self.tone.valid() && self.placement.valid()
    }
    pub(crate) fn sanitized(mut self) -> Self {
        self.tone = self.tone.sanitized();
        self.placement = self.placement.sanitized();
        self
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CicadaProfile {
    pub tone: CicadaTone,
    pub placement: Placement,
    pub species: CicadaSpecies,
}
impl CicadaProfile {
    pub(crate) fn valid(self) -> bool {
        self.tone.valid() && self.placement.valid()
    }
    pub(crate) fn sanitized(mut self) -> Self {
        self.tone = self.tone.sanitized();
        self.placement = self.placement.sanitized();
        self
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CicadaSpecies {
    #[default]
    DogDay,
    Minminzemi,
    Higurashi,
    Aburazemi,
    Niiniizemi,
    Kumazemi,
    Pharaoh,
    ScissorGrinder,
    CigaleGrise,
    GreenGrocer,
}
impl CicadaSpecies {
    pub const ALL: [Self; 10] = [
        Self::DogDay,
        Self::Minminzemi,
        Self::Higurashi,
        Self::Aburazemi,
        Self::Niiniizemi,
        Self::Kumazemi,
        Self::Pharaoh,
        Self::ScissorGrinder,
        Self::CigaleGrise,
        Self::GreenGrocer,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::DogDay => "Dog-day (US)",
            Self::Minminzemi => "Minminzemi (JP)",
            Self::Higurashi => "Higurashi (JP)",
            Self::Aburazemi => "Aburazemi (JP)",
            Self::Niiniizemi => "Niiniizemi (JP)",
            Self::Kumazemi => "Kumazemi (JP)",
            Self::Pharaoh => "Pharaoh (US)",
            Self::ScissorGrinder => "Scissor grinder (US)",
            Self::CigaleGrise => "Cigale grise (FR)",
            Self::GreenGrocer => "Green grocer (AU)",
        }
    }
    pub fn pitch(self) -> f32 {
        match self {
            Self::DogDay => 5000.0,
            Self::Minminzemi => 5000.0,
            Self::Higurashi => 5000.0,
            Self::Aburazemi => 4500.0,
            Self::Niiniizemi => 7500.0,
            Self::Kumazemi => 5000.0,
            Self::Pharaoh => 1400.0,
            Self::ScissorGrinder => 5500.0,
            Self::CigaleGrise => 4500.0,
            Self::GreenGrocer => 4000.0,
        }
    }
}
