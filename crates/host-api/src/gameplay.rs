//! Shared, retryable gameplay planning. The host owns dependency capture and
//! publication; a handler only reads a snapshot and stages changes.
//!
//! Terrain, item creation and exact inventory operations share one overlay.
//! Entity services are added here as host transaction participants are unified.
use std::collections::BTreeMap;

mod entities;
mod handlers;
mod inventory;
pub use entities::Entity;
pub use handlers::{Event, EventKind, Handler, HandlerRegistration, RemovalCause};
pub use inventory::{Components, InventoryId, Slot, Stack};

pub type Cell = [i32; 3];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub state: String,
    pub block_type: String,
    pub primary_item: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable(Cell),
    UnknownContent(String),
    InventoryUnavailable(InventoryId),
    Invalid(String),
    BudgetExceeded,
    Host(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(cell) => write!(f, "terrain unavailable at {cell:?}"),
            Self::UnknownContent(key) => write!(f, "unknown content: {key}"),
            Self::InventoryUnavailable(owner) => write!(f, "inventory unavailable: {owner:?}"),
            Self::Invalid(reason) | Self::Host(reason) => f.write_str(reason),
            Self::BudgetExceeded => f.write_str("gameplay operation budget exceeded"),
        }
    }
}
impl std::error::Error for Error {}

/// Host implementation must capture dependencies for successful reads, including
/// air. A missing chunk is `Unavailable`, never procedural fallback or air.
pub trait Snapshot {
    fn player(&self) -> Option<u128>;
    fn entity(&mut self, id: u64) -> Result<Option<Entity>, Error>;
    fn anchored_entity_at(&mut self, cell: Cell) -> Result<Option<u64>, Error>;
    fn block(&mut self, cell: Cell) -> Result<Block, Error>;
    fn state(&self, key: &str) -> Result<Block, Error>;
    fn item_exists(&self, key: &str) -> bool;
    fn inventory(&mut self, owner: InventoryId) -> Result<Vec<Slot>, Error>;
    fn validate_stack(&self, stack: &Stack) -> Result<(), Error>;
    fn inventory_accepts(&self, owner: InventoryId, slot: usize, stack: &Stack) -> bool;
}

#[derive(Clone, Debug, PartialEq)]
pub struct DropSpawn {
    pub position: [f32; 3],
    pub stack: Stack,
    pub pickup_delay_ms: u32,
}

/// Staged output, consumed by the host's existing atomic transaction machinery.
/// Constructing a plan does not publish anything or bypass host validation.
#[derive(Debug, Default)]
pub struct Plan {
    pub blocks: BTreeMap<Cell, String>,
    pub drops: Vec<DropSpawn>,
    pub inventories: BTreeMap<InventoryId, Vec<Option<Stack>>>,
}

pub struct Context<'a> {
    snapshot: &'a mut dyn Snapshot,
    blocks: BTreeMap<Cell, Block>,
    inventories: BTreeMap<InventoryId, Vec<Slot>>,
    plan: Plan,
    remaining: usize,
    failure: Option<Error>,
}

impl<'a> Context<'a> {
    pub fn player(&self) -> Option<InventoryId> {
        self.snapshot.player().map(InventoryId::Player)
    }
    pub fn new(snapshot: &'a mut dyn Snapshot, operation_budget: usize) -> Self {
        Self {
            snapshot,
            blocks: BTreeMap::new(),
            inventories: BTreeMap::new(),
            plan: Plan::default(),
            remaining: operation_budget,
            failure: None,
        }
    }

    fn charge(&mut self) -> Result<(), Error> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.remaining == 0 {
            return self.fail(Error::BudgetExceeded);
        }
        self.remaining -= 1;
        Ok(())
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.failure.get_or_insert_with(|| error.clone());
        Err(error)
    }

    pub fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        self.charge()?;
        if let Some(block) = self.blocks.get(&cell) {
            return Ok(block.clone());
        }
        match self.snapshot.block(cell) {
            Ok(block) => {
                self.blocks.insert(cell, block.clone());
                Ok(block)
            }
            Err(error) => self.fail(error),
        }
    }

    /// Records the preimage even for a blind write. Later reads see this state;
    /// repeated writes coalesce without forgetting the original dependency.
    pub fn set_block(&mut self, cell: Cell, state: &str) -> Result<(), Error> {
        self.block(cell)?;
        let block = match self.snapshot.state(state) {
            Ok(block) => block,
            Err(error) => return self.fail(error),
        };
        self.plan.blocks.insert(cell, block.state.clone());
        self.blocks.insert(cell, block);
        Ok(())
    }

    /// Explicit item creation, not an inventory transfer. Each spawn is one
    /// legal stack; authors split larger rewards deliberately.
    pub fn spawn_drop(
        &mut self,
        position: [f32; 3],
        item: &str,
        count: u16,
        pickup_delay_ms: u32,
    ) -> Result<(), Error> {
        self.spawn_stack(position, Stack::new(item, count), pickup_delay_ms)
    }

    pub fn spawn_stack(
        &mut self,
        position: [f32; 3],
        stack: Stack,
        pickup_delay_ms: u32,
    ) -> Result<(), Error> {
        self.charge()?;
        if !position.iter().all(|v| v.is_finite()) || !(1..=128).contains(&stack.count) {
            return self.fail(Error::Invalid(
                "invalid drop position or stack count".into(),
            ));
        }
        if let Err(error) = self.snapshot.validate_stack(&stack) {
            return self.fail(error);
        }
        self.plan.drops.push(DropSpawn {
            position,
            stack,
            pickup_delay_ms,
        });
        Ok(())
    }

    /// Even if a callback ignores a failed operation, no partial output escapes.
    pub fn finish(self) -> Result<Plan, Error> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(self.plan),
        }
    }
}

/// Stable random input for an operation. Retrying with identical inputs produces
/// the same result; authoritative state never depends on a VM's global RNG.
pub fn cell_random(seed: u64, cell: Cell, sequence: u64) -> u64 {
    let [x, y, z] = cell;
    let mut value = seed
        ^ (x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (z as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb)
        ^ sequence;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
