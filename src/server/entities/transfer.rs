//! Cross-entity item transfer declarations and the pure exchange contract.
//!
//! Nothing can move an item between two entities except through here. A tick
//! planner declares a push or pull (`EntityItemTransfer`) on its own schedule; the
//! trusted durable layer (`super::super::durable::actions::entity`) resolves
//! both snapshots, runs both pure exchange hooks, and stages both payload
//! updates as one `Operation::Batch` with real `before` preimages on both
//! ends. The check and the move are the same statement: if the recipient
//! filled in the meantime its `before` is stale and the whole transaction is
//! rejected, so an item is never in neither place or in both.
//!
//! THE RULE for policies: planners return declared intent and never get a
//! write path. The exchange hooks below are pure functions of their inputs
//! (no wall clock, no RNG, no I/O): they compute candidate after-payloads,
//! and only the trusted layer turns those into a prepared transaction.
//!
//! Conservation contract for hook implementations:
//! - `withdraw` removes exactly `count` of `item` and returns the taken
//!   stack alongside the sender's after-payload. `None` means the stock is
//!   temporarily unavailable: the work defers (`WouldBlock`) and re-plans.
//! - `deposit` adds the entire stack or refuses it whole. A transfer that
//!   cannot fit is never partially applied: `None` defers, it never sheds
//!   items. `Err` from either hook rejects the whole plan (`InvalidInput`).
//! - Stack counts stay within the 128-block cap; the trusted layer
//!   re-validates the taken stack before staging.

use super::types::{EntityError, EntityId, EntityPayload};
use crate::content::Catalog;
use crate::inventory::{STACK_LIMIT, Stack};
use crate::items::ItemId;

/// Exact, component-preserving slot operations shared by inventory ports.
pub(super) fn put(slot: &mut Option<Stack>, stack: &Stack) -> bool {
    if stack.count == 0 || stack.count > STACK_LIMIT {
        return false;
    }
    match slot {
        None => {
            *slot = Some(stack.clone());
            true
        }
        Some(old)
            if old.item == stack.item
                && old.components == stack.components
                && u32::from(old.count) + u32::from(stack.count) <= u32::from(STACK_LIMIT) =>
        {
            old.count += stack.count;
            true
        }
        _ => false,
    }
}

pub(super) fn take(slot: &mut Option<Stack>, count: u16) -> Option<Stack> {
    let old = slot.as_mut()?;
    if count == 0 || count > old.count {
        return None;
    }
    let mut taken = old.clone();
    taken.count = count;
    old.count -= count;
    if old.count == 0 {
        *slot = None;
    }
    Some(taken)
}

/// One transfer of `count` of `item` with a visible peer. `push == false`
/// initiates a pull from `source`; `push == true` sends to that peer instead.
/// The initiating entity advances its own schedule, preserving the peer's.
/// A lost wake costs latency and never state: the
/// sender never deducts speculatively, it only loses stock inside the atomic
/// batch the receiver's trusted plan assembles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityItemTransfer {
    pub source: EntityId,
    /// When true, `source` names the destination and the ticking entity sends.
    pub push: bool,
    pub item: ItemId,
    pub count: u16,
}

impl EntityItemTransfer {
    /// Structural validation of the declared intent. Item identity is checked
    /// against the catalog by the trusted layer; this rejects only shapes no
    /// honest planner can emit.
    pub fn validate(&self, receiver: EntityId) -> Result<(), EntityError> {
        if self.source == receiver {
            return Err(EntityError::InvalidPayload);
        }
        if self.count == 0 || self.count > STACK_LIMIT {
            return Err(EntityError::InvalidPayload);
        }
        Ok(())
    }
}

/// Pure per-type item exchange used only through the trusted transfer plan.
/// Implementations must be deterministic functions of their inputs.
pub trait EntityTransferPolicy: Send + Sync + 'static {
    /// Automation discovery uses only the registered public projection.
    fn offers(&self, _public: &[u8]) -> Vec<Stack> {
        Vec::new()
    }
    fn accepts(&self, _public: &[u8], _stack: &Stack, _catalog: &Catalog) -> bool {
        false
    }
    /// Candidate sender payload after removing `count` of `item`, plus the
    /// exact taken stack. `None` defers: stock is unavailable right now.
    fn withdraw(
        &self,
        payload: &EntityPayload,
        item: ItemId,
        count: u16,
        catalog: &Catalog,
    ) -> Result<Option<(EntityPayload, Stack)>, EntityError>;

    /// Candidate receiver payload after adding the whole stack. `None`
    /// defers: the stack does not fit right now and must not be split.
    fn deposit(
        &self,
        payload: &EntityPayload,
        stack: &Stack,
        catalog: &Catalog,
    ) -> Result<Option<EntityPayload>, EntityError>;
}

#[cfg(test)]
#[path = "transfer/tests.rs"]
mod tests;
