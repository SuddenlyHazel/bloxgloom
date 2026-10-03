//! Canonical, bounded moving-entity records. Pending reactions are durable and
//! must be consumed in the same transaction as their gameplay effects.
use crate::RegistrationError;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    /// Unit quaternion [x, y, z, w]; physical for opt-in rigid bodies.
    pub orientation: [f32; 4],
    pub angular_velocity: [f32; 3],
    pub revision: u64,
    pub grounded: bool,
}
impl Motion {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if self
            .position
            .iter()
            .any(|x| !x.is_finite() || x.abs() > 16_000_000.0)
            || self.velocity.iter().any(|x| !x.is_finite())
            || self.acceleration.iter().any(|x| !x.is_finite())
            || self.angular_velocity.iter().any(|x| !x.is_finite())
            || length(self.angular_velocity) > super::MAX_ANGULAR_SPEED + 0.001
            || length(self.velocity) > super::MAX_SPEED + 0.001
            || length(self.acceleration) > super::MAX_ACCELERATION + 0.001
            || self.orientation.iter().any(|x| !x.is_finite())
            || (self.orientation.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() > 0.001
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn length(value: [f32; 3]) -> f32 {
    value.into_iter().map(|x| x * x).sum::<f32>().sqrt()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Terrain { cell: [i32; 3], state: String },
    Entity { id: u64, revision: u64 },
}
/// Owned captured contact at the current motion revision. This is host input,
/// not a command or a client reconstruction.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionContact {
    pub motion_revision: u64,
    pub tick: u64,
    pub target: Target,
    pub normal: [f32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpiryReason {
    Lifetime,
    WorldBoundary,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Impact {
    pub entity: u64,
    pub motion_revision: u64,
    pub tick: u64,
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub incoming_velocity: [f32; 3],
    pub target: Target,
    /// Initial overlap or a exhausted contact iteration budget stopped motion.
    pub blocked: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Impact(Impact),
    Expiry {
        entity: u64,
        motion_revision: u64,
        tick: u64,
        reason: ExpiryReason,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub simulation_tick: u64,
    pub motion: Motion,
    pub remaining_ticks: u32,
    pub next_behavior_tick: Option<u64>,
    pub source: Option<u64>,
    pub source_ticks: u32,
    pub contact: Option<Target>,
    pub contact_normal: Option<[f32; 3]>,
    pub pending: Option<Pending>,
    pub state: Vec<u8>,
}
impl Record {
    pub fn contact_state(&self) -> Option<MotionContact> {
        Some(MotionContact {
            motion_revision: self.motion.revision,
            tick: self.simulation_tick,
            target: self.contact.clone()?,
            normal: self.contact_normal?,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, RegistrationError> {
        self.validate()?;
        let mut out = vec![2];
        put_motion(&mut out, self.motion);
        out.extend(self.simulation_tick.to_le_bytes());
        out.extend(self.remaining_ticks.to_le_bytes());
        put_option_u64(&mut out, self.next_behavior_tick);
        put_option_u64(&mut out, self.source);
        out.extend(self.source_ticks.to_le_bytes());
        put_optional_target(&mut out, self.contact.as_ref());
        if let Some(normal) = self.contact_normal {
            for x in normal {
                out.extend(x.to_le_bytes());
            }
        }
        match &self.pending {
            None => out.push(0),
            Some(Pending::Impact(value)) => {
                out.push(1);
                for x in [value.entity, value.motion_revision, value.tick] {
                    out.extend(x.to_le_bytes());
                }
                for x in value
                    .position
                    .into_iter()
                    .chain(value.normal)
                    .chain(value.incoming_velocity)
                {
                    out.extend(x.to_le_bytes());
                }
                put_target(&mut out, &value.target);
                out.push(value.blocked as u8);
            }
            Some(Pending::Expiry {
                entity,
                motion_revision,
                tick,
                reason,
            }) => {
                out.push(2);
                for x in [entity, motion_revision, tick] {
                    out.extend(x.to_le_bytes());
                }
                out.push(*reason as u8);
            }
        }
        out.extend((self.state.len() as u16).to_le_bytes());
        out.extend(&self.state);
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, RegistrationError> {
        let mut c = Cursor(bytes);
        if c.byte()? != 2 {
            return Err(invalid());
        }
        let motion = c.motion()?;
        let simulation_tick = c.u64()?;
        let remaining_ticks = c.u32()?;
        let next_behavior_tick = c.option_u64()?;
        let source = c.option_u64()?;
        let source_ticks = c.u32()?;
        let contact = match c.byte()? {
            0 => None,
            1 => Some(c.target()?),
            _ => return Err(invalid()),
        };
        let contact_normal = if contact.is_some() {
            Some(c.vector()?)
        } else {
            None
        };
        let pending = match c.byte()? {
            0 => None,
            1 => Some(Pending::Impact(Impact {
                entity: c.u64()?,
                motion_revision: c.u64()?,
                tick: c.u64()?,
                position: c.vector()?,
                normal: c.vector()?,
                incoming_velocity: c.vector()?,
                target: c.target()?,
                blocked: c.boolean()?,
            })),
            2 => Some(Pending::Expiry {
                entity: c.u64()?,
                motion_revision: c.u64()?,
                tick: c.u64()?,
                reason: match c.byte()? {
                    0 => ExpiryReason::Lifetime,
                    1 => ExpiryReason::WorldBoundary,
                    _ => return Err(invalid()),
                },
            }),
            _ => return Err(invalid()),
        };
        let length = usize::from(c.u16()?);
        let state = c.take(length)?.to_vec();
        if !c.0.is_empty() {
            return Err(invalid());
        }
        let out = Self {
            simulation_tick,
            motion,
            remaining_ticks,
            next_behavior_tick,
            source,
            source_ticks,
            contact,
            contact_normal,
            pending,
            state,
        };
        out.validate()?;
        Ok(out)
    }
    pub fn validate(&self) -> Result<(), RegistrationError> {
        self.motion.validate()?;
        if self.state.len() > u16::MAX as usize
            || self.remaining_ticks > super::MAX_LIFETIME_TICKS
            || self.source_ticks > super::MAX_SOURCE_EXCLUSION_TICKS
            || self.source.is_none() && self.source_ticks != 0
            || self.source == Some(0)
        {
            return Err(invalid());
        }
        if self.contact.is_some() != self.contact_normal.is_some()
            || self.contact_normal.is_some_and(|n| {
                n.iter().any(|x| !x.is_finite()) || (length(n) - 1.0).abs() > 0.001
            })
        {
            return Err(invalid());
        }
        if let Some(target) = &self.contact {
            validate_target(target)?;
        }
        match &self.pending {
            Some(Pending::Impact(value)) => {
                validate_target(&value.target)?;
                if value.entity == 0
                    || value.motion_revision != self.motion.revision
                    || value
                        .position
                        .into_iter()
                        .chain(value.normal)
                        .chain(value.incoming_velocity)
                        .any(|x| !x.is_finite())
                    || value.position.iter().any(|x| x.abs() > 16_000_000.0)
                    || (length(value.normal) - 1.0).abs() > 0.001
                    || length(value.incoming_velocity) > super::MAX_SPEED + 0.001
                {
                    return Err(invalid());
                }
            }
            Some(Pending::Expiry {
                entity,
                motion_revision,
                ..
            }) if *entity == 0 || *motion_revision != self.motion.revision => {
                return Err(invalid());
            }
            Some(Pending::Expiry { .. }) | None => {}
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Projection {
    pub motion: Motion,
    pub tick: u64,
    pub stopped: bool,
    pub data: Vec<u8>,
}
impl Projection {
    pub fn encode(&self) -> Result<Vec<u8>, RegistrationError> {
        self.motion.validate()?;
        if self.data.len() > 4096 {
            return Err(invalid());
        }
        let mut out = vec![2];
        put_motion(&mut out, self.motion);
        out.extend(self.tick.to_le_bytes());
        out.push(self.stopped as u8);
        out.extend((self.data.len() as u16).to_le_bytes());
        out.extend(&self.data);
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, RegistrationError> {
        let mut c = Cursor(bytes);
        if c.byte()? != 2 {
            return Err(invalid());
        }
        let motion = c.motion()?;
        motion.validate()?;
        let tick = c.u64()?;
        let stopped = c.boolean()?;
        let length = usize::from(c.u16()?);
        if length > 4096 {
            return Err(invalid());
        }
        let data = c.take(length)?.to_vec();
        if !c.0.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            motion,
            tick,
            stopped,
            data,
        })
    }
}
fn invalid() -> RegistrationError {
    RegistrationError("invalid moving entity record".into())
}
fn validate_target(value: &Target) -> Result<(), RegistrationError> {
    match value {
        Target::Terrain { state, .. }
            if state.is_empty() || state.len() > 128 || !state.is_ascii() =>
        {
            Err(invalid())
        }
        Target::Entity { id: 0, .. } => Err(invalid()),
        _ => Ok(()),
    }
}
fn put_motion(out: &mut Vec<u8>, value: Motion) {
    for x in value
        .position
        .into_iter()
        .chain(value.velocity)
        .chain(value.acceleration)
        .chain(value.orientation)
        .chain(value.angular_velocity)
    {
        out.extend(x.to_le_bytes());
    }
    out.extend(value.revision.to_le_bytes());
    out.push(value.grounded as u8);
}
fn put_option_u64(out: &mut Vec<u8>, value: Option<u64>) {
    out.push(value.is_some() as u8);
    if let Some(x) = value {
        out.extend(x.to_le_bytes());
    }
}
fn put_optional_target(out: &mut Vec<u8>, value: Option<&Target>) {
    out.push(value.is_some() as u8);
    if let Some(x) = value {
        put_target(out, x);
    }
}
fn put_target(out: &mut Vec<u8>, value: &Target) {
    match value {
        Target::Terrain { cell, state } => {
            out.push(0);
            for x in cell {
                out.extend(x.to_le_bytes());
            }
            out.push(state.len() as u8);
            out.extend(state.as_bytes());
        }
        Target::Entity { id, revision } => {
            out.push(1);
            out.extend(id.to_le_bytes());
            out.extend(revision.to_le_bytes());
        }
    }
}
struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], RegistrationError> {
        if self.0.len() < n {
            return Err(invalid());
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn byte(&mut self) -> Result<u8, RegistrationError> {
        Ok(self.take(1)?[0])
    }
    fn boolean(&mut self) -> Result<bool, RegistrationError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid()),
        }
    }
    fn u16(&mut self) -> Result<u16, RegistrationError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn u32(&mut self) -> Result<u32, RegistrationError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn u64(&mut self) -> Result<u64, RegistrationError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn vector(&mut self) -> Result<[f32; 3], RegistrationError> {
        let mut out = [0.0; 3];
        for x in &mut out {
            *x = f32::from_le_bytes(self.take(4)?.try_into().map_err(|_| invalid())?);
        }
        Ok(out)
    }
    fn quaternion(&mut self) -> Result<[f32; 4], RegistrationError> {
        let mut out = [0.0; 4];
        for x in &mut out {
            *x = f32::from_le_bytes(self.take(4)?.try_into().map_err(|_| invalid())?);
        }
        Ok(out)
    }
    fn option_u64(&mut self) -> Result<Option<u64>, RegistrationError> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.u64()?)),
            _ => Err(invalid()),
        }
    }
    fn motion(&mut self) -> Result<Motion, RegistrationError> {
        Ok(Motion {
            position: self.vector()?,
            velocity: self.vector()?,
            acceleration: self.vector()?,
            orientation: self.quaternion()?,
            angular_velocity: self.vector()?,
            revision: self.u64()?,
            grounded: self.boolean()?,
        })
    }
    fn target(&mut self) -> Result<Target, RegistrationError> {
        match self.byte()? {
            0 => {
                let mut cell = [0; 3];
                for x in &mut cell {
                    *x = i32::from_le_bytes(self.take(4)?.try_into().map_err(|_| invalid())?);
                }
                let n = usize::from(self.byte()?);
                let state = std::str::from_utf8(self.take(n)?)
                    .map_err(|_| invalid())?
                    .to_owned();
                Ok(Target::Terrain { cell, state })
            }
            1 => Ok(Target::Entity {
                id: self.u64()?,
                revision: self.u64()?,
            }),
            _ => Err(invalid()),
        }
    }
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
