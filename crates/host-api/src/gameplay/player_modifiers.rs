//! Validated package-owned effects share the existing profile transaction overlay.
use super::{Context, Error, PlayerOperation, PlayerOperationKind};
use crate::{
    player_modifiers::{self, Effect, Movement, Set},
    players::State,
};

impl Context<'_> {
    fn modifier_owner(&mut self, key: &str) -> Result<String, Error> {
        let namespace = self
            .handler_namespace
            .clone()
            .ok_or_else(|| Error::Invalid("modifiers require a package handler".into()))?;
        if !player_modifiers::valid_key(key)
            || key.split_once(':').map(|v| v.0) != Some(namespace.as_str())
        {
            return self.fail(Error::Invalid(
                "modifier effect belongs to another package".into(),
            ));
        }
        if !self.snapshot.player_authority(&namespace) {
            return self.fail(Error::Invalid("player modifier authority denied".into()));
        }
        Ok(namespace)
    }

    /// Read active effects owned by this package. None selects durable profile
    /// lifetime; Some(session) selects exactly that live session's temporary set.
    pub fn player_modifiers(
        &mut self,
        profile: u128,
        session: Option<u64>,
    ) -> Result<Vec<Effect>, Error> {
        self.charge()?;
        let namespace = self
            .handler_namespace
            .clone()
            .ok_or_else(|| Error::Invalid("modifiers require a package handler".into()))?;
        if !self.snapshot.player_authority(&namespace) {
            return self.fail(Error::Invalid("player modifier authority denied".into()));
        }
        if let Some(session) = session {
            self.player_target(profile, session)?;
        }
        let capture = match self
            .snapshot
            .player_modifier_state(&namespace, profile, session)
        {
            Ok(value) => value,
            Err(error) => return self.fail(error),
        };
        let mut effects = if session.is_some() {
            capture.session
        } else {
            let cell = self
                .plan
                .profile_states
                .get(&(player_modifiers::PROFILE_SYSTEM.into(), profile))
                .unwrap_or(&capture.profile);
            Set::decode(&cell.state.data).map_err(|e| Error::Host(e.to_string()))?
        };
        if let Some(session) = session {
            for operation in &self.plan.player_operations {
                if operation.profile != profile || operation.session != session {
                    continue;
                }
                if let PlayerOperationKind::SessionModifier { key, value } = &operation.kind {
                    if let Some(value) = value {
                        effects
                            .set(value.clone(), self.tick())
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                    } else {
                        effects.remove(key);
                    }
                }
            }
        }
        Ok(effects
            .iter()
            .filter(|effect| {
                effect.active(self.tick())
                    && effect.key.split_once(':').map(|v| v.0) == Some(namespace.as_str())
            })
            .cloned()
            .collect())
    }

    pub fn set_player_modifier(
        &mut self,
        profile: u128,
        session: Option<u64>,
        key: &str,
        movement: Movement,
        duration_ticks: Option<u32>,
    ) -> Result<(), Error> {
        let expires_at = match duration_ticks {
            None => None,
            Some(duration) if (1..=player_modifiers::MAX_DURATION_TICKS).contains(&duration) => {
                Some(
                    self.tick()
                        .checked_add(u64::from(duration))
                        .ok_or_else(|| Error::Invalid("modifier expiry exhausted".into()))?,
                )
            }
            _ => return self.fail(Error::Invalid("modifier duration out of bounds".into())),
        };
        self.stage_modifier(
            profile,
            session,
            key,
            Some(Effect {
                key: key.into(),
                movement,
                expires_at,
            }),
        )
    }
    pub fn remove_player_modifier(
        &mut self,
        profile: u128,
        session: Option<u64>,
        key: &str,
    ) -> Result<(), Error> {
        self.stage_modifier(profile, session, key, None)
    }
    fn stage_modifier(
        &mut self,
        profile: u128,
        session: Option<u64>,
        key: &str,
        value: Option<Effect>,
    ) -> Result<(), Error> {
        self.charge()?;
        let namespace = self.modifier_owner(key)?;
        if let Some(value) = &value
            && let Err(error) = value.validate()
        {
            return self.fail(Error::Invalid(error.to_string()));
        }
        if let Some(session) = session {
            self.player_target(profile, session)?;
        }
        let capture = match self
            .snapshot
            .player_modifier_state(&namespace, profile, session)
        {
            Ok(value) => value,
            Err(error) => return self.fail(error),
        };
        if let Some(session) = session {
            let mut effects = capture.session;
            for operation in &self.plan.player_operations {
                if operation.profile != profile || operation.session != session {
                    continue;
                }
                if let PlayerOperationKind::SessionModifier { key, value } = &operation.kind {
                    if let Some(value) = value {
                        effects
                            .set(value.clone(), self.tick())
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                    } else {
                        effects.remove(key);
                    }
                }
            }
            if let Some(value) = &value
                && let Err(error) = effects.set(value.clone(), self.tick())
            {
                return self.fail(Error::Invalid(error.to_string()));
            }
            // Reserve and advance the same profile owner revision for session
            // writes, without saving session effects. This serializes competing
            // writers and prevents an old cap/query capture crossing publication.
            let target = (player_modifiers::PROFILE_SYSTEM.into(), profile);
            if !self.plan.profile_states.contains_key(&target)
                && self.plan.profile_states.len() >= 64
            {
                return self.fail(Error::Invalid(
                    "modifier profile write limit exceeded".into(),
                ));
            }
            self.plan
                .profile_states
                .entry(target)
                .or_insert(capture.profile);
            self.plan.player_operations.push(PlayerOperation {
                profile,
                session,
                kind: PlayerOperationKind::SessionModifier {
                    key: key.into(),
                    value,
                },
            });
        } else {
            let target = (player_modifiers::PROFILE_SYSTEM.into(), profile);
            let mut cell = self
                .plan
                .profile_states
                .get(&target)
                .cloned()
                .unwrap_or(capture.profile);
            let mut effects =
                Set::decode(&cell.state.data).map_err(|e| Error::Host(e.to_string()))?;
            if let Some(value) = value {
                if let Err(error) = effects.set(value, self.tick()) {
                    return self.fail(Error::Invalid(error.to_string()));
                }
            } else {
                effects.retain_active(self.tick());
                effects.remove(key);
            }
            if !self.plan.profile_states.contains_key(&target)
                && self.plan.profile_states.len() >= 64
            {
                return self.fail(Error::Invalid(
                    "modifier profile write limit exceeded".into(),
                ));
            }
            cell.state = State {
                data: effects.encode().map_err(|e| Error::Host(e.to_string()))?,
                public_data: vec![],
            };
            cell.next_tick = effects.iter().filter_map(|effect| effect.expires_at).min();
            self.plan.profile_states.insert(target, cell);
        }
        Ok(())
    }
}
