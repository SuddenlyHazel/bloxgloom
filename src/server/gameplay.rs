//! Translation from public gameplay plans to existing authoritative participants.
use super::durable::TerrainReads;
use crate::content::Catalog;
use crate::world::{BlockId, ChunkKey, PreparedEdit, World};
use bloxgloom_host_api::gameplay::{Block, Cell, Context, Error, Snapshot};
use std::io;
mod entities;
mod entity_inventory;
pub(super) mod inventory;

pub(super) struct Participants<'a> {
    pub actor: Option<(u128, &'a crate::inventory::Inventory)>,
    pub entities: &'a super::entities::EntityStore,
}

pub(super) fn block(catalog: &Catalog, id: BlockId) -> Result<Block, Error> {
    let state = catalog
        .state(id)
        .ok_or_else(|| Error::Host("unknown stored block state".into()))?;
    let definition = catalog
        .block_type(state.block_type)
        .ok_or_else(|| Error::Host("unknown stored block type".into()))?;
    Ok(Block {
        state: state.key.clone(),
        block_type: definition.key.to_string(),
        primary_item: catalog
            .primary_block_item(id)
            .and_then(|id| catalog.item(id))
            .map(|item| item.key.to_string()),
    })
}

pub(super) fn error(error: Error) -> io::Error {
    let kind = match error {
        Error::Unavailable(_) => io::ErrorKind::WouldBlock,
        Error::Host(_) => io::ErrorKind::InvalidData,
        _ => io::ErrorKind::InvalidInput,
    };
    io::Error::new(kind, error)
}

struct WorldSnapshot<'a> {
    world: &'a mut World,
    reads: &'a mut TerrainReads,
    requested: &'a mut Vec<ChunkKey>,
    actor: Option<(u128, &'a crate::inventory::Inventory)>,
    inventory_read: bool,
    entities: Option<&'a super::entities::EntityStore>,
}
impl Snapshot for WorldSnapshot<'_> {
    fn project_entity_state(&self, id: u64, state: &[u8]) -> Result<Vec<u8>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::project(store, id, state)
    }
    fn nearby_entities(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> Result<Vec<bloxgloom_host_api::gameplay::Entity>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::nearby(self.world.catalog(), self.reads, store, position, radius)
    }
    fn entity_state(&mut self, id: u64, owner: &str) -> Result<Option<Vec<u8>>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::state(self.world.catalog(), self.reads, store, id, owner)
    }
    fn validate_entity_state(&self, key: &str, owner: &str, state: &[u8]) -> Result<(), Error> {
        entities::validate_state(self.world.catalog(), key, owner, state)
    }
    fn entity(&mut self, id: u64) -> Result<Option<bloxgloom_host_api::gameplay::Entity>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::read(self.world.catalog(), self.reads, store, id)
    }
    fn anchored_entity_at(&mut self, cell: Cell) -> Result<Option<u64>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::anchored(self.reads, store, cell)
    }
    fn player(&self) -> Option<u128> {
        self.actor.map(|(profile, _)| profile)
    }
    fn inventory(
        &mut self,
        owner: bloxgloom_host_api::gameplay::InventoryId,
    ) -> Result<Vec<bloxgloom_host_api::gameplay::Slot>, Error> {
        if let bloxgloom_host_api::gameplay::InventoryId::Entity(id) = owner {
            let store = self.entities.ok_or(Error::InventoryUnavailable(owner))?;
            return entity_inventory::capture(self.world.catalog(), self.reads, store, id);
        }
        let Some((profile, value)) = self.actor else {
            return Err(Error::InventoryUnavailable(owner));
        };
        if owner != bloxgloom_host_api::gameplay::InventoryId::Player(profile) {
            return Err(Error::InventoryUnavailable(owner));
        }
        self.inventory_read = true;
        inventory::capture(self.world.catalog(), value)
    }
    fn validate_stack(&self, stack: &bloxgloom_host_api::gameplay::Stack) -> Result<(), Error> {
        inventory::stack(self.world.catalog(), stack).map(|_| ())
    }
    fn inventory_accepts(
        &self,
        owner: bloxgloom_host_api::gameplay::InventoryId,
        slot: usize,
        stack: &bloxgloom_host_api::gameplay::Stack,
    ) -> bool {
        match owner {
            bloxgloom_host_api::gameplay::InventoryId::Player(profile) => self
                .actor
                .is_some_and(|(id, inventory)| id == profile && slot < inventory.slots.len()),
            bloxgloom_host_api::gameplay::InventoryId::Entity(id) => {
                self.entities.is_some_and(|store| {
                    entity_inventory::accepts(self.world.catalog(), store, id, slot, stack)
                })
            }
        }
    }
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        let [x, y, z] = cell;
        let id = self
            .reads
            .read(self.world, x, y, z)
            .map_err(|e| match e.kind() {
                io::ErrorKind::WouldBlock => Error::Unavailable(cell),
                io::ErrorKind::QuotaExceeded => Error::BudgetExceeded,
                _ => Error::Host(e.to_string()),
            })?;
        let Some(id) = id else {
            let key = crate::world::world_to_chunk(x, y, z).0;
            if !self.requested.contains(&key) {
                self.requested.push(key);
            }
            return Err(Error::Unavailable(cell));
        };
        block(self.world.catalog(), id)
    }
    fn state(&self, key: &str) -> Result<Block, Error> {
        let catalog = self.world.catalog();
        block(
            catalog,
            catalog
                .state_by_key(key)
                .ok_or_else(|| Error::UnknownContent(key.into()))?,
        )
    }
    fn item_exists(&self, key: &str) -> bool {
        self.world.catalog().item_by_key(key).is_some()
    }
}

pub(super) type Edit = (i32, i32, i32, BlockId);
pub(super) type Spawn = ([f32; 3], crate::inventory::Stack, std::time::Duration);
pub(super) type Removal = (BlockId, Cell, bloxgloom_host_api::gameplay::RemovalCause);

pub(super) struct OperationInput<'a> {
    pub edits: &'a [Edit],
    pub removals: &'a [Removal],
    pub seed: u64,
    pub tick: u64,
}

pub(super) struct WorldPlan {
    pub entity_updates: Vec<super::entities::PreparedEntityTransaction>,
    pub entity_spawns: Vec<super::entities::EntitySpawn>,
    pub edits: Vec<Edit>,
    pub prepared: Vec<PreparedEdit>,
    pub drops: Vec<Spawn>,
    pub inventory: Option<crate::inventory::Inventory>,
}

/// Invoke decision owners with one shared overlay. Every terrain/drop effect is
/// translated into participants of the caller's existing CommitAction.
pub(super) fn plan_removals(
    world: &mut World,
    reads: &mut TerrainReads,
    requested: &mut Vec<ChunkKey>,
    input: OperationInput<'_>,
    participants: Participants<'_>,
) -> io::Result<WorldPlan> {
    use bloxgloom_host_api::gameplay::{Event, EventKind, cell_random};
    let OperationInput {
        edits,
        removals,
        seed,
        tick,
    } = input;
    let actor = participants.actor;
    let catalog = world.catalog_arc();
    // Preparation is invisible. Its per-chunk version is stable random input
    // for existing harvest behavior; an expanded overlay is prepared below.
    let prepared = world.prepare_edits(edits)?;
    let mut snapshot = WorldSnapshot {
        world,
        reads,
        requested,
        actor,
        inventory_read: false,
        entities: Some(participants.entities),
    };
    let mut context = Context::new(&mut snapshot, 4096);
    for &(x, y, z, state) in edits {
        let state = catalog
            .state(state)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown block state"))?;
        context.set_block([x, y, z], &state.key).map_err(error)?;
    }
    for &(id, cell, cause) in removals {
        let previous = block(&catalog, id).map_err(error)?;
        let Some(handler) = catalog.gameplay_handler(EventKind::BlockRemoved, &previous.block_type)
        else {
            continue;
        };
        let key = crate::world::world_to_chunk(cell[0], cell[1], cell[2]).0;
        let version = prepared
            .iter()
            .find(|edit| edit.key == key)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "removal outside staged edits")
            })?
            .new_version;
        let event = Event::BlockRemoved {
            cell,
            previous,
            cause,
            random: cell_random(seed, cell, version),
        };
        context.dispatch(handler, &event).map_err(|e| {
            let e = error(e);
            io::Error::new(e.kind(), format!("{}: {e}", handler.key))
        })?;
    }
    let mut plan = context.finish().map_err(error)?;
    let inventory = if snapshot.inventory_read {
        let (profile, before) = actor.expect("inventory read requires actor capture");
        let owner = bloxgloom_host_api::gameplay::InventoryId::Player(profile);
        Some(match plan.inventories.remove(&owner) {
            Some(slots) => inventory::apply(&catalog, before, slots).map_err(error)?,
            None => before.clone(),
        })
    } else {
        None
    };
    let mut entity_updates = Vec::new();
    for (owner, slots) in plan.inventories {
        let bloxgloom_host_api::gameplay::InventoryId::Entity(id) = owner else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "uncaptured gameplay inventory output",
            ));
        };
        if let Some(update) =
            entity_inventory::prepare(&catalog, reads, participants.entities, id, slots)
                .map_err(error)?
        {
            entity_updates.push(update);
        }
    }
    use bloxgloom_host_api::gameplay::EntityChange;
    for (id, change) in plan.entity_changes {
        let id = super::entities::EntityId::new(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid entity ID"))?;
        let before = participants.entities.snapshot(id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::WouldBlock, "entity changed during planning")
        })?;
        let transaction = match change {
            EntityChange::Update { state, .. } => participants.entities.prepare_update(
                id,
                before.revision,
                super::entities::EntityPatch {
                    payload: Some(super::entities::EntityPayload::new(state)),
                    ..Default::default()
                },
            ),
            EntityChange::Remove { .. } => {
                participants.entities.prepare_despawn(id, before.revision)
            }
        }
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        entity_updates.push(transaction);
    }
    let final_edits = plan
        .blocks
        .into_iter()
        .map(|([x, y, z], key)| {
            catalog
                .state_by_key(&key)
                .map(|id| (x, y, z, id))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "staged state disappeared")
                })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut original = edits.to_vec();
    original.sort_by_key(|&(x, y, z, _)| [x, y, z]);
    let prepared = if final_edits == original {
        prepared
    } else {
        world.prepare_edits(&final_edits)?
    };
    let mut entity_spawns = Vec::with_capacity(plan.entity_spawns.len());
    for spawn in plan.entity_spawns {
        let id = catalog.entity_type_id_by_key(&spawn.key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "unknown gameplay entity")
        })?;
        let target = spawn.position.map(|n| n.floor() as i32);
        if removals.iter().all(|(_, source, _)| {
            (0..3).any(|i| (i64::from(target[i]) - i64::from(source[i])).abs() > 8)
        }) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "entity spawn outside interaction radius",
            ));
        }
        let state = if let Some(&(_, _, _, state)) = final_edits
            .iter()
            .find(|&&(x, y, z, _)| [x, y, z] == target)
        {
            state
        } else {
            let [x, y, z] = target;
            let Some(state) = reads.read(world, x, y, z)? else {
                let key = crate::world::world_to_chunk(x, y, z).0;
                if !requested.contains(&key) {
                    requested.push(key);
                }
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "entity spawn terrain unavailable",
                ));
            };
            state
        };
        if catalog.block_flags(state) & crate::content::SOLID != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "entity spawn is obstructed",
            ));
        }
        entity_spawns.push(super::entities::EntitySpawn::Mobile {
            entity_type: id,
            position: spawn.position,
            payload: super::entities::EntityPayload::new(spawn.state),
            spawn_tick: tick,
        });
    }
    let drops = plan
        .drops
        .into_iter()
        .map(|drop| {
            Ok((
                drop.position,
                inventory::stack(&catalog, &drop.stack).map_err(error)?,
                std::time::Duration::from_millis(u64::from(drop.pickup_delay_ms)),
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(WorldPlan {
        entity_updates,
        entity_spawns,
        edits: final_edits,
        prepared,
        drops,
        inventory,
    })
}

pub(super) fn combine_entities(
    store: &super::entities::EntityStore,
    initial: Option<super::entities::PreparedEntityTransaction>,
    mut updates: Vec<super::entities::PreparedEntityTransaction>,
) -> io::Result<Option<super::entities::PreparedEntityTransaction>> {
    if updates.is_empty() {
        return Ok(initial);
    }
    updates.extend(initial);
    store
        .combine_prepared(updates)
        .map(Some)
        .map_err(io::Error::other)
}
