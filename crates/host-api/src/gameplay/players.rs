//! Captured online player identity; profile claims are not account authentication.
use super::{Context, Error};

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub profile: u128,
    /// Server-issued durable action epoch, scoped to this profile.
    pub session: u64,
    pub entity: u64,
    pub name: String,
    pub position: [f32; 3],
}

/// Receipt-bound session effects. They do not replay after crash/reconnect.
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
}

impl Context<'_> {
    pub fn message_player(&mut self, profile: u128, session: u64, text: &str) -> Result<(), Error> {
        self.player_operation(profile, session, text, false)
    }
    pub fn kick_player(&mut self, profile: u128, session: u64, reason: &str) -> Result<(), Error> {
        self.player_operation(profile, session, reason, true)
    }
    fn player_operation(
        &mut self,
        profile: u128,
        session: u64,
        text: &str,
        kicked: bool,
    ) -> Result<(), Error> {
        self.charge()?;
        if !self
            .handler_namespace
            .as_deref()
            .is_some_and(|ns| self.snapshot.player_authority(ns))
        {
            return self.fail(Error::Invalid("player operation authority denied".into()));
        }
        if text.is_empty()
            || text.len() > 255
            || text.chars().any(char::is_control)
            || self.plan.player_operations.len() >= 64
        {
            return self.fail(Error::Invalid("invalid player operation or limit".into()));
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
    pub fn player_profile(&self) -> Option<u128> {
        self.snapshot.player()
    }
    /// Captured directory in ascending profile order. No live client objects.
    pub fn players(&mut self) -> Result<Vec<Player>, Error> {
        self.charge()?;
        self.snapshot.players()
    }
    pub fn player_by_profile(&mut self, profile: u128) -> Result<Option<Player>, Error> {
        Ok(self.players()?.into_iter().find(|p| p.profile == profile))
    }
    pub fn player_by_session(
        &mut self,
        profile: u128,
        session: u64,
    ) -> Result<Option<Player>, Error> {
        Ok(self
            .player_by_profile(profile)?
            .filter(|p| p.session == session))
    }
}
