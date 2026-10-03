//! Authoritative moving objects. Positions and model coordinates use the body
//! center; units are blocks, seconds and logical simulation ticks.
use crate::{RegistrationError, entity::Cuboid, gameplay::EntityState};
use std::sync::Arc;

mod record;
pub use record::*;
mod physics;
pub use physics::*;

pub const MAX_SPEED: f32 = 64.0;
pub const MAX_ACCELERATION: f32 = 128.0;
pub const MAX_LIFETIME_TICKS: u32 = 72_000;
pub const MAX_SOURCE_EXCLUSION_TICKS: u32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    Stop,
    Bounce,
    Slide,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionMask {
    pub terrain: bool,
    pub players: bool,
    pub creatures: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub half_extents: [f32; 3],
    pub collisions: CollisionMask,
    pub response: Response,
    pub restitution: f32,
    pub gravity_scale: f32,
    pub max_speed: f32,
    pub max_acceleration: f32,
}
impl Body {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if self
            .half_extents
            .iter()
            .any(|x| !x.is_finite() || !(0.025..=1.5).contains(x))
            || !self.restitution.is_finite()
            || !(0.0..=1.0).contains(&self.restitution)
            || !self.gravity_scale.is_finite()
            || !(0.0..=4.0).contains(&self.gravity_scale)
            || !self.max_speed.is_finite()
            || !(0.0..=MAX_SPEED).contains(&self.max_speed)
            || !self.max_acceleration.is_finite()
            || !(0.0..=MAX_ACCELERATION).contains(&self.max_acceleration)
        {
            return Err(RegistrationError("invalid moving body bounds".into()));
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct MovingEntity {
    pub key: String,
    pub schema_version: u16,
    pub schema_fingerprint: u64,
    pub max_state_bytes: u16,
    pub max_public_bytes: u16,
    pub body: Body,
    pub physics: Option<Physics>,
    pub lifetime_ticks: u32,
    pub interval: u32,
    pub source_exclusion_ticks: u32,
    pub handles_impact: bool,
    pub handles_expiry: bool,
    pub model: Vec<Cuboid>,
    pub state: Arc<dyn EntityState>,
}
impl std::fmt::Debug for MovingEntity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MovingEntity")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}
impl MovingEntity {
    /// Conservative terrain capture/spawn envelope under arbitrary rotation.
    /// Actual rigid-body contacts use the oriented cuboid in Rapier.
    pub fn capture_half_extents(&self) -> [f32; 3] {
        if self.physics.is_some() {
            let radius = self
                .body
                .half_extents
                .iter()
                .map(|v| v * v)
                .sum::<f32>()
                .sqrt();
            [radius; 3]
        } else {
            self.body.half_extents
        }
    }
    pub fn validate(&self) -> Result<(), RegistrationError> {
        crate::gameplay::EntityDefinition {
            key: self.key.clone(),
            schema_version: self.schema_version,
            schema_fingerprint: self.schema_fingerprint,
            max_state_bytes: self.max_state_bytes,
            initial_delay_ticks: Some(self.interval),
            state: self.state.clone(),
        }
        .validate()?;
        self.body.validate()?;
        if let Some(physics) = self.physics {
            physics.validate()?;
            if self.handles_impact || self.body.response == Response::Stop {
                return Err(RegistrationError(
                    "rigid bodies require slide/bounce and cannot pause for impact callbacks"
                        .into(),
                ));
            }
        }
        if self.max_state_bytes > 65000
            || self.max_public_bytes > 4000
            || !(1..=MAX_LIFETIME_TICKS).contains(&self.lifetime_ticks)
            || self.source_exclusion_ticks > MAX_SOURCE_EXCLUSION_TICKS
            || self.model.len() > 64
            || self.model.iter().any(|p| {
                (0..3).any(|i| {
                    !p.min[i].is_finite()
                        || !p.max[i].is_finite()
                        || p.min[i] >= p.max[i]
                        || p.min[i] < -4.0
                        || p.max[i] > 4.0
                        || !p.color[i].is_finite()
                        || !(0.0..=1.0).contains(&p.color[i])
                })
            })
        {
            return Err(RegistrationError("invalid moving entity bounds".into()));
        }
        Ok(())
    }
    /// Frozen collision/model policy participates in catalog compatibility.
    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![
            2,
            self.body.collisions.terrain as u8,
            self.body.collisions.players as u8,
            self.body.collisions.creatures as u8,
            self.body.response as u8,
            self.handles_impact as u8,
            self.handles_expiry as u8,
        ];
        for value in self.body.half_extents.into_iter().chain([
            self.body.restitution,
            self.body.gravity_scale,
            self.body.max_speed,
            self.body.max_acceleration,
        ]) {
            bytes.extend(value.to_le_bytes());
        }
        bytes.push(self.physics.is_some() as u8);
        if let Some(physics) = self.physics {
            for value in [
                physics.linear_damping,
                physics.angular_damping,
                physics.friction,
                physics.max_angular_speed,
            ] {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.extend(self.max_state_bytes.to_le_bytes());
        bytes.extend(self.max_public_bytes.to_le_bytes());
        for value in [
            self.lifetime_ticks,
            self.interval,
            self.source_exclusion_ticks,
        ] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.push(self.model.len() as u8);
        for part in &self.model {
            for value in part.min.into_iter().chain(part.max).chain(part.color) {
                bytes.extend(value.to_le_bytes());
            }
            bytes.push(part.motion as u8);
        }
        bytes
    }
}

/// Allocated durable identity after the complete launch transaction commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnReceipt {
    pub ordinal: u8,
    pub entity: u64,
}
