//! Notification-only entity wake effects.
//!
//! An entity plan can see itself, the world, and its neighbours' public
//! views, but it cannot ask anyone to do anything. This module is that
//! channel, and it carries exactly one kind of request: "run your own tick
//! sooner".
//!
//! THE RULE: an effect may only cause work to happen SOONER. It must never
//! cause work to happen that would not otherwise happen. A wake is delivered
//! as a transient tick attempt for the destination entity. The destination's
//! own planner still decides, and its durable work still flows through
//! `CommitAction` / `PreparedEntityTransaction` with before preimages and
//! revision checks. A lost or dropped wake therefore costs latency and never
//! state, so wakes are never persisted.
//!
//! Plans declare wakes as neighbour entity IDs; this module maps them onto
//! the shared registered-effect vocabulary (`bloxgloom:wake_entity`) and
//! routes them with the same buffer, ordering, and all-or-error bounds as
//! every other registered effect. There is deliberately no second effect
//! vocabulary here.

use super::types::{EntityId, EntityView, MAX_PLAN_NEIGHBOURS};
use crate::server::effects::{
    EffectConsumerOutput, EffectKindId, EffectKindRegistry, EffectKindRegistryFrozen,
    EffectRegistryError, RegisteredEffectBuffer, RegisteredEffectLimits, route_registered_effects,
};
use crate::server::parallel::{OwnerJob, OwnerKey, PatchUsage};
use crate::server::registry::SystemId;
use crate::server::simulation::{Phase, TickId};
use std::io::{self, ErrorKind};

/// Registered kind for transient entity wake notifications.
pub(super) const WAKE_KIND_ID: &str = "bloxgloom:wake_entity";
/// Stable producer label for wakes emitted by entity tick plans.
pub(super) const TICK_PRODUCER_ID: &str = "bloxgloom:entity_tick";
/// Stable producer label for wakes emitted by entity interaction plans.
pub(super) const INTERACT_PRODUCER_ID: &str = "bloxgloom:entity_interact";

/// Maximum wakes declared by one plan. Destinations are confined to the
/// planner's captured neighbour view, so the neighbour capture bound applies.
pub(super) const MAX_WAKES_PER_PLAN: usize = MAX_PLAN_NEIGHBOURS;

/// Typed payload for one wake notification: the entity to run sooner. No
/// state travels with the wake; the destination re-reads everything through
/// its own planner inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EntityWake {
    id: EntityId,
}

impl EntityWake {
    const fn id(self) -> EntityId {
        self.id
    }
}

/// Registers the single notification kind entity plans may emit. The consumer
/// declares a zero-write, zero-effect scratch output: wake delivery never
/// runs through the parallel owner-patch path (delivery only schedules a
/// transient tick attempt), and the registered consumer must not gain a write
/// path either.
pub(super) fn register_wake_kind(
    registry: &mut EffectKindRegistry,
) -> Result<(), EffectRegistryError> {
    registry.register(
        EffectKindId::new(WAKE_KIND_ID)?,
        1,
        8,
        1,
        |_: &EntityWake| 8usize,
        |wake: &EntityWake| {
            if wake.id.get() == 0 {
                Err("wake destination is the reserved zero entity".to_owned())
            } else {
                Ok(())
            }
        },
        |wake: &EntityWake| Ok(vec![OwnerKey::Entity(wake.id.get())]),
        |_: &OwnerJob, _: &[&EntityWake]| {
            Ok(EffectConsumerOutput::new((), PatchUsage::default()))
        },
    )
}

pub(super) fn tick_producer() -> SystemId {
    SystemId::new(TICK_PRODUCER_ID).expect("static entity tick producer ID")
}

pub(super) fn interact_producer() -> SystemId {
    SystemId::new(INTERACT_PRODUCER_ID).expect("static entity interact producer ID")
}

fn wake_kind() -> EffectKindId {
    EffectKindId::new(WAKE_KIND_ID).expect("static wake effect kind ID")
}

/// Sorts, dedupes, bounds, and confines a plan's wake list to its captured
/// neighbour view. Emission order is therefore canonical: repeated plans over
/// the same inputs produce identical effect sets regardless of planner
/// iteration order.
///
/// An over-bound list defers the plan (`WouldBlock`): the bound is a local
/// capacity condition, never a reason to stop the coordinator. A destination
/// outside the captured view rejects the whole plan (`InvalidInput`): the
/// planner may only wake neighbours it can already see.
pub(super) fn canonical_wakes(
    neighbours: &EntityView,
    wakes: &[EntityId],
) -> Result<Vec<EntityId>, io::Error> {
    let mut canonical = wakes.to_vec();
    canonical.sort_unstable();
    canonical.dedup();
    if canonical.len() > MAX_WAKES_PER_PLAN {
        return Err(io::Error::new(
            ErrorKind::WouldBlock,
            "entity plan exceeds its wake bound",
        ));
    }
    for id in &canonical {
        if !neighbours.iter().any(|view| view.id == *id) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "entity plan wakes an entity outside its neighbour view",
            ));
        }
    }
    Ok(canonical)
}

/// Emits one canonical wake per destination through this producer's bounded
/// buffer and routes the complete set. Any overflow or routing violation
/// rejects the ENTIRE producer output (`WouldBlock`) so a truncated wake set
/// can never be delivered.
pub(super) fn route_wakes(
    registry: &EffectKindRegistryFrozen,
    tick: TickId,
    producer: SystemId,
    source: EntityId,
    wakes: &[EntityId],
) -> Result<Vec<EntityId>, io::Error> {
    let kind = wake_kind();
    let mut buffer = RegisteredEffectBuffer::new(
        tick,
        Phase::DurableActions,
        producer,
        OwnerKey::Entity(source.get()),
        MAX_WAKES_PER_PLAN,
        registry,
    )
    .map_err(|error| blocked(format!("wake buffer rejected: {error:?}")))?;
    for id in wakes {
        buffer
            .emit(&kind, EntityWake { id: *id })
            .map_err(|error| blocked(format!("wake emit rejected: {error:?}")))?;
    }
    let intents = buffer
        .finish()
        .map_err(|error| blocked(format!("wake buffer overflowed: {error:?}")))?;
    let batch = route_registered_effects(intents, RegisteredEffectLimits::default())
        .map_err(|error| blocked(format!("wake routing rejected: {error:?}")))?;
    // Owners and effects each arrive in stable order (owner, then the full
    // producer order key), so routed destinations are deterministic.
    let mut destinations = Vec::with_capacity(wakes.len());
    for owner in batch.owners() {
        for effect in &owner.effects {
            let wake = effect
                .intent()
                .payload::<EntityWake>()
                .ok_or_else(|| blocked("routed wake has an unexpected payload type"))?;
            destinations.push(wake.id());
        }
    }
    Ok(destinations)
}

fn blocked(reason: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::WouldBlock, reason.into())
}
