//! Health writes and transition hooks share the ordinary gameplay transaction.
use super::{Context, Error, Player, PlayerOperation, PlayerOperationKind, ProfileCell};
use crate::player_health::{self, Damage, Event, EventKind, State, View};
impl Context<'_> {
    /// Builtin host adapter only: coordinates come from captured server terrain.
    pub fn native_respawn_at_spawn(&mut self, revision: u64) -> Result<(), Error> {
        let profile = self
            .player_profile()
            .ok_or_else(|| Error::Invalid("respawn needs actor".into()))?;
        let player = self
            .players()?
            .into_iter()
            .find(|p| p.profile == profile)
            .ok_or_else(|| Error::Invalid("respawn actor unavailable".into()))?;
        let health = self.player_health(profile, player.session)?;
        if health.alive || health.revision != revision {
            return self.fail(Error::Invalid(
                "respawn requires current dead health revision".into(),
            ));
        }
        let position = match self.snapshot.native_respawn_position() {
            Ok(v) => v,
            Err(e) => return self.fail(e),
        };
        self.native_respawn_player(profile, player.session, revision, position)
    }

    pub fn player_health(&mut self, profile: u128, session: u64) -> Result<View, Error> {
        self.charge()?;
        self.health_player(profile, session)?;
        let cell = self.health_cell(profile)?;
        self.health_view(&cell)
    }
    fn health_player(&mut self, profile: u128, session: u64) -> Result<Player, Error> {
        if profile == 0 || session == 0 {
            return self.fail(Error::Invalid("invalid health target".into()));
        }
        let players = self.snapshot.players()?;
        match players
            .into_iter()
            .find(|p| p.profile == profile && p.session == session)
        {
            Some(player) => Ok(player),
            None => self.fail(Error::Invalid("health target session is not online".into())),
        }
    }
    fn health_cell(&mut self, profile: u128) -> Result<ProfileCell, Error> {
        let captured = match self.snapshot.player_health_cell(profile) {
            Ok(cell) => cell,
            Err(e) => return self.fail(e),
        };
        Ok(self
            .plan
            .profile_states
            .get(&(player_health::PROFILE_SYSTEM.into(), profile))
            .cloned()
            .unwrap_or(captured))
    }
    fn health_view(&mut self, cell: &ProfileCell) -> Result<View, Error> {
        let state = match State::decode(&cell.state.data) {
            Ok(state) => state,
            Err(error) => return self.fail(Error::Host(error.to_string())),
        };
        // Absence is logical revision zero; initialized owner revision zero is one.
        let revision = if cell.initialized {
            match cell.revision.checked_add(1) {
                Some(revision) => revision,
                None => return self.fail(Error::Host("health revision exhausted".into())),
            }
        } else {
            0
        };
        Ok(View::new(state, revision))
    }
    fn health_begin(
        &mut self,
        profile: u128,
        session: u64,
        revision: u64,
        native: bool,
    ) -> Result<(Player, ProfileCell, View), Error> {
        self.charge()?;
        if self.health_hook_active {
            return self.fail(Error::Invalid(
                "recursive health mutation in transition hook".into(),
            ));
        }
        if !native {
            self.player_target(profile, session)?;
        }
        let player = self.health_player(profile, session)?;
        let cell = self.health_cell(profile)?;
        let health = self.health_view(&cell)?;
        if health.revision != revision {
            return self.fail(Error::Invalid("stale player health revision".into()));
        }
        if self.plan.player_operations.len() >= 64 {
            return self.fail(Error::Invalid("player operation limit exceeded".into()));
        }
        Ok((player, cell, health))
    }
    fn health_stage(
        &mut self,
        mut player: Player,
        mut cell: ProfileCell,
        state: State,
        before: View,
        transition: Option<(EventKind, Option<String>)>,
        position: Option<[f32; 3]>,
    ) -> Result<(), Error> {
        let key = (player_health::PROFILE_SYSTEM.into(), player.profile);
        if !self.plan.profile_states.contains_key(&key) && self.plan.profile_states.len() >= 64 {
            return self.fail(Error::Invalid("health profile write limit exceeded".into()));
        }
        let committed_revision = match before.revision.checked_add(1) {
            Some(revision) => revision,
            None => return self.fail(Error::Invalid("health revision exhausted".into())),
        };
        cell.state.data = match state.encode() {
            Ok(bytes) => bytes,
            Err(error) => return self.fail(Error::Invalid(error.to_string())),
        };
        cell.state.public_data.clear();
        cell.next_tick = None;
        self.plan.profile_states.insert(key, cell);
        let health = View::new(state, before.revision);
        self.plan.player_operations.push(PlayerOperation {
            profile: player.profile,
            session: player.session,
            kind: PlayerOperationKind::HealthChanged {
                health: View {
                    revision: committed_revision,
                    ..health
                },
                respawn_position: position,
            },
        });
        if let Some((kind, cause)) = transition {
            if let Some(position) = position {
                player.position = position;
            }
            let event = Event {
                kind,
                player,
                before,
                health,
                cause,
            };
            for registration in self.snapshot.health_hooks() {
                self.charge()?;
                let namespace = self.handler_namespace.take();
                let key = self.handler_key.take();
                self.handler_namespace = registration.key.split_once(':').map(|v| v.0.to_owned());
                self.handler_key = Some(registration.key.clone());
                self.health_hook_active = true;
                let result = registration.hook.handle(self, &event);
                self.health_hook_active = false;
                self.handler_namespace = namespace;
                self.handler_key = key;
                if let Err(error) = result {
                    return self.fail(error);
                }
            }
        }
        Ok(())
    }
    pub fn damage_player(
        &mut self,
        profile: u128,
        session: u64,
        revision: u64,
        amount: u32,
        cause: &str,
    ) -> Result<(), Error> {
        let (player, cell, before) = self.health_begin(profile, session, revision, false)?;
        if amount == 0
            || amount > player_health::MAX_HEALTH
            || !crate::regions::valid_key(cause)
            || cause.split_once(':').map(|v| v.0) != self.handler_namespace.as_deref()
        {
            return self.fail(Error::Invalid("invalid or foreign damage cause".into()));
        }
        if !before.alive {
            return Ok(());
        }
        let mut damage = Damage {
            target: player.clone(),
            health: before,
            amount,
            cause: cause.into(),
        };
        for registration in self.snapshot.damage_policies() {
            self.charge()?;
            let amount = match registration.policy.decide(&damage) {
                Ok(value) if value <= player_health::MAX_HEALTH => value,
                Ok(_) => {
                    return self.fail(Error::Invalid("damage policy amount out of bounds".into()));
                }
                Err(e) => return self.fail(Error::Invalid(e)),
            };
            damage.amount = amount;
            if amount == 0 {
                return Ok(());
            }
        }
        let mut state = State::decode(&cell.state.data).map_err(|e| Error::Host(e.to_string()))?;
        state.current = state.current.saturating_sub(damage.amount);
        let transition = if !state.alive() {
            let Some(life) = state.life.checked_add(1) else {
                return self.fail(Error::Invalid("health life exhausted".into()));
            };
            state.life = life;
            Some((EventKind::Died, Some(cause.into())))
        } else {
            None
        };
        self.health_stage(player, cell, state, before, transition, None)
    }
    pub fn heal_player(
        &mut self,
        profile: u128,
        session: u64,
        revision: u64,
        amount: u32,
    ) -> Result<(), Error> {
        let (player, cell, before) = self.health_begin(profile, session, revision, false)?;
        if amount == 0 || amount > player_health::MAX_HEALTH || !before.alive {
            return self.fail(Error::Invalid("invalid heal or target is dead".into()));
        }
        let mut state = State::decode(&cell.state.data).map_err(|e| Error::Host(e.to_string()))?;
        state.current = state.current.saturating_add(amount).min(state.max);
        if state.current == before.current {
            return Ok(());
        }
        self.health_stage(player, cell, state, before, None, None)
    }
    pub fn set_player_max_health(
        &mut self,
        profile: u128,
        session: u64,
        revision: u64,
        max: u32,
    ) -> Result<(), Error> {
        let (player, cell, before) = self.health_begin(profile, session, revision, false)?;
        if max == 0 || max > player_health::MAX_HEALTH {
            return self.fail(Error::Invalid("max health out of bounds".into()));
        }
        if max == before.max {
            return Ok(());
        }
        let state = State {
            current: before.current.min(max),
            max,
            life: before.life,
            ..State::decode(&cell.state.data).map_err(|e| Error::Host(e.to_string()))?
        };
        self.health_stage(player, cell, state, before, None, None)
    }
    pub fn respawn_player(
        &mut self,
        profile: u128,
        session: u64,
        revision: u64,
        position: [f32; 3],
    ) -> Result<(), Error> {
        self.health_respawn(profile, session, revision, position, false)
    }
    /// Native request path only. The host supplies a captured, collision-validated
    /// spawn; scripting bindings never expose this authority bypass.
    pub fn native_respawn_player(
        &mut self,
        profile: u128,
        session: u64,
        revision: u64,
        position: [f32; 3],
    ) -> Result<(), Error> {
        self.health_respawn(profile, session, revision, position, true)
    }
    fn health_respawn(
        &mut self,
        profile: u128,
        session: u64,
        revision: u64,
        position: [f32; 3],
        native: bool,
    ) -> Result<(), Error> {
        let (player, cell, before) = self.health_begin(profile, session, revision, native)?;
        if before.alive
            || position
                .iter()
                .any(|v| !v.is_finite() || v.abs() >= 1_000_000.)
        {
            return self.fail(Error::Invalid("invalid respawn target or position".into()));
        }
        let Some(life) = before.life.checked_add(1) else {
            return self.fail(Error::Invalid("health life exhausted".into()));
        };
        let state = State {
            current: before.max,
            max: before.max,
            life,
            respawn: Some((life, position)),
        };
        self.health_stage(
            player,
            cell,
            state,
            before,
            Some((EventKind::Respawned, None)),
            Some(position),
        )
    }
}
