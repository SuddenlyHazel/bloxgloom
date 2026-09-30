//! Shared, retryable gameplay planning. The host owns dependency capture and
//! publication; a handler only reads a snapshot and stages changes.
//!
//! Terrain, item creation and exact inventory operations share one overlay.
//! Entity services are added here as host transaction participants are unified.
use std::collections::BTreeMap;

mod clock;
mod definition;
mod entities;
mod handlers;
mod inventory;
mod observations;
pub use clock::WorldTime;
pub use definition::{EntityDefinition, EntityState};
pub use entities::{Entity, EntityChange, EntitySpawn};
pub use handlers::{Event, EventKind, Handler, HandlerRegistration, RemovalCause};
pub use inventory::{Components, InventoryId, PickupTransfer, Slot, Stack};
pub use observations::{
    Committed, CommittedBlock, CommittedEntity, Observer, ObserverRegistration,
};

pub type Cell = [i32; 3];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub state: String,
    pub block_type: String,
    pub primary_item: Option<String>,
    pub plant: bool,
    pub supports_plant: bool,
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
    fn world_time(&mut self) -> Result<WorldTime, Error> {
        Err(Error::Invalid(
            "world clock unavailable in this context".into(),
        ))
    }
    /// Stable authenticated request identity, retained across action retries.
    fn action_id(&self) -> Option<u128> {
        None
    }
    fn tick(&self) -> u64;
    fn seed(&self) -> u64;
    fn player(&self) -> Option<u128>;
    /// Captured authoritative feet position for this authenticated actor.
    /// Never derive this from an action's client-supplied target coordinates.
    fn player_position(&self) -> Option<[f32; 3]> {
        None
    }
    /// Whether this drop is eligible for an automatic pickup by the captured
    /// actor now. A wider entity-inventory interaction radius is not enough.
    fn pickup_eligible(&self, _drop_id: u64) -> bool {
        false
    }
    /// Server-authenticated admin identity, never derived from request bytes.
    fn admin(&self) -> bool {
        false
    }
    fn entity(&mut self, id: u64) -> Result<Option<Entity>, Error>;
    fn nearby_entities(&mut self, position: [f32; 3], radius: f32) -> Result<Vec<Entity>, Error>;
    fn entity_state(&mut self, id: u64, owner: &str) -> Result<Option<Vec<u8>>, Error>;
    fn project_entity_state(&self, id: u64, state: &[u8]) -> Result<Vec<u8>, Error>;
    fn validate_entity_state(&self, key: &str, owner: &str, state: &[u8]) -> Result<(), Error>;
    fn validate_entity_schedule(&self, id: u64) -> Result<(), Error> {
        Err(Error::Invalid(format!(
            "entity {id} has no scheduled handler"
        )))
    }
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
    pub world_time: Option<u64>,
    pub blocks: BTreeMap<Cell, String>,
    pub drops: Vec<DropSpawn>,
    pub inventories: BTreeMap<InventoryId, Vec<Option<Stack>>>,
    pub entity_spawns: Vec<EntitySpawn>,
    pub entity_changes: BTreeMap<u64, EntityChange>,
    pub entity_schedules: BTreeMap<u64, Option<u64>>,
    pub admin_spawns: Vec<String>,
}

pub struct Context<'a> {
    snapshot: &'a mut dyn Snapshot,
    blocks: BTreeMap<Cell, Block>,
    block_preimages: BTreeMap<Cell, Block>,
    inventories: BTreeMap<InventoryId, Vec<Slot>>,
    handler_namespace: Option<String>,
    handler_key: Option<String>,
    // Cached state carries the namespace whose access the snapshot validated.
    entity_overlay: BTreeMap<u64, (String, Option<Vec<u8>>)>,
    plan: Plan,
    remaining: usize,
    failure: Option<Error>,
}

impl<'a> Context<'a> {
    pub fn action_id(&self) -> Option<u128> {
        self.snapshot.action_id()
    }
    /// Stable per-handler seed for host-provided sequential PRNGs. Does not
    /// consume the script operation budget; it reads immutable world metadata.
    pub fn random_stream_seed(&self) -> u64 {
        let mut salt = 0xcbf2_9ce4_8422_2325u64;
        for byte in self.handler_key.as_deref().unwrap_or("").bytes() {
            salt = (salt ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        cell_random(self.snapshot.seed() ^ salt, [0; 3], 0)
    }
    pub fn tick(&self) -> u64 {
        self.snapshot.tick()
    }
    pub fn player(&self) -> Option<InventoryId> {
        self.snapshot.player().map(InventoryId::Player)
    }
    pub fn player_position(&self) -> Option<[f32; 3]> {
        self.snapshot.player_position()
    }
    /// Explicit creative grant, gated by the host's authenticated admin session.
    /// Ordinary `give` remains available to gameplay rewards without admin access.
    pub fn admin_give(&mut self, item: &str, count: u16) -> Result<bool, Error> {
        self.charge()?;
        if !self.snapshot.admin() {
            return self.fail(Error::Invalid("admin access denied".into()));
        }
        let owner = self
            .player()
            .ok_or_else(|| Error::Invalid("no player".into()))?;
        self.give(owner, Stack::new(item, count))
    }
    /// Host resolves the creature's registered key and chooses a supported nearby
    /// position; the handler cannot supply coordinates or allocate an entity ID.
    pub fn admin_spawn(&mut self, key: &str) -> Result<(), Error> {
        self.charge()?;
        if !self.snapshot.admin() {
            return self.fail(Error::Invalid("admin access denied".into()));
        }
        if !self.plan.admin_spawns.is_empty() || key.len() > 128 || !key.is_ascii() {
            return self.fail(Error::Invalid("invalid admin spawn".into()));
        }
        self.plan.admin_spawns.push(key.into());
        Ok(())
    }
    pub fn new(snapshot: &'a mut dyn Snapshot, operation_budget: usize) -> Self {
        Self {
            snapshot,
            blocks: BTreeMap::new(),
            block_preimages: BTreeMap::new(),
            inventories: BTreeMap::new(),
            handler_namespace: None,
            handler_key: None,
            entity_overlay: BTreeMap::new(),
            plan: Plan::default(),
            remaining: operation_budget,
            failure: None,
        }
    }

    /// The host dispatches a startup-resolved owner. Language bindings expose
    /// handler operations, not a way to select or impersonate this registration.
    pub fn dispatch(
        &mut self,
        registration: &HandlerRegistration,
        event: &Event,
    ) -> Result<(), Error> {
        self.handler_namespace = registration
            .key
            .split_once(':')
            .map(|(namespace, _)| namespace.to_owned());
        self.handler_key = Some(registration.key.clone());
        let result = registration.handler.handle(self, event);
        self.handler_namespace = None;
        self.handler_key = None;
        result
    }

    /// Stable per-handler random word. The caller chooses an explicit cell and
    /// sequence, so retries do not depend on callback order or VM RNG state.
    pub fn random(&mut self, cell: Cell, sequence: u64) -> Result<u64, Error> {
        self.charge()?;
        let Some(key) = self.handler_key.as_deref() else {
            return self.fail(Error::Invalid(
                "random requires a registered handler".into(),
            ));
        };
        let mut salt = 0xcbf2_9ce4_8422_2325u64;
        for byte in key.bytes() {
            salt = (salt ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Ok(cell_random(self.snapshot.seed() ^ salt, cell, sequence))
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
        let before = self.block(cell)?;
        let block = match self.snapshot.state(state) {
            Ok(block) => block,
            Err(error) => return self.fail(error),
        };
        self.plan.blocks.insert(cell, block.state.clone());
        self.block_preimages.entry(cell).or_insert(before);
        self.blocks.insert(cell, block);
        Ok(())
    }

    /// Changed preimages and proposed values in stable coordinate order.
    /// Hosts use this to dispatch support/neighbor decisions on the same overlay.
    pub fn staged_block_transitions(&self) -> Vec<(Cell, Block, Block)> {
        self.plan
            .blocks
            .keys()
            .filter_map(|cell| {
                let before = self.block_preimages.get(cell)?;
                let after = self.blocks.get(cell)?;
                (before != after).then(|| (*cell, before.clone(), after.clone()))
            })
            .collect()
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
