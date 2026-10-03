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
    /// Public package rig and appearance captured with the directory fence.
    pub model: Option<String>,
    pub model_visual: Option<crate::entity::VisualState>,
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
            if let Some(player) = players
                .iter_mut()
                .find(|p| p.profile == operation.profile && p.session == operation.session)
            {
                match operation.kind {
                    super::PlayerOperationKind::Appearance(palettes) => {
                        player.appearance = [palettes[0], palettes[1], palettes[2], 0]
                    }
                    super::PlayerOperationKind::Teleport(position) => player.position = position,
                    super::PlayerOperationKind::HealthChanged {
                        respawn_position: Some(position),
                        ..
                    } => player.position = position,
                    super::PlayerOperationKind::Model(ref model) => {
                        player.model = model.clone();
                        player.model_visual = model
                            .as_ref()
                            .map(|_| crate::entity::VisualState::default());
                    }
                    super::PlayerOperationKind::ModelVisual(visual) => {
                        player.model_visual = Some(visual)
                    }
                    super::PlayerOperationKind::Animation {
                        ref clip,
                        speed,
                        looping,
                        crossfade_s,
                    } => {
                        if let (Some(model), Some(visual)) =
                            (&player.model, &mut player.model_visual)
                            && let Some(schema) = self.snapshot.player_model_schema(model)
                            && let Some(index) = schema.clips.iter().position(|name| name == clip)
                            && let Some(sequence) = visual.sequence.checked_add(1)
                        {
                            visual.sequence = sequence;
                            visual.sample_tick = self.snapshot.tick();
                            visual.playback = Some(crate::entity::ClipPlayback {
                                clip: index as u16,
                                speed,
                                looping,
                                crossfade_s,
                                started_tick: visual.sample_tick,
                                sequence,
                            });
                        }
                    }
                    super::PlayerOperationKind::StopAnimation(crossfade_s) => {
                        if let Some(visual) = &mut player.model_visual {
                            visual.playback = None;
                            visual.transition_s = crossfade_s;
                        }
                    }
                    _ => {}
                }
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
