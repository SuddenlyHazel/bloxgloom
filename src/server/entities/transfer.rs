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

/// Snapshot-local equality identity. Assigned by exact interning during bounded
/// host capture, never a hash or a serialization of private component bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutomationStack {
    pub item: ItemId,
    pub count: u16,
    pub has_components: bool,
    pub key: u32,
}
impl AutomationStack {
    pub fn fits(self, slot: Option<Self>) -> bool {
        slot.is_none_or(|old| {
            old.key == self.key
                && u32::from(old.count) + u32::from(self.count) <= u32::from(STACK_LIMIT)
        })
    }
}

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

/// Maximum valid player-requested move from the current slots. Unlike owner
/// automation, a screen click asks for up to `maximum` items and may use a
/// smaller current stack or the remaining destination capacity.
pub(super) fn movable_count(
    source: &Option<Stack>,
    destination: &Option<Stack>,
    maximum: u16,
) -> Option<u16> {
    if maximum == 0 || maximum > STACK_LIMIT {
        return None;
    }
    let stack = source.as_ref()?;
    let space = match destination {
        None => STACK_LIMIT,
        Some(current) if current.item == stack.item && current.components == stack.components => {
            STACK_LIMIT.saturating_sub(current.count)
        }
        Some(_) => 0,
    };
    let count = maximum.min(stack.count).min(space);
    (count > 0).then_some(count)
}

pub(super) fn move_up_to(
    source: &mut Option<Stack>,
    destination: &mut Option<Stack>,
    maximum: u16,
) -> Option<u16> {
    let count = movable_count(source, destination, maximum)?;
    let stack = take(source, count)?;
    put(destination, &stack).then_some(count)
}

/// One transfer of `count` of `item` with a visible peer. `push == false`
/// initiates a pull from `source`; `push == true` sends to that peer instead.
/// The initiating entity advances its own schedule, preserving the peer's.
/// A lost wake costs latency and never state: the
/// sender never deducts speculatively, it only loses stock inside the atomic
/// batch the receiver's trusted plan assembles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityItemTransfer {
    pub route: Option<PortRoute>,
    pub source: EntityId,
    /// When true, `source` names the destination and the ticking entity sends.
    pub push: bool,
    pub item: ItemId,
    pub count: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortRoute {
    pub source: u8,
    pub destination: u8,
    /// Exact source slot selected from the captured revision's public inventory.
    pub source_slot: u8,
    /// Optional exact destination slot; None chooses the first exact fit.
    pub destination_slot: Option<u8>,
    pub from: [i32; 3],
    pub to: [i32; 3],
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
    /// Shared gameplay inventory service. Root policies implement slot filters
    /// and payload replacement; transaction assembly still owns publication.
    fn inventory_accepts(&self, _slot: u8, _stack: &Stack, _catalog: &Catalog) -> bool {
        false
    }
    fn replace_inventory(
        &self,
        _payload: &EntityPayload,
        _slots: Vec<Option<Stack>>,
        _catalog: &Catalog,
    ) -> Result<EntityPayload, EntityError> {
        Err(EntityError::InvalidPayload)
    }
    /// Restrict an already selected port to one absolute inventory slot. Port
    /// permissions still apply; unsupported selectors fail closed.
    fn at_slot(&self, _slot: u8) -> Option<std::sync::Arc<dyn EntityTransferPolicy>> {
        None
    }
    /// Bounded source offers retain slot identity through fit testing and apply.
    fn automation_offers(&self, _slots: &[Option<AutomationStack>]) -> Vec<(u8, AutomationStack)> {
        vec![]
    }
    fn automation_accepts(
        &self,
        _slots: &[Option<AutomationStack>],
        _stack: AutomationStack,
    ) -> bool {
        false
    }
    /// Host capture/commit only. Never returned to a behavior or retained in a
    /// neighbour worker view. Registered inventories are bounded to 54 slots.
    fn inventory_slots(&self, _payload: &EntityPayload) -> Result<Vec<Option<Stack>>, EntityError> {
        Ok(vec![])
    }
    fn ports(&self) -> Vec<String> {
        vec![]
    }
    fn port(
        &self,
        _index: u8,
        _face: [i32; 3],
    ) -> Option<std::sync::Arc<dyn EntityTransferPolicy>> {
        None
    }
    // Legacy projection-only reference planners are test fixtures. Production
    // automation must use the exact opaque inventory capture above.
    #[cfg(test)]
    fn offers(&self, _public: &[u8]) -> Vec<Stack> {
        Vec::new()
    }
    #[cfg(test)]
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
