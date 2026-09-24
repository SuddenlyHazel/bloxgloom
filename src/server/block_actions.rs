//! Frozen dispatch for trusted block lifecycle planners.
//!
//! A block type may register one placement/break pair at startup. The generic
//! durable command path selects a hook by catalogued type, never by a central
//! hard-coded block ID switch. Hooks only prepare one `CommitAction`; the
//! coordinator still owns full-key WAL admission and post-receipt visibility.

use super::State;
use super::durable::CommitAction;
use super::durable::actions::BlockEditCommand;
use super::simulation::TickId;
use crate::content::{BlockStateId, BlockTypeId, Catalog};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};

const MAX_BLOCK_ACTION_HANDLERS: usize = 65_536;

pub(super) type BlockEditHook =
    fn(&mut State, TickId, BlockEditCommand, BlockStateId) -> io::Result<CommitAction>;

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
