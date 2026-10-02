//! General anchored own-state behavior. All callbacks are pure, deterministic,
//! bounded and retryable; they may run on workers. Unload is not removal.
use crate::{
    RegistrationError,
    entity::{Error, Payload},
    lifecycle::FootprintCell,
};
use std::{fmt, sync::Arc};

/// Generic EntityInteract envelope shared with action dispatchers. The host
/// checks reach, interest, identity, revision, and the registered own-state hook.
pub fn interaction_request(id: u64, revision: u64, request: &[u8]) -> Result<Vec<u8>, Error> {
    if id == 0 || revision == 0 || request.is_empty() || request.len() > 239 {
        return Err(Error::InvalidState);
    }
    let mut bytes = Vec::with_capacity(17 + request.len());
    bytes.push(4);
    bytes.extend(id.to_le_bytes());
    bytes.extend(revision.to_le_bytes());
    bytes.extend(request);
    Ok(bytes)
}

#[derive(Clone)]
pub struct AnchoredBlockEntity {
    pub entity: String,
    pub block: String,
    pub placement_item: String,
    pub anchor_state: String,
    pub footprint: Vec<FootprintCell>,
    /// Component-free units debited from the selected hotbar stack atomically.
    pub placement_cost: u16,
    /// Units released on any removal, never more than the placement debit.
    pub removal_refund: u16,
    pub schema_version: u16,
    /// Version callback semantics as well as the private/public codecs here.
    pub schema_fingerprint: u64,
    pub max_state_bytes: usize,
    pub max_public_bytes: usize,
    /// Persistent polling deadline. Terrain wakes are advisory accelerators;
    /// polling ensures support/neighbor changes are not lost across restart.
    pub interval: u32,
    /// Complete bounded terrain input, relative to the anchor. No implicit air
    /// for unavailable chunks. The host fences every captured chunk until apply.
    pub observe: Vec<[i32; 3]>,
    /// Default own-state use request. General action dispatch can reuse interact.
    pub interaction: Vec<u8>,
    pub behavior: Arc<dyn Behavior>,
}
impl fmt::Debug for AnchoredBlockEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnchoredBlockEntity")
            .field("entity", &self.entity)
            .finish()
    }
}
pub struct Cell<'a> {
    pub offset: [i32; 3],
    pub state: &'a str,
    pub solid: bool,
}
pub struct Context<'a> {
    pub environment: Option<crate::gameplay::Environment>,
    pub tags: Option<&'a dyn crate::queries::Tags>,
    pub anchor: [i32; 3],
    pub tick: u64,
    pub state: &'a Payload,
    pub cells: &'a [Cell<'a>],
}
pub enum Reaction {
    Keep,
    Update(Payload),
    /// Host clears the whole footprint, despawns, and refunds atomically.
    Remove,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalCause {
    Broken,
    Reaction,
    WorldEdit,
}
pub trait Behavior: Send + Sync + 'static {
    fn initialize(&self, anchor: [i32; 3]) -> Result<Payload, Error>;
    fn decode(&self, bytes: &[u8]) -> Result<Payload, Error>;
    fn encode(&self, state: &Payload) -> Result<Vec<u8>, Error>;
    fn public(&self, state: &Payload) -> Result<Vec<u8>, Error>;
    /// Level-triggered neighbor/support/invalidation hook. It observes current
    /// state, not an event log; do not count wake invocations as gameplay time.
    fn react(&self, context: &Context<'_>) -> Result<Reaction, Error>;
    /// Own-state only: never edits inventories or changes footprint ownership.
    fn interact(&self, state: &Payload, request: &[u8]) -> Result<Payload, Error>;
    /// Called for player destruction, self-removal and system invalidation.
    /// May reduce the configured refund, but never mint more than the original
    /// debit. This is a retryable planner, NOT a post-commit notification.
    fn refund(&self, _state: &Payload, _cause: RemovalCause, maximum: u16) -> Result<u16, Error> {
        Ok(maximum)
    }
}
impl AnchoredBlockEntity {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        crate::lifecycle::StorageBlockEntity {
            entity: self.entity.clone(),
            block: self.block.clone(),
            placement_item: self.placement_item.clone(),
            anchor_state: self.anchor_state.clone(),
            footprint: self.footprint.clone(),
            slots: 1,
            automation_faces: None,
        }
        .validate()?;
        let mut seen = std::collections::BTreeSet::new();
        if self.placement_cost == 0
            || self.placement_cost > 128
            || self.removal_refund > self.placement_cost
            || self.schema_version == 0
            || self.max_state_bytes == 0
            || self.max_state_bytes > 65_536
            || self.max_public_bytes > 4096
            || self.interval == 0
            || self.observe.len() > 64
            || self.interaction.len() > 239
            || self
                .observe
                .iter()
                .any(|v| v.iter().any(|n| n.unsigned_abs() > 16) || !seen.insert(*v))
        {
            return Err(RegistrationError("invalid anchored bounds".into()));
        }
        Ok(())
    }
}
