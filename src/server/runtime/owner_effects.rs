//! Transient owner-system effects routed through the registered vocabulary.
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
    EffectKindId, EffectKindRegistryFrozen, RegisteredEffectBuffer, RegisteredEffectLimits,
    route_registered_effects, MAX_EFFECTS_PER_BATCH, MAX_EFFECTS_PER_OWNER,
    MAX_EFFECTS_PER_PRODUCER_TICK,
};
use super::super::parallel::{
    BatchId, JobKey, OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerStore, OwnerWaveLimits,
};
use super::super::registry::{ExecutableSystem, SystemId};
use super::super::simulation::TickId;
use std::any::Any;
use std::collections::BTreeMap;
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
/// that job's effect emissions. The replacement still commits through the
/// validated owner-wave path; the emissions are routed at the barrier and
/// never persist.
pub(in crate::server) struct OwnerEffectPatch {
    state: OwnerData,
    effects: Vec<EmittedOwnerEffect>,
}

impl OwnerEffectPatch {
    pub(in crate::server) fn new(state: OwnerData, effects: Vec<EmittedOwnerEffect>) -> Self {
        Self { state, effects }
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

/// Routes a validated producer wave's emissions and runs each live
/// destination's registered consumer at the commit barrier.
///
/// Producer patches arrive in stable wave order and each producer's emissions
/// keep their handler order, so the routed set is deterministic across worker
/// scheduling. Returns the `(system, owner)` wake list the runtime schedules
/// for normal work next tick. Consumer outputs are transient scratch: they
/// are validated and dropped here, never committed.
///
/// When `drop_before_delivery` is set, intents are still built and routed
/// (identical bound enforcement) but the batch is discarded before any
/// consumer runs: the producer commits still apply while no destination is
/// woken. Deliveries to owners absent from every live store are skipped in
/// place; a missing destination only costs latency.
///
/// Every failure is `WouldBlock`: the producing work defers and retries, and
/// nothing commits.
#[allow(clippy::too_many_arguments)]
pub(in crate::server) fn route_and_consume(
    stores: &BTreeMap<SystemId, OwnerStore<OwnerData>>,
    patches: &[OwnerPatch],
    tick: TickId,
    system: &ExecutableSystem,
    batch_wave: u16,
    effect_kinds: &EffectKindRegistryFrozen,
    limits: &OwnerWaveLimits,
    drop_before_delivery: bool,
) -> Result<Vec<(SystemId, OwnerKey)>, io::Error> {
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
            total_deliveries: system
                .max_effects_per_tick()
                .min(MAX_EFFECTS_PER_BATCH),
            per_destination: system
                .max_effects_per_tick()
                .min(MAX_EFFECTS_PER_OWNER),
            payload_bytes: RegisteredEffectLimits::default().payload_bytes,
        },
    )
    .map_err(|error| blocked(format!("effect routing rejected: {error:?}")))?;
    if drop_before_delivery {
        return Ok(Vec::new());
    }

    let mut wakes = Vec::new();
    for group in batch.owners() {
        for (id, store) in stores {
            let Some(revision) = store.revision(group.owner) else {
                continue;
            };
            let snapshot = store
                .snapshot(group.owner)
                .map_err(|error| blocked(format!("destination snapshot missing: {error:?}")))?;
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
                    group.owner,
                    usage.writes,
                    usage.effects
                )));
            }
            if usage.estimated_bytes > limits.max_patch_bytes_per_job {
                return Err(blocked(format!(
                    "effect consumer for {:?} exceeds its scratch bound",
                    group.owner
                )));
            }
            wakes.push((id.clone(), group.owner));
        }
    }
    Ok(wakes)
}

fn blocked(reason: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::WouldBlock, reason.into())
}

#[cfg(test)]
#[path = "owner_effects/tests.rs"]
mod tests;
