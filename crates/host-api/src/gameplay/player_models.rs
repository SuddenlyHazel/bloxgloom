//! Receipt-bound controls for package player rigs and baked GLB clips.
use super::{Context, Error, PlayerOperation, PlayerOperationKind};
use crate::entity::{VisualSchema, VisualState};

impl Context<'_> {
    pub fn player_model(
        &mut self,
        profile: u128,
        session: u64,
    ) -> Result<Option<(String, VisualState)>, Error> {
        self.player_target(profile, session)?;
        Ok(self
            .players()?
            .into_iter()
            .find(|p| p.profile == profile && p.session == session)
            .and_then(|p| p.model.zip(p.model_visual)))
    }

    pub fn player_model_schema(&mut self, key: &str) -> Result<VisualSchema, Error> {
        self.charge()?;
        if !self
            .handler_namespace
            .as_deref()
            .is_some_and(|ns| self.snapshot.player_authority(ns))
        {
            return self.fail(Error::Invalid("player model authority denied".into()));
        }
        match self.snapshot.player_model_schema(key) {
            Some(schema) => Ok(schema),
            None => self.fail(Error::Invalid("unregistered player model".into())),
        }
    }

    pub fn set_player_model(
        &mut self,
        profile: u128,
        session: u64,
        model: Option<&str>,
    ) -> Result<(), Error> {
        self.player_target(profile, session)?;
        if model.is_some_and(|key| self.snapshot.player_model_schema(key).is_none()) {
            return self.fail(Error::Invalid("unregistered player model".into()));
        }
        self.plan.player_operations.push(PlayerOperation {
            profile,
            session,
            kind: PlayerOperationKind::Model(model.map(str::to_owned)),
        });
        Ok(())
    }

    pub fn set_player_model_visual(
        &mut self,
        profile: u128,
        session: u64,
        mut visual: VisualState,
    ) -> Result<(), Error> {
        let Some((model, before)) = self.player_model(profile, session)? else {
            return self.fail(Error::Invalid(
                "select a packaged player model before changing its look".into(),
            ));
        };
        visual.playback = before.playback;
        visual.sequence = before.sequence;
        visual.sample_tick = before.sample_tick;
        if !self
            .snapshot
            .player_model_schema(&model)
            .is_some_and(|schema| schema.accepts(&visual))
        {
            return self.fail(Error::Invalid("invalid player model controls".into()));
        }
        self.plan.player_operations.push(PlayerOperation {
            profile,
            session,
            kind: PlayerOperationKind::ModelVisual(visual),
        });
        Ok(())
    }

    pub fn play_player_animation(
        &mut self,
        profile: u128,
        session: u64,
        clip: &str,
        speed: f32,
        looping: bool,
        crossfade_s: f32,
    ) -> Result<(), Error> {
        let Some((model, visual)) = self.player_model(profile, session)? else {
            return self.fail(Error::Invalid(
                "baked player animation requires a packaged model".into(),
            ));
        };
        if !speed.is_finite()
            || !(0.0..=8.0).contains(&speed)
            || !crossfade_s.is_finite()
            || !(0.0..=5.0).contains(&crossfade_s)
            || visual.sequence == u32::MAX
            || !self
                .snapshot
                .player_model_schema(&model)
                .is_some_and(|schema| schema.clips.iter().any(|name| name == clip))
        {
            return self.fail(Error::Invalid("invalid baked player animation".into()));
        }
        self.plan.player_operations.push(PlayerOperation {
            profile,
            session,
            kind: PlayerOperationKind::Animation {
                clip: clip.into(),
                speed,
                looping,
                crossfade_s,
            },
        });
        Ok(())
    }
    pub fn stop_player_animation(
        &mut self,
        profile: u128,
        session: u64,
        crossfade_s: f32,
    ) -> Result<(), Error> {
        if self.player_model(profile, session)?.is_none()
            || !crossfade_s.is_finite()
            || !(0.0..=5.0).contains(&crossfade_s)
        {
            return self.fail(Error::Invalid("invalid player animation stop".into()));
        }
        self.plan.player_operations.push(PlayerOperation {
            profile,
            session,
            kind: PlayerOperationKind::StopAnimation(crossfade_s),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
