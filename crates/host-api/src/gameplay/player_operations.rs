//! Authorized receipt-bound native player intents, separate from identity queries.
use super::{Context, Error};

/// Apply once after receipt to the exact live session, without crash replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerOperation {
    pub profile: u128,
    pub session: u64,
    pub kind: PlayerOperationKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerOperationKind {
    Message(String),
    Kick(String),
    /// Saved through the native profile cosmetic file before avatar publication.
    Appearance([u8; 3]),
}
impl Context<'_> {
    pub fn message_player(&mut self, profile: u128, session: u64, text: &str) -> Result<(), Error> {
        self.notice(profile, session, text, false)
    }
    pub fn kick_player(&mut self, profile: u128, session: u64, reason: &str) -> Result<(), Error> {
        self.notice(profile, session, reason, true)
    }
    pub fn set_player_appearance(
        &mut self,
        profile: u128,
        session: u64,
        palettes: [u8; 3],
    ) -> Result<(), Error> {
        self.player_target(profile, session)?;
        if !self.snapshot.valid_player_appearance(palettes) {
            return self.fail(Error::Invalid("unregistered appearance palette".into()));
        }
        self.plan.player_operations.push(PlayerOperation {
            profile,
            session,
            kind: PlayerOperationKind::Appearance(palettes),
        });
        Ok(())
    }
    fn notice(
        &mut self,
        profile: u128,
        session: u64,
        text: &str,
        kicked: bool,
    ) -> Result<(), Error> {
        self.player_target(profile, session)?;
        if text.is_empty() || text.len() > 255 || text.chars().any(char::is_control) {
            return self.fail(Error::Invalid("invalid player operation text".into()));
        }
        let kind = if kicked {
            PlayerOperationKind::Kick(text.into())
        } else {
            PlayerOperationKind::Message(text.into())
        };
        self.plan.player_operations.push(PlayerOperation {
            profile,
            session,
            kind,
        });
        Ok(())
    }
    fn player_target(&mut self, profile: u128, session: u64) -> Result<(), Error> {
        self.charge()?;
        if !self
            .handler_namespace
            .as_deref()
            .is_some_and(|ns| self.snapshot.player_authority(ns))
        {
            return self.fail(Error::Invalid("player operation authority denied".into()));
        }
        if self.plan.player_operations.len() >= 64 {
            return self.fail(Error::Invalid("player operation limit exceeded".into()));
        }
        let players = match self.snapshot.players() {
            Ok(players) => players,
            Err(error) => return self.fail(error),
        };
        if !players
            .iter()
            .any(|p| p.profile == profile && p.session == session && session != 0)
        {
            return self.fail(Error::Invalid("player session is not online".into()));
        }
        Ok(())
    }
}
