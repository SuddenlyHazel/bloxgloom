//! Transient owner-system effects routed through the registered vocabulary.
//! The advisory rule below applies only to `EmittedOwnerEffect`. The patch
//! envelope also carries explicit world edits and durable payload intents;
//! those are transaction participants and never pass through advisory routing
//! or its delivery-shedding switch.
//!
//! THE RULE: an effect may only cause work to happen SOONER. It must never
//! cause work to happen that would not otherwise happen. A producer handler
//! returns its durable owner replacement together with wake-like intents; at
//! the commit barrier those intents are routed with the shared
//! `effects::registered` machinery and each live destination is scheduled for
//! its own normal work next tick. A lost or dropped effect costs latency and
//! never state, so delivery needs no persistence.
//!
//! The barrier is all-or-nothing: any over-bound, unroutable, or
//! write-declaring effect set rejects the entire producer wave with
//! `WouldBlock` (defer the work) before anything commits, so a truncated
//! producer output can never be routed. Failures here never take the
//! coordinator's `InvalidData` path. Applying an effect never runs the
//! destination system again in the same tick; destination handlers run next
//! tick at the earliest.

use super::super::effects::{
    EffectKindId, EffectKindRegistryFrozen, MAX_EFFECTS_PER_BATCH, MAX_EFFECTS_PER_OWNER,
    MAX_EFFECTS_PER_PRODUCER_TICK, RegisteredEffectBuffer, RegisteredEffectLimits,
    route_registered_effects,
};
use super::super::parallel::{
    BatchId, JobKey, OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerSnapshot, OwnerWaveLimits,
};
use super::super::registry::{ExecutableSystem, SystemId};
use super::super::simulation::TickId;
use super::owner_durable::DurableOwnerStore;
use std::any::Any;
use std::io::{self, ErrorKind};
use std::sync::Arc;

/// One unvalidated effect emission from an owner handler: the registered kind
/// plus its typed payload. The barrier validates each emission against the
/// frozen registry in vector order, so handlers must enumerate emissions
/// deterministically for identical inputs to produce identical effect sets.
pub(in crate::server) struct EmittedOwnerEffect {
    kind: EffectKindId,
    payload: Arc<dyn Any + Send + Sync>,
}

impl EmittedOwnerEffect {
    #[allow(
        dead_code,
        reason = "Registered owner handlers construct typed wake emissions."
    )]
    pub(in crate::server) fn new<P: Any + Send + Sync>(kind: EffectKindId, payload: P) -> Self {
        Self {
            kind,
            payload: Arc::new(payload),
        }
    }

    pub(in crate::server) fn kind(&self) -> &EffectKindId {
        &self.kind
    }
}

/// Producer patch payload carrying the durable owner replacement alongside
/// advisory effects, explicit payload-free wakes, world edits, and durable
/// payload intents. Only advisory effects route transiently at the barrier.
pub(in crate::server) struct OwnerEffectPatch {
    state: OwnerData,
    effects: Vec<EmittedOwnerEffect>,
    durable_wakes: Vec<(SystemId, OwnerKey)>,
    world_edits: Vec<bloxgloom_host_api::system::BlockEdit>,
    drops: Vec<bloxgloom_host_api::system::DropSpawn>,
    entity_spawns: Vec<bloxgloom_host_api::system::EntitySpawn>,
    edit_cause: bloxgloom_host_api::system::EditCause,
    intents: Vec<bloxgloom_host_api::system::IntentRequest>,
}

impl OwnerEffectPatch {
    #[allow(
        dead_code,
        reason = "Registered owner handlers return state plus wake emissions."
    )]
    pub(in crate::server) fn new(state: OwnerData, effects: Vec<EmittedOwnerEffect>) -> Self {
        Self {
            state,
            effects,
            durable_wakes: Vec::new(),
            world_edits: Vec::new(),
            drops: Vec::new(),
            entity_spawns: Vec::new(),
            edit_cause: Default::default(),
            intents: Vec::new(),
        }
    }

    pub(in crate::server) fn with_durable_wakes(
        mut self,
        wakes: Vec<(SystemId, OwnerKey)>,
    ) -> Self {
        self.durable_wakes = wakes;
        self
    }

    pub(in crate::server) fn durable_wakes(patch: &OwnerPatch) -> &[(SystemId, OwnerKey)] {
        patch
            .payload::<OwnerEffectPatch>()
            .map_or(&[], |emission| &emission.durable_wakes)
    }

    pub(in crate::server) fn with_world_edits(
        mut self,
        edits: Vec<bloxgloom_host_api::system::BlockEdit>,
        cause: bloxgloom_host_api::system::EditCause,
    ) -> Self {
        self.world_edits = edits;
        self.edit_cause = cause;
        self
    }

    pub(in crate::server) fn edit_cause(
        patch: &OwnerPatch,
    ) -> bloxgloom_host_api::system::EditCause {
        patch
            .payload::<Self>()
            .map_or(Default::default(), |emission| emission.edit_cause)
    }

    pub(in crate::server) fn world_edits(
        patch: &OwnerPatch,
    ) -> &[bloxgloom_host_api::system::BlockEdit] {
        patch
            .payload::<OwnerEffectPatch>()
            .map_or(&[], |emission| &emission.world_edits)
    }

    pub(in crate::server) fn with_drops(
        mut self,
        drops: Vec<bloxgloom_host_api::system::DropSpawn>,
    ) -> Self {
        self.drops = drops;
        self
    }

    pub(in crate::server) fn drops(patch: &OwnerPatch) -> &[bloxgloom_host_api::system::DropSpawn] {
        patch
            .payload::<OwnerEffectPatch>()
            .map_or(&[], |emission| &emission.drops)
    }

    pub(in crate::server) fn with_entity_spawns(
        mut self,
        spawns: Vec<bloxgloom_host_api::system::EntitySpawn>,
    ) -> Self {
        self.entity_spawns = spawns;
        self
    }

    pub(in crate::server) fn entity_spawns(
        patch: &OwnerPatch,
    ) -> &[bloxgloom_host_api::system::EntitySpawn] {
        patch
            .payload::<OwnerEffectPatch>()
            .map_or(&[], |emission| &emission.entity_spawns)
    }

    pub(in crate::server) fn with_intents(
        mut self,
        intents: Vec<bloxgloom_host_api::system::IntentRequest>,
    ) -> Self {
        self.intents = intents;
        self
    }

    pub(in crate::server) fn intents(
        patch: &OwnerPatch,
    ) -> &[bloxgloom_host_api::system::IntentRequest] {
        patch
            .payload::<Self>()
            .map_or(&[], |emission| &emission.intents)
    }

    /// Actual emitted intent count for wave bound accounting. Plain patches
    /// carry none; anything else is rejected by patch validation.
    pub(in crate::server) fn emitted_count(patch: &OwnerPatch) -> usize {
        patch
            .payload::<OwnerEffectPatch>()
            .map_or(0, |emission| emission.effects.len())
    }

    /// Durable replacement for either patch shape. `None` means the payload
    /// is neither a plain replacement nor an effect emission.
    pub(in crate::server) fn replacement(patch: &OwnerPatch) -> Option<OwnerData> {
        if let Some(data) = patch.payload::<OwnerData>() {
            Some(data.clone())
        } else {
            patch
                .payload::<OwnerEffectPatch>()
                .map(|emission| emission.state.clone())
        }
    }
}

/// Routed wake sets from one producer wave: live destinations whose consumer
/// ran at the barrier, plus unloaded destinations with no live cell anywhere.
/// Unloaded destinations fan out to every registered system whose partition
/// accepts the owner, in canonical system order, so the flag is held exactly
/// where the owner can load. An owner no system accepts stages nothing: a
/// missing destination only costs latency.
pub(in crate::server) struct RoutedOwnerWakes {
    live: Vec<(SystemId, OwnerKey)>,
    unloaded: Vec<(SystemId, OwnerKey)>,
}

impl RoutedOwnerWakes {
    fn empty() -> Self {
        Self {
            live: Vec::new(),
            unloaded: Vec::new(),
        }
    }

    pub(in crate::server) fn live(&self) -> &[(SystemId, OwnerKey)] {
        &self.live
    }

    pub(in crate::server) fn unloaded(&self) -> &[(SystemId, OwnerKey)] {
        &self.unloaded
    }
}

/// Routes a validated producer wave's emissions and runs each live
/// destination's registered consumer at the commit barrier.
///
/// Producer patches arrive in stable wave order and each producer's emissions
/// keep their handler order, so the routed set is deterministic across worker
/// scheduling. Returns the live `(system, owner)` wake list the runtime
/// schedules for normal work next tick alongside the unloaded destinations
/// the runtime must hold durably. Consumer outputs are transient scratch:
/// they are validated and dropped here, never committed.
///
/// When `drop_before_delivery` is set, intents are still built and routed
/// (identical bound enforcement) but the batch is discarded before any
/// consumer runs: the producer commits still apply while no destination is
/// woken, live or durable.
///
/// Every failure is `WouldBlock`: the producing work defers and retries, and
/// nothing commits.
/// Every failure is `WouldBlock`: the producing work defers and retries, and
/// nothing commits.
#[allow(clippy::too_many_arguments)]
pub(in crate::server) fn route_and_consume(
    owners: &DurableOwnerStore,
    patches: &[OwnerPatch],
    tick: TickId,
    system: &ExecutableSystem,
    batch_wave: u16,
    effect_kinds: &EffectKindRegistryFrozen,
    limits: &OwnerWaveLimits,
    drop_before_delivery: bool,
) -> Result<RoutedOwnerWakes, io::Error> {
    let mut intents = Vec::new();
    for patch in patches {
        if let Some(emission) = patch.payload::<OwnerEffectPatch>() {
            if patch.usage().effects != emission.effects.len() {
                return Err(blocked(format!(
                    "owner {:?} declares {} effects but emits {}",
                    patch.owner(),
                    patch.usage().effects,
                    emission.effects.len()
                )));
            }
            if emission.effects.is_empty() {
                continue;
            }
            let mut buffer = RegisteredEffectBuffer::new(
                tick,
                system.phase(),
                system.id().clone(),
                patch.owner(),
                system
                    .max_effects_per_job()
                    .min(MAX_EFFECTS_PER_PRODUCER_TICK),
                effect_kinds,
            )
            .map_err(|error| blocked(format!("effect buffer rejected: {error:?}")))?;
            for emitted in &emission.effects {
                buffer
                    .emit_erased(emitted.kind(), Arc::clone(&emitted.payload))
                    .map_err(|error| blocked(format!("effect emit rejected: {error:?}")))?;
            }
            intents.extend(
                buffer
                    .finish()
                    .map_err(|error| blocked(format!("effect buffer overflowed: {error:?}")))?,
            );
        } else if patch.usage().effects != 0 {
            return Err(blocked(format!(
                "owner {:?} declares {} effects without emitting any",
                patch.owner(),
                patch.usage().effects
            )));
        }
    }

    let batch = route_registered_effects(
        intents,
        RegisteredEffectLimits {
            total_deliveries: system.max_effects_per_tick().min(MAX_EFFECTS_PER_BATCH),
            per_destination: system.max_effects_per_tick().min(MAX_EFFECTS_PER_OWNER),
            payload_bytes: RegisteredEffectLimits::default().payload_bytes,
        },
    )
    .map_err(|error| blocked(format!("effect routing rejected: {error:?}")))?;
    if drop_before_delivery {
        return Ok(RoutedOwnerWakes::empty());
    }

    let mut routed = RoutedOwnerWakes::empty();
    for group in batch.owners() {
        let mut live_cells = 0usize;
        for id in owners.systems() {
            let Some((revision, data)) = owners.snapshot(id, group.owner) else {
                continue;
            };
            let snapshot = OwnerSnapshot::new(group.owner, revision, Arc::new(data));
            debug_assert_eq!(snapshot.revision(), revision);
            let job = OwnerJob::new(
                id.clone(),
                JobKey::new(
                    BatchId::new(tick, system.phase(), batch_wave),
                    group.owner,
                    u64::MAX,
                    snapshot.revision(),
                ),
                vec![snapshot],
            )
            .map_err(|error| blocked(format!("destination job rejected: {error:?}")))?;
            let consumer = group
                .prepare_consumer(&job)
                .map_err(|error| blocked(format!("effect consumer rejected: {error:?}")))?;
            let usage = consumer.usage();
            if usage.writes != 0 || usage.effects != 0 {
                return Err(blocked(format!(
                    "effect consumer for {:?} declares writes={} effects={}: consumers schedule work and must not write authoritative state",
                    group.owner, usage.writes, usage.effects
                )));
            }
            if usage.estimated_bytes > limits.max_patch_bytes_per_job {
                return Err(blocked(format!(
                    "effect consumer for {:?} exceeds its scratch bound",
                    group.owner
                )));
            }
            routed.live.push((id.clone(), group.owner));
            live_cells += 1;
        }
        if live_cells == 0 {
            // No live cell anywhere: hold the wake durably exactly where the
            // owner can load, in canonical system order. Owners no system
            // accepts stage nothing.
            for id in owners.systems() {
                if owners.accepts_owner(id, group.owner) {
                    routed.unloaded.push((id.clone(), group.owner));
                }
            }
        }
    }
    Ok(routed)
}

fn blocked(reason: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::WouldBlock, reason.into())
}

#[cfg(test)]
#[path = "owner_effects/tests.rs"]
mod tests;
