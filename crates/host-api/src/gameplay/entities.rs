use super::{Cell, Context, Error};

#[derive(Clone, Debug, PartialEq)]
pub struct EntitySpawn {
    pub key: String,
    pub position: [f32; 3],
    pub state: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EntityChange {
    Update { id: u64, state: Vec<u8> },
    Remove { id: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: u64,
    pub entity_type: String,
    pub position: [f32; 3],
    pub anchor: Option<Cell>,
    /// Registered public projection; private payloads stay behind owned services.
    pub data: Vec<u8>,
}

impl Context<'_> {
    /// Schedule or suspend an owned entity. A due callback with no explicit
    /// reschedule suspends by default, so it cannot create a busy loop.
    pub fn schedule_entity(&mut self, id: u64, after_ticks: Option<u32>) -> Result<bool, Error> {
        if self.entity_state(id)?.is_none() {
            return Ok(false);
        }
        if let Err(error) = self.snapshot.validate_entity_schedule(id) {
            return self.fail(error);
        }
        let due = match after_ticks {
            Some(delay) if (1..=100_000).contains(&delay) => {
                let Some(due) = self.snapshot.tick().checked_add(u64::from(delay)) else {
                    return self.fail(Error::Invalid("entity schedule overflow".into()));
                };
                Some(due)
            }
            Some(_) => {
                return self.fail(Error::Invalid(
                    "entity delay outside supported range".into(),
                ));
            }
            None => None,
        };
        self.plan.entity_schedules.insert(id, due);
        Ok(true)
    }
    pub fn nearby_entities(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> Result<Vec<Entity>, Error> {
        self.charge()?;
        if !position.iter().all(|value| value.is_finite())
            || !radius.is_finite()
            || !(0.0..=16.0).contains(&radius)
        {
            return self.fail(Error::Invalid("invalid entity query bounds".into()));
        }
        match self.snapshot.nearby_entities(position, radius) {
            Ok(entities) => {
                let mut projected = Vec::with_capacity(entities.len());
                for entity in entities {
                    match self.project_overlay(entity) {
                        Ok(Some(entity)) => projected.push(entity),
                        Ok(None) => {}
                        Err(error) => return self.fail(error),
                    }
                }
                Ok(projected)
            }
            Err(error) => self.fail(error),
        }
    }
    fn project_overlay(&self, mut entity: Entity) -> Result<Option<Entity>, Error> {
        match self.plan.entity_changes.get(&entity.id) {
            Some(EntityChange::Remove { .. }) => Ok(None),
            Some(EntityChange::Update { state, .. }) => {
                entity.data = self.snapshot.project_entity_state(entity.id, state)?;
                Ok(Some(entity))
            }
            None => Ok(Some(entity)),
        }
    }
    fn owner(&self) -> Result<&str, Error> {
        self.handler_namespace
            .as_deref()
            .ok_or_else(|| Error::Invalid("entity mutation requires a registered handler".into()))
    }

    pub fn entity_state(&mut self, id: u64) -> Result<Option<Vec<u8>>, Error> {
        self.charge()?;
        let owner = self.owner()?.to_owned();
        if let Some(state) = self.entity_overlay.get(&id) {
            return Ok(state.clone());
        }
        match self.snapshot.entity_state(id, &owner) {
            Ok(state) => {
                self.entity_overlay.insert(id, state.clone());
                Ok(state)
            }
            Err(error) => self.fail(error),
        }
    }

    /// Allocated ID is assigned on commit, not predicted by handler code.
    pub fn spawn_entity(
        &mut self,
        key: &str,
        position: [f32; 3],
        state: &[u8],
    ) -> Result<(), Error> {
        self.charge()?;
        if self.plan.entity_spawns.len() >= 32 {
            return self.fail(Error::BudgetExceeded);
        }
        if !position.iter().all(|n| n.is_finite()) {
            return self.fail(Error::Invalid("invalid entity position".into()));
        }
        let owner = self.owner()?.to_owned();
        if let Err(error) = self.snapshot.validate_entity_state(key, &owner, state) {
            return self.fail(error);
        }
        self.plan.entity_spawns.push(EntitySpawn {
            key: key.into(),
            position,
            state: state.into(),
        });
        Ok(())
    }

    pub fn update_entity(&mut self, id: u64, state: &[u8]) -> Result<bool, Error> {
        let Some(before) = self.entity_state(id)? else {
            return Ok(false);
        };
        if before == state {
            return Ok(true);
        }
        let owner = self.owner()?.to_owned();
        let Some(entity) = self.snapshot.entity(id)? else {
            return self.fail(Error::Host("entity disappeared during planning".into()));
        };
        if let Err(error) = self
            .snapshot
            .validate_entity_state(&entity.entity_type, &owner, state)
        {
            return self.fail(error);
        }
        self.entity_overlay.insert(id, Some(state.into()));
        self.plan.entity_changes.insert(
            id,
            EntityChange::Update {
                id,
                state: state.into(),
            },
        );
        Ok(true)
    }

    pub fn remove_entity(&mut self, id: u64) -> Result<bool, Error> {
        if self.entity_state(id)?.is_none() {
            return Ok(false);
        }
        self.entity_overlay.insert(id, None);
        self.plan.entity_schedules.remove(&id);
        self.plan
            .entity_changes
            .insert(id, EntityChange::Remove { id });
        Ok(true)
    }
    pub fn entity(&mut self, id: u64) -> Result<Option<Entity>, Error> {
        self.charge()?;
        match self.snapshot.entity(id) {
            Ok(Some(entity)) => match self.project_overlay(entity) {
                Ok(entity) => Ok(entity),
                Err(error) => self.fail(error),
            },
            Ok(None) => Ok(None),
            Err(error) => self.fail(error),
        }
    }

    /// Looks up anchored occupancy, including secondary footprint cells. The
    /// host captures absence as well as presence for conflict validation.
    pub fn anchored_entity_at(&mut self, cell: Cell) -> Result<Option<u64>, Error> {
        self.charge()?;
        match self.snapshot.anchored_entity_at(cell) {
            Ok(entity) => Ok(entity),
            Err(error) => self.fail(error),
        }
    }
}
