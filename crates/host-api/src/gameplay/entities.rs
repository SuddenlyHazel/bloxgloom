use super::{Cell, Context, Error};

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
    pub fn entity(&mut self, id: u64) -> Result<Option<Entity>, Error> {
        self.charge()?;
        match self.snapshot.entity(id) {
            Ok(entity) => Ok(entity),
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
