//! Frozen presentation metadata. Profiles never grant gameplay authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum RainSurface {
    Water,
    Dirt,
    Leaf,
    Concrete,
    Glass,
    Metal,
    Plastic,
    Asphalt,
    AsphaltRoof,
    Wood,
}
impl RainSurface {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "water" => Self::Water,
            "dirt" => Self::Dirt,
            "leaf" => Self::Leaf,
            "stone" | "concrete" => Self::Concrete,
            "glass" => Self::Glass,
            "metal" => Self::Metal,
            "plastic" => Self::Plastic,
            "asphalt" => Self::Asphalt,
            "roof" => Self::AsphaltRoof,
            "wood" => Self::Wood,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Habitat {
    #[default]
    None,
    Ground,
    Canopy,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpactProfile {
    pub gain: f32,
    pub click: f32,
    pub frequency_hz: [f32; 2],
    pub damping_per_s: [f32; 2],
    pub resonance: f32,
    pub lowpass_hz: f32,
}
impl ImpactProfile {
    pub fn valid(self) -> bool {
        let range = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
        range(self.gain, 0.0, 2.0)
            && range(self.click, 0.0, 2.0)
            && self
                .frequency_hz
                .into_iter()
                .all(|v| range(v, 50.0, 16_000.0))
            && self
                .damping_per_s
                .into_iter()
                .all(|v| range(v, 10.0, 2_000.0))
            && range(self.resonance, 0.0, 2.0)
            && range(self.lowpass_hz, 100.0, 18_000.0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Acoustics {
    pub surface: RainSurface,
    pub habitat: Habitat,
    pub impact: Option<ImpactProfile>,
}
impl Acoustics {
    pub fn valid(self) -> bool {
        self.impact.is_none_or(ImpactProfile::valid)
    }
    /// Fixed-length canonical representation, used in compatibility fingerprints.
    pub fn bytes(self) -> Vec<u8> {
        let mut out = vec![
            self.surface as u8,
            self.habitat as u8,
            u8::from(self.impact.is_some()),
        ];
        if let Some(p) = self.impact {
            for v in [
                p.gain,
                p.click,
                p.frequency_hz[0],
                p.frequency_hz[1],
                p.damping_per_s[0],
                p.damping_per_s[1],
                p.resonance,
                p.lowpass_hz,
            ] {
                out.extend(v.to_le_bytes());
            }
        }
        out
    }
}
