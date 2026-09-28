//! One startup selection; the existing player entity identity commits its full
//! contract, so key changes and removals cannot become manifest tombstones.
use super::{Catalog, RegistrationError, valid_key};
use bloxgloom_host_api::player::{BUILTIN_RULES, PlayerRules};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Selection {
    pub(crate) key: String,
    pub(crate) revision: u32,
    pub(crate) rules: PlayerRules,
}

impl Selection {
    pub(crate) fn validate(&self) -> Result<(), RegistrationError> {
        if !valid_key(&self.key) || self.revision == 0 || self.rules.validate().is_err() {
            return Err(RegistrationError::InvalidDefinition);
        }
        Ok(())
    }

    pub(super) fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut bytes = b"player-rules/v1".to_vec();
        bytes.push(self.key.len() as u8);
        bytes.extend(self.key.as_bytes());
        bytes.extend(self.revision.to_le_bytes());
        bytes.extend(self.rules.canonical_bytes());
        bytes
    }
}

impl Catalog {
    pub(crate) fn select_player_rules(
        &mut self,
        selection: Selection,
    ) -> Result<(), RegistrationError> {
        selection.validate()?;
        if self.player_selection.is_some() {
            return Err(RegistrationError::DuplicateKey);
        }
        self.player_rules = selection.rules;
        self.player_selection = Some(selection);
        Ok(())
    }

    pub(super) fn validate_player_selection(&self) -> Result<(), RegistrationError> {
        match &self.player_selection {
            Some(selection) => {
                selection.validate()?;
                if self.entity_type_id_by_key("bloxgloom:player").is_none()
                    || selection.rules.canonical_bytes() != self.player_rules.canonical_bytes()
                {
                    return Err(RegistrationError::InvalidDefinition);
                }
            }
            None if self.player_rules.canonical_bytes() != BUILTIN_RULES.canonical_bytes() => {
                return Err(RegistrationError::InvalidDefinition);
            }
            None => {}
        }
        Ok(())
    }
}
