//! Owned motion operations join the same staged gameplay transaction as state,
//! inventories and terrain. Snapshot implementations fence motion revisions.
use super::{Context, Error};
use crate::motion::Motion;

#[derive(Clone, Debug, PartialEq)]
pub struct MovingSpawn {
    pub key: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub orientation: [f32; 4],
    pub state: Vec<u8>,
    pub source: Option<u64>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionChange {
    pub velocity: Option<[f32; 3]>,
    pub acceleration: Option<[f32; 3]>,
    pub orientation: Option<[f32; 4]>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionCommand {
    pub id: u64,
    pub expected_revision: u64,
    pub change: MotionChange,
}
/// An opaque allocation reference, scoped to one Context. It is not a durable
/// entity identity; committed host receipts resolve its zero-based index.
#[derive(Clone, Debug)]
pub struct SpawnReference {
    index: usize,
    scope: std::sync::Arc<()>,
}
impl SpawnReference {
    pub fn index(&self) -> usize {
        self.index
    }
}
impl Context<'_> {
    pub fn owns_spawn_reference(&self, reference: &SpawnReference) -> bool {
        std::sync::Arc::ptr_eq(&self.motion_reference_scope, &reference.scope)
            && reference.index < self.plan.moving_spawns.len()
    }
    pub fn spawn_moving_entity(&mut self, spawn: MovingSpawn) -> Result<SpawnReference, Error> {
        self.charge()?;
        if self.plan.moving_spawns.len() + self.plan.entity_spawns.len() >= 32 {
            return self.fail(Error::BudgetExceeded);
        }
        let owner = self.motion_owner()?.to_owned();
        let motion = Motion {
            position: spawn.position,
            velocity: spawn.velocity,
            acceleration: [0.0; 3],
            orientation: spawn.orientation,
            revision: 0,
            grounded: false,
        };
        if let Err(error) = motion.validate() {
            return self.fail(Error::Invalid(error.0));
        }
        if let Err(error) = self.snapshot.validate_moving_spawn(&owner, &spawn) {
            return self.fail(error);
        }
        let reference = SpawnReference {
            index: self.plan.moving_spawns.len(),
            scope: self.motion_reference_scope.clone(),
        };
        self.plan.moving_spawns.push(spawn);
        Ok(reference)
    }
    fn motion_owner(&self) -> Result<&str, Error> {
        self.handler_namespace
            .as_deref()
            .ok_or_else(|| Error::Invalid("motion requires registered owner".into()))
    }
    pub fn motion(&mut self, id: u64) -> Result<Option<Motion>, Error> {
        self.charge()?;
        let owner = self.motion_owner()?.to_owned();
        let mut motion = match self.snapshot.motion(id, &owner) {
            Ok(value) => value,
            Err(error) => return self.fail(error),
        };
        if matches!(
            self.plan.entity_changes.get(&id),
            Some(super::EntityChange::Remove { .. })
        ) {
            return Ok(None);
        }
        if let (Some(value), Some(command)) = (&mut motion, self.plan.motion_commands.get(&id)) {
            apply(value, command.change);
        }
        Ok(motion)
    }
    pub fn set_motion(
        &mut self,
        id: u64,
        expected_revision: u64,
        change: MotionChange,
    ) -> Result<bool, Error> {
        let Some(mut motion) = self.motion(id)? else {
            return Ok(false);
        };
        if motion.revision != expected_revision {
            return self.fail(Error::Invalid("motion revision mismatch".into()));
        }
        apply(&mut motion, change);
        if let Err(error) = motion.validate() {
            return self.fail(Error::Invalid(error.0));
        }
        let owner = self.motion_owner()?.to_owned();
        if let Err(error) = self.snapshot.validate_motion_change(id, &owner, &motion) {
            return self.fail(error);
        }
        let entry = self
            .plan
            .motion_commands
            .entry(id)
            .or_insert(MotionCommand {
                id,
                expected_revision,
                change: MotionChange::default(),
            });
        if change.velocity.is_some() {
            entry.change.velocity = change.velocity;
        }
        if change.acceleration.is_some() {
            entry.change.acceleration = change.acceleration;
        }
        if change.orientation.is_some() {
            entry.change.orientation = change.orientation;
        }
        Ok(true)
    }
}
fn apply(motion: &mut Motion, change: MotionChange) {
    if let Some(value) = change.velocity {
        motion.velocity = value;
    }
    if let Some(value) = change.acceleration {
        motion.acceleration = value;
    }
    if let Some(value) = change.orientation {
        motion.orientation = value;
    }
}
