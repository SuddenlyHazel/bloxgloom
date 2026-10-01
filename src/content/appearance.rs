//! Frozen palette identity and constant-time public byte lookup. Index zero and
//! all other builtin indices remain unchanged when a package appends colors.
use super::{Catalog, RegistrationError};
use bloxgloom_host_api::appearance::Appearance;

pub(crate) const SKINS: &[[f32; 3]] = &[
    [0.91, 0.68, 0.49],
    [0.75, 0.50, 0.32],
    [0.59, 0.37, 0.24],
    [0.40, 0.25, 0.18],
    [0.97, 0.79, 0.61],
    [0.67, 0.43, 0.32],
];
pub(crate) const SHIRTS: &[[f32; 3]] = &[
    [0.13, 0.39, 0.69],
    [0.70, 0.23, 0.18],
    [0.19, 0.55, 0.38],
    [0.67, 0.49, 0.17],
    [0.42, 0.30, 0.61],
    [0.17, 0.57, 0.62],
    [0.63, 0.34, 0.45],
    [0.38, 0.46, 0.55],
];
pub(crate) const PANTS: &[[f32; 3]] = &[
    [0.14, 0.20, 0.35],
    [0.22, 0.25, 0.29],
    [0.30, 0.25, 0.21],
    [0.23, 0.32, 0.26],
    [0.34, 0.29, 0.43],
    [0.25, 0.33, 0.43],
];

impl Catalog {
    pub(crate) fn register_player_appearance(
        &mut self,
        appearance: Appearance,
    ) -> Result<(), RegistrationError> {
        appearance
            .validate()
            .map_err(|_| RegistrationError::InvalidDefinition)?;
        if self.player_appearance.is_some() {
            return Err(RegistrationError::DuplicateKey);
        }
        self.player_appearance = Some(appearance);
        Ok(())
    }

    pub(crate) fn appearance_color(&self, part: usize, index: u8) -> Option<[f32; 3]> {
        let builtin = *[SKINS, SHIRTS, PANTS].get(part)?;
        let index = usize::from(index);
        if index < builtin.len() {
            return Some(builtin[index]);
        }
        self.player_appearance.as_ref()?.palettes[part]
            .get(index - builtin.len())
            .copied()
    }

    pub(crate) fn valid_appearance_state(
        &self,
        appearance: crate::appearance::AppearanceState,
    ) -> bool {
        self.valid_appearance(appearance.legacy())
            && appearance.character.is_none_or(|recipe| recipe.valid())
    }

    pub(crate) fn valid_appearance(&self, appearance: [u8; 4]) -> bool {
        appearance[3] == 0
            && (0..3).all(|part| self.appearance_color(part, appearance[part]).is_some())
    }
}
