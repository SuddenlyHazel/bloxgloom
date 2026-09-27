//! Frozen dispatch for trusted block lifecycle planners.
//!
//! A block type may register one placement/break pair at startup. The generic
//! durable command path selects a hook by catalogued type, never by a central
//! hard-coded block ID switch.
//!
//! Hooks never receive `&mut State`. They inspect a read-only
//! [`BlockActionContext`] and plan through a [`BlockCommitBuilder`], which
//! exposes only world-edit preparation and chunk-load requests; entity and
//! drop planning (`EntityStore::prepare_*`, `drops::plan_*`) already take
//! shared borrows. Hooks only prepare one `CommitAction`; the coordinator
//! still owns full-key WAL admission and post-receipt visibility.

use super::State;
use super::durable::CommitAction;
use super::durable::actions::BlockEditCommand;
use super::entities::EntityStore;
use super::simulation::TickId;
use crate::content::{BlockStateId, BlockTypeId, Catalog};
use crate::world::{BlockId, ChunkKey, World, world_to_chunk};
use std::collections::{BTreeMap, HashMap};
use std::io::{self, ErrorKind};
use std::sync::Arc;

const MAX_BLOCK_ACTION_HANDLERS: usize = 65_536;

/// Read-only planning view handed to block lifecycle hooks. Every member is
/// a shared borrow or a cheap owned handle, so a hook can inspect client,
/// entity, and catalog state but cannot mutate the coordinator.
pub(super) struct BlockActionContext<'a> {
    catalog: Arc<Catalog>,
    clients: &'a HashMap<u64, super::Client>,
    entities: &'a EntityStore,
    seed: u64,
    lifecycles: &'a super::lifecycle::Registry,
}

impl BlockActionContext<'_> {
    pub(super) fn lifecycle(
        &self,
        state: BlockStateId,
    ) -> io::Result<Arc<super::lifecycle::Resolved>> {
        self.lifecycles
            .for_state(&self.catalog, state)
            .cloned()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "no registered lifecycle"))
    }
    pub(super) fn catalog(&self) -> Arc<Catalog> {
        Arc::clone(&self.catalog)
    }

    pub(super) fn client(&self, id: u64) -> Option<&super::Client> {
        self.clients.get(&id)
    }

    pub(super) fn clients(&self) -> &HashMap<u64, super::Client> {
        self.clients
    }

    pub(super) fn entities(&self) -> &EntityStore {
        self.entities
    }

    pub(super) fn seed(&self) -> u64 {
        self.seed
    }
}

/// Narrow planning surface for block lifecycle hooks. A hook stages world
/// edits and records chunk-load requests here; it holds no other `&mut`
/// borrow, so live client, entity, drop, and durability state stay
/// unreachable except through these planning calls.
pub(super) struct BlockCommitBuilder<'a> {
    world: &'a mut World,
    requested_chunks: Vec<ChunkKey>,
    terrain_reads: super::durable::TerrainReads,
}

impl BlockCommitBuilder<'_> {
    pub(super) fn plan_removals(
        &mut self,
        edits: &[super::gameplay::Edit],
        removals: &[super::gameplay::Removal],
        seed: u64,
        tick: u64,
        participants: super::gameplay::Participants<'_>,
    ) -> io::Result<super::gameplay::WorldPlan> {
        super::gameplay::plan_removals(
            self.world,
            &mut self.terrain_reads,
            &mut self.requested_chunks,
            super::gameplay::OperationInput {
                edits,
                removals,
                seed,
                tick,
                action: None,
            },
            participants,
        )
    }
    /// Reads one cell, recording its chunk for the coordinator to request
    /// when it is not resident. Mirrors `actions::cached_block_or_request`:
    /// a miss defers the edit with `WouldBlock` after the load is queued.
    /// Recorded requests drain even when the hook fails.
    pub(super) fn cached_block_or_request(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        reason: &'static str,
    ) -> io::Result<BlockId> {
        if let Some(block) = self.terrain_reads.read(self.world, x, y, z)? {
            return Ok(block);
        }
        let key = world_to_chunk(x, y, z).0;
        if !self.requested_chunks.contains(&key) {
            self.requested_chunks.push(key);
        }
        Err(io::Error::new(ErrorKind::WouldBlock, reason))
    }

    #[cfg(test)]
    fn prepare_edits(
        &mut self,
        edits: &[(i32, i32, i32, BlockId)],
    ) -> io::Result<Vec<crate::world::PreparedEdit>> {
        for &(x, y, z, _) in edits {
            self.cached_block_or_request(x, y, z, "test hook edit chunk unavailable")?;
        }
        self.world.prepare_edits(edits)
    }

    fn take_requested_chunks(&mut self) -> Vec<ChunkKey> {
        std::mem::take(&mut self.requested_chunks)
    }
}

pub(super) type BlockEditHook = fn(
    &BlockActionContext,
    &mut BlockCommitBuilder,
    TickId,
    BlockEditCommand,
    BlockStateId,
) -> io::Result<CommitAction>;

/// Invokes one lifecycle hook with split coordinator borrows, then queues
/// any chunk loads the hook recorded. The hook's outcome decides: recorded
/// requests are best-effort prefetch, so a failing request never discards a
/// planned commit (the edit simply defers on a later tick while its chunk
/// is still not resident) and never masks the hook's own error.
pub(super) fn invoke_hook(
    hook: BlockEditHook,
    state: &mut State,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockStateId,
) -> io::Result<CommitAction> {
    let context = BlockActionContext {
        catalog: state.world.catalog_arc(),
        clients: &state.clients,
        entities: &state.entities,
        seed: state.seed,
        lifecycles: &state.lifecycles,
    };
    let mut builder = BlockCommitBuilder {
        world: &mut state.world,
        requested_chunks: Vec::new(),
        terrain_reads: Default::default(),
    };
    let result = hook(&context, &mut builder, tick, command, previous).and_then(|mut action| {
        action
            .terrain_reads
            .extend(std::mem::take(&mut builder.terrain_reads))?;
        Ok(action)
    });
    let requested = builder.take_requested_chunks();
    for key in requested {
        let _ = super::streaming::request_chunk(state, key);
    }
    result
}

#[derive(Clone, Copy)]
pub(super) struct BlockActionHooks {
    pub(super) place: BlockEditHook,
    pub(super) break_block: BlockEditHook,
}

impl BlockActionHooks {
    pub(super) const fn new(place: BlockEditHook, break_block: BlockEditHook) -> Self {
        Self { place, break_block }
    }
}

pub(super) struct BlockActionRegistryBuilder<'a> {
    catalog: &'a Catalog,
    hooks: BTreeMap<BlockTypeId, BlockActionHooks>,
}

impl<'a> BlockActionRegistryBuilder<'a> {
    pub(super) fn new(catalog: &'a Catalog) -> Self {
        Self {
            catalog,
            hooks: BTreeMap::new(),
        }
    }

    pub(super) fn register(
        &mut self,
        block_type: BlockTypeId,
        hooks: BlockActionHooks,
    ) -> io::Result<()> {
        if self.catalog.block_type(block_type).is_none() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "block action handler has no catalogued block type",
            ));
        }
        if self.hooks.len() >= MAX_BLOCK_ACTION_HANDLERS {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "too many block action handlers",
            ));
        }
        if self.hooks.contains_key(&block_type) {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "duplicate block action handler",
            ));
        }
        self.hooks.insert(block_type, hooks);
        Ok(())
    }

    pub(super) fn freeze(self) -> BlockActionRegistry {
        BlockActionRegistry { hooks: self.hooks }
    }
}

pub(super) struct BlockActionRegistry {
    hooks: BTreeMap<BlockTypeId, BlockActionHooks>,
}

impl BlockActionRegistry {
    pub(super) fn for_state(
        &self,
        catalog: &Catalog,
        state: BlockStateId,
    ) -> Option<BlockActionHooks> {
        let block_type = catalog.state(state)?.block_type;
        self.hooks.get(&block_type).copied()
    }
}

#[cfg(test)]
#[path = "block_actions/tests.rs"]
mod tests;
