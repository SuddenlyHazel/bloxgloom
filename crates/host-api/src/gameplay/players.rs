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
    /// Frozen palette indices: skin, shirt, pants and reserved flags.
    pub appearance: [u8; 4],
}

impl Context<'_> {
    pub fn player_profile(&self) -> Option<u128> {
        self.snapshot.player()
    }
    /// Captured directory in ascending profile order. No live client objects.
    pub fn players(&mut self) -> Result<Vec<Player>, Error> {
        self.charge()?;
        let mut players = match self.snapshot.players() {
            Ok(players) => players,
            Err(error) => return self.fail(error),
        };
        for operation in &self.plan.player_operations {
            if let super::PlayerOperationKind::Appearance(palettes) = operation.kind
                && let Some(player) = players
                    .iter_mut()
                    .find(|p| p.profile == operation.profile && p.session == operation.session)
            {
                player.appearance = [palettes[0], palettes[1], palettes[2], 0];
            }
        }
        Ok(players)
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
