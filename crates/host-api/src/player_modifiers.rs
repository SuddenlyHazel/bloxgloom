//! Package-owned runtime player effects, separate from the frozen base rules.
//! Expiry uses world logical ticks: server downtime pauses it, disconnection
//! from a running server does not. Session effects never enter saved state.
use crate::{
    RegistrationError,
    player::{MotionRates, PlayerRules},
};
use std::collections::BTreeMap;

pub const MAX_EFFECTS: usize = 32;
pub const MAX_KEY_BYTES: usize = 96;
pub const MAX_DURATION_TICKS: u32 = 1_000_000;
pub const MAX_STATE_BYTES: usize = 4096;
pub const PROFILE_SYSTEM: &str = "bloxgloom:player_modifiers";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Movement {
    pub speed: f32,
    pub sprint: f32,
    pub jump: f32,
    pub gravity: f32,
}
impl Default for Movement {
    fn default() -> Self {
        Self {
            speed: 1.0,
            sprint: 1.0,
            jump: 1.0,
            gravity: 1.0,
        }
    }
}
impl Movement {
    pub fn validate(self) -> Result<(), RegistrationError> {
        if [self.speed, self.sprint, self.jump, self.gravity]
            .iter()
            .any(|v| !v.is_finite() || !(0.1..=4.0).contains(v))
        {
            return Err(invalid("movement multipliers must be finite in 0.1..=4"));
        }
        Ok(())
    }

    /// Shared authority/prediction rates; derived movement retains the existing
    /// <=16 blocks/s capture, collision and accounting bounds.
    pub fn rules(self, base: PlayerRules, crouching: bool, sprinting: bool) -> PlayerRules {
        let rules = base.for_movement(crouching, sprinting);
        let multiplier = self.speed
            * if sprinting && !crouching {
                self.sprint
            } else {
                1.0
            };
        if multiplier == 1.0 && rules.validate().is_ok() {
            return rules;
        }
        let budget =
            (rules.motion().budget_blocks_per_second * f64::from(multiplier)).clamp(0.1, 16.0);
        let intent =
            (rules.motion().intent_blocks_per_second * multiplier).clamp(0.1, budget as f32);
        // f32 rounding may move intent just above the f64 budget, including at
        // the minimum rate. Widen authority by only that representational gap.
        let budget = budget.max(f64::from(intent));
        PlayerRules::new(
            rules.body(),
            MotionRates {
                intent_blocks_per_second: intent,
                budget_blocks_per_second: budget,
            },
            rules.spawn(),
            rules.eye_height(),
        )
        .expect("derived validated modifier bounds")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub key: String,
    pub movement: Movement,
    pub expires_at: Option<u64>,
}
impl Effect {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if !valid_key(&self.key) || self.expires_at == Some(0) {
            return Err(invalid("invalid modifier identity or expiry"));
        }
        self.movement.validate()
    }
    pub fn active(&self, tick: u64) -> bool {
        self.expires_at.is_none_or(|until| tick < until)
    }
}
pub fn valid_key(key: &str) -> bool {
    key.len() <= MAX_KEY_BYTES
        && key.split_once(':').is_some_and(|(namespace, local)| {
            !namespace.is_empty()
                && !local.is_empty()
                && namespace
                    .bytes()
                    .chain(local.bytes())
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Set(BTreeMap<String, Effect>);
impl Set {
    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.0.values()
    }
    pub fn get(&self, key: &str) -> Option<&Effect> {
        self.0.get(key)
    }
    pub fn retain_active(&mut self, tick: u64) {
        self.0.retain(|_, effect| effect.active(tick));
    }
    pub fn set(&mut self, effect: Effect, tick: u64) -> Result<(), RegistrationError> {
        effect.validate()?;
        self.retain_active(tick);
        if !self.0.contains_key(&effect.key) && self.0.len() >= MAX_EFFECTS {
            return Err(invalid("player modifier effect cap exceeded (32)"));
        }
        self.0.insert(effect.key.clone(), effect);
        Ok(())
    }
    pub fn remove(&mut self, key: &str) -> bool {
        self.0.remove(key).is_some()
    }
    pub fn encode(&self) -> Result<Vec<u8>, RegistrationError> {
        if self.0.len() > MAX_EFFECTS {
            return Err(invalid("too many player modifiers"));
        }
        let mut out = vec![1, self.0.len() as u8];
        for effect in self.iter() {
            effect.validate()?;
            out.push(effect.key.len() as u8);
            out.extend(effect.key.as_bytes());
            out.push(effect.expires_at.is_some() as u8);
            if let Some(tick) = effect.expires_at {
                out.extend(tick.to_le_bytes());
            }
            for value in [
                effect.movement.speed,
                effect.movement.sprint,
                effect.movement.jump,
                effect.movement.gravity,
            ] {
                out.extend(value.to_le_bytes());
            }
        }
        if out.len() > MAX_STATE_BYTES {
            return Err(invalid("modifier state byte cap exceeded"));
        }
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, RegistrationError> {
        if bytes.len() > MAX_STATE_BYTES || bytes.first() != Some(&1) {
            return Err(invalid("invalid modifier state version"));
        }
        let mut bytes = &bytes[1..];
        let count = take(&mut bytes, 1)?[0] as usize;
        if count > MAX_EFFECTS {
            return Err(invalid("too many player modifiers"));
        }
        let mut effects = BTreeMap::new();
        let mut previous = String::new();
        for _ in 0..count {
            let len = take(&mut bytes, 1)?[0] as usize;
            let key = std::str::from_utf8(take(&mut bytes, len)?)
                .map_err(|_| invalid("invalid modifier key"))?
                .to_owned();
            if key <= previous {
                return Err(invalid("modifier keys must be canonical and unique"));
            }
            previous.clone_from(&key);
            let expires_at = match take(&mut bytes, 1)?[0] {
                0 => None,
                1 => Some(u64::from_le_bytes(take(&mut bytes, 8)?.try_into().unwrap())),
                _ => return Err(invalid("invalid modifier expiry tag")),
            };
            let mut values = [0.0; 4];
            for value in &mut values {
                *value = f32::from_le_bytes(take(&mut bytes, 4)?.try_into().unwrap());
            }
            let effect = Effect {
                key: key.clone(),
                movement: Movement {
                    speed: values[0],
                    sprint: values[1],
                    jump: values[2],
                    gravity: values[3],
                },
                expires_at,
            };
            effect.validate()?;
            effects.insert(key, effect);
        }
        if !bytes.is_empty() {
            return Err(invalid("trailing modifier bytes"));
        }
        Ok(Self(effects))
    }
}

/// Effects are aggregated lexically across both lifetimes before clamping. f64
/// multiplication prevents per-effect f32 rounding/order from changing results.
pub fn aggregate(profile: &Set, session: &Set, tick: u64) -> Movement {
    let mut effects: Vec<_> = profile
        .iter()
        .chain(session.iter())
        .filter(|e| e.active(tick))
        .collect();
    effects.sort_by(|a, b| a.key.cmp(&b.key));
    let mut values = [1.0_f64; 4];
    for effect in effects {
        for (total, value) in values.iter_mut().zip([
            effect.movement.speed,
            effect.movement.sprint,
            effect.movement.jump,
            effect.movement.gravity,
        ]) {
            *total *= f64::from(value);
        }
    }
    Movement {
        speed: values[0].clamp(0.1, 4.0) as f32,
        sprint: values[1].clamp(0.1, 4.0) as f32,
        jump: values[2].clamp(0.1, 2.0) as f32,
        gravity: values[3].clamp(0.1, 2.0) as f32,
    }
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub profile: crate::gameplay::ProfileCell,
    pub session: Set,
}

fn take<'a>(bytes: &mut &'a [u8], len: usize) -> Result<&'a [u8], RegistrationError> {
    if bytes.len() < len {
        return Err(invalid("truncated modifier state"));
    }
    let (head, tail) = bytes.split_at(len);
    *bytes = tail;
    Ok(head)
}
fn invalid(message: &str) -> RegistrationError {
    RegistrationError(message.into())
}

#[cfg(test)]
#[path = "player_modifiers/tests.rs"]
mod tests;
