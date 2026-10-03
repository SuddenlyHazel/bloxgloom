//! Self-contained model declarations. The host validates and prepares these at
//! startup; scripts cannot supply filesystem paths or mutate installed assets.
use crate::RegistrationError;
mod player;
pub use player::PlayerModel;

pub const MAX_GLB_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CONTROLS_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct ModelAsset {
    pub player: Option<PlayerModel>,
    pub key: String,
    pub glb: Vec<u8>,
    /// Native named variants, layers, material tints and clip loop intent JSON.
    pub controls: Vec<u8>,
    pub scale: f32,
}

impl ModelAsset {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if !crate::sound::key(&self.key)
            || self.glb.len() > MAX_GLB_BYTES
            || !self.glb.starts_with(b"glTF")
            || self.controls.len() > MAX_CONTROLS_BYTES
            || !self.scale.is_finite()
            || !(0.001..=100.0).contains(&self.scale)
        {
            return Err(RegistrationError(
                "invalid bounded GLB model declaration".into(),
            ));
        }
        if let Some(player) = &self.player {
            player.validate()?;
        }
        Ok(())
    }
}
