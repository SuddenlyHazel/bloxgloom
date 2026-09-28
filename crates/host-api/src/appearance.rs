//! Startup-only humanoid palette additions. The original builtin palettes and
//! byte indices are never replaced. Colors are linear RGB, not client input.
pub const MAX_ADDITIONS: usize = 24;
pub const MODEL: &str = "bloxgloom:humanoid/v1";

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub key: String,
    pub revision: u32,
    pub model: String,
    /// Skin, shirt, pants, in that order. Each list appends to builtin indices.
    pub palettes: [Vec<[f32; 3]>; 3],
}

impl Appearance {
    pub fn validate(&self) -> Result<(), crate::RegistrationError> {
        if self.key.len() > 129
            || self.key.split_once(':').is_none_or(|(owner, name)| {
                [owner, name].iter().any(|part| {
                    part.is_empty()
                        || !part
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                })
            })
            || self.revision == 0
            || self.model != MODEL
            || self.palettes.iter().all(Vec::is_empty)
            || self.palettes.iter().any(|palette| {
                palette.len() > MAX_ADDITIONS
                    || palette
                        .iter()
                        .flatten()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v) || v.is_sign_negative())
            })
        {
            return Err(crate::RegistrationError("invalid player appearance".into()));
        }
        Ok(())
    }

    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut bytes = b"player-appearance/v1".to_vec();
        bytes.push(self.key.len() as u8);
        bytes.extend(self.key.as_bytes());
        bytes.extend(self.revision.to_le_bytes());
        bytes.extend(self.model.as_bytes());
        for palette in &self.palettes {
            bytes.push(palette.len() as u8);
            for color in palette {
                for component in color {
                    bytes.extend(component.to_le_bytes());
                }
            }
        }
        bytes
    }
}
