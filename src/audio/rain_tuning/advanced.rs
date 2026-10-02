//! Sound profiles adapted from NoiseMachine (MIT, Copyright 2026 kvmet).
//! Bounds also describe the editable controls, keeping UI and DSP validation aligned.
use super::range;
macro_rules! profile {
    ($name:ident { $( $field:ident: $default:expr, $label:literal, $low:expr, $high:expr, $unit:literal; )* }) => {
        #[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        #[serde(default)]
        pub struct $name { $(pub $field: f32,)* }
        impl Default for $name {
            fn default() -> Self { Self { $($field: $default,)* } }
        }
        impl $name {
            pub(crate) fn controls(&mut self, mut visit: impl FnMut(&'static str, &mut f32, std::ops::RangeInclusive<f32>, &'static str)) {
                $(let bounds = ($low)(self)..=($high)(self);
                visit($label, &mut self.$field, bounds, $unit);)*
            }
            pub(crate) fn valid(self) -> bool {
                let mut copy = self;
                let mut valid = true;
                copy.controls(|_, v, r, _| valid &= super::range(*v, *r.start(), *r.end()));
                valid
            }
            pub(crate) fn sanitized(self) -> Self {
                if self.valid() { self } else { Self::default() }
            }
        }
    };
}
mod ambient;
mod preview;
pub use ambient::*;
pub use preview::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Advanced {
    pub wind: WindProfile,
    pub crickets: CricketProfile,
    pub cicadas: CicadaProfile,
    pub listener: ListenerProfile,
    pub thunder: ThunderProfile,
    pub reverb: AmbientReverb,
    pub preview: Preview,
}
impl Advanced {
    pub(crate) fn valid(self) -> bool {
        self.wind.valid()
            && self.crickets.valid()
            && self.cicadas.valid()
            && self.listener.valid()
            && self.thunder.valid()
            && self.reverb.valid()
            && self.preview.valid()
    }
    pub(crate) fn sanitized(mut self) -> Self {
        self.wind = self.wind.sanitized();
        self.crickets = self.crickets.sanitized();
        self.cicadas = self.cicadas.sanitized();
        self.listener = self.listener.sanitized();
        self.thunder = self.thunder.sanitized();
        self.reverb = self.reverb.sanitized();
        self.preview = self.preview.sanitized();
        self
    }
}
