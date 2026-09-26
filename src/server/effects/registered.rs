//! Typed extension effect registration and deterministic owner routing.
//!
//! Payloads remain typed in memory. Each kind declares a schema version and a
//! bounded encoded-size measure; save/WAL codecs remain owned by durable
//! domains. Routing validates the complete expanded delivery set before it
//! returns any destination batch.

use super::super::parallel::{OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
use super::super::registry::{IdentifierError, SystemId};
use super::super::simulation::{Phase, TickId};
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

pub const MAX_EFFECT_KIND_ID_BYTES: usize = 128;
pub const MAX_REGISTERED_EFFECT_KINDS: usize = 256;
pub const MAX_EFFECT_KIND_PAYLOAD_BYTES: usize = 64 * 1_024;
pub const MAX_EFFECT_DESTINATIONS: usize = 16_384;
pub const MAX_EFFECT_BUFFER_PAYLOAD_BYTES: usize = 16 * 1_024 * 1_024;
pub const MAX_EFFECT_BATCH_PAYLOAD_BYTES: usize = 64 * 1_024 * 1_024;

/// Namespaced identity for an executable effect kind.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EffectKindId(String);

impl EffectKindId {
    pub fn new(value: impl Into<String>) -> Result<Self, EffectRegistryError> {
        let value = value.into();
        if value.len() > MAX_EFFECT_KIND_ID_BYTES {
            return Err(EffectRegistryError::IdTooLong {
                actual: value.len(),
                maximum: MAX_EFFECT_KIND_ID_BYTES,
            });
        }
        SystemId::new(value.clone()).map_err(|error| match error {
            IdentifierError::InvalidFormat { .. } => EffectRegistryError::InvalidId(value.clone()),
            IdentifierError::TooLong { .. } => EffectRegistryError::IdTooLong {
                actual: value.len(),
                maximum: MAX_EFFECT_KIND_ID_BYTES,
            },
        })?;
        Ok(Self(value))
    }

    #[allow(
        dead_code,
        reason = "Extension-facing identity accessor, parallel to SystemId::as_str."
    )]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identity of one produced effect. The ordering is the plan's
/// `(tick, phase, system, source owner, producer sequence)` order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RegisteredEffectOrderKey {
    pub tick: TickId,
    pub phase: Phase,
    pub system: SystemId,
    pub source: OwnerKey,
    pub sequence: u64,
}

impl RegisteredEffectOrderKey {
    pub fn new(
        tick: TickId,
        phase: Phase,
        system: SystemId,
        source: OwnerKey,
        sequence: u64,
    ) -> Self {
        Self {
            tick,
            phase,
            system,
            source,
            sequence,
        }
    }
}

/// Startup declaration errors for typed effect kinds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EffectRegistryError {
    InvalidId(String),
    IdTooLong { actual: usize, maximum: usize },
    DuplicateKind { id: EffectKindId },
    TooManyKinds { maximum: usize },
    ZeroSchemaVersion { id: EffectKindId },
    InvalidPayloadLimit { id: EffectKindId, maximum: usize },
    InvalidDestinationLimit { id: EffectKindId, maximum: usize },
}

/// Runtime output errors while producing, validating, or routing an effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisteredEffectError {
    UnknownKind {
        id: EffectKindId,
    },
    WrongPayloadType {
        id: EffectKindId,
    },
    InvalidPayload {
        id: EffectKindId,
        reason: String,
    },
    PayloadTooLarge {
        id: EffectKindId,
        actual: usize,
        maximum: usize,
    },
    PayloadSizeChanged {
        id: EffectKindId,
        emitted: usize,
        routed: usize,
    },
    ProducerLimitTooLarge {
        requested: usize,
        maximum: usize,
    },
    ProducerOverflow {
        limit: usize,
    },
    ProducerPayloadOverflow {
        actual: usize,
        limit: usize,
    },
    InvalidDestinationCount {
        id: EffectKindId,
        actual: usize,
        maximum: usize,
    },
    DuplicateDestination {
        id: EffectKindId,
        owner: OwnerKey,
    },
    ConsumerOwnerMismatch {
        expected: OwnerKey,
        actual: OwnerKey,
    },
    ConsumerRejected {
        id: EffectKindId,
        reason: String,
    },
    ConsumerUsageOverflow,
}

impl fmt::Display for RegisteredEffectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

/// Typed scratch value and declared usage from one batched effect consumer.
pub struct EffectConsumerOutput {
    payload: Box<dyn Any + Send>,
    usage: PatchUsage,
}

impl EffectConsumerOutput {
    pub fn new<T: Any + Send>(payload: T, usage: PatchUsage) -> Self {
        Self {
            payload: Box::new(payload),
            usage,
        }
    }

    fn into_parts(self) -> (Box<dyn Any + Send>, PatchUsage) {
        (self.payload, self.usage)
    }
}

/// Output from one registered kind within an owner effect-consumer batch.
pub struct EffectConsumerScratch {
    kind: EffectKindId,
    payload: Box<dyn Any + Send>,
}

impl EffectConsumerScratch {
    #[allow(
        dead_code,
        reason = "Extensions inspecting batched consumer output need its kind identity."
    )]
    pub fn kind(&self) -> &EffectKindId {
        &self.kind
    }

    pub fn payload<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }
}

impl fmt::Debug for EffectConsumerScratch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EffectConsumerScratch")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// Type-erased collection of stable per-kind outputs from one owner batch.
#[derive(Default)]
pub struct EffectConsumerBatch {
    outputs: Vec<EffectConsumerScratch>,
}

impl EffectConsumerBatch {
    #[allow(
        dead_code,
        reason = "Extensions can inspect all typed outputs of a batched consumer."
    )]
    pub fn outputs(&self) -> &[EffectConsumerScratch] {
        &self.outputs
    }

    #[allow(
        dead_code,
        reason = "Typed consumer output access remains available to registered extensions."
    )]
    pub fn output<T: Any>(&self, kind: &EffectKindId) -> Option<&T> {
        self.outputs
            .iter()
            .find(|output| &output.kind == kind)?
            .payload()
    }
}

impl fmt::Debug for EffectConsumerBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EffectConsumerBatch")
            .field("outputs", &self.outputs)
            .finish()
    }
}

trait ErasedEffectKind: Send + Sync {
    fn schema_version(&self) -> u16;
    fn max_payload_bytes(&self) -> usize;
    fn max_destinations(&self) -> usize;
    fn validate_payload(
        &self,
        payload: &(dyn Any + Send + Sync),
    ) -> Result<usize, RegisteredEffectError>;
    fn destinations(
        &self,
        payload: &(dyn Any + Send + Sync),
    ) -> Result<Vec<OwnerKey>, RegisteredEffectError>;
    fn prepare_consumer(
        &self,
        job: &OwnerJob,
        payloads: &[Arc<dyn Any + Send + Sync>],
    ) -> Result<EffectConsumerOutput, RegisteredEffectError>;
}

struct TypedEffectKind<P, M, V, D, C> {
    id: EffectKindId,
    schema_version: u16,
    max_payload_bytes: usize,
    max_destinations: usize,
    measure: M,
    validate: V,
    destinations: D,
    consumer: C,
    marker: std::marker::PhantomData<fn() -> P>,
}

impl<P, M, V, D, C> ErasedEffectKind for TypedEffectKind<P, M, V, D, C>
where
    P: Any + Send + Sync,
    M: Fn(&P) -> usize + Send + Sync,
    V: Fn(&P) -> Result<(), String> + Send + Sync,
    D: Fn(&P) -> Result<Vec<OwnerKey>, String> + Send + Sync,
    C: Fn(&OwnerJob, &[&P]) -> Result<EffectConsumerOutput, String> + Send + Sync,
{
    fn schema_version(&self) -> u16 {
        self.schema_version
    }

    fn max_payload_bytes(&self) -> usize {
        self.max_payload_bytes
    }

    fn max_destinations(&self) -> usize {
        self.max_destinations
    }

    fn validate_payload(
        &self,
        payload: &(dyn Any + Send + Sync),
    ) -> Result<usize, RegisteredEffectError> {
        let payload =
            payload
                .downcast_ref::<P>()
                .ok_or_else(|| RegisteredEffectError::WrongPayloadType {
                    id: self.id.clone(),
                })?;
        (self.validate)(payload).map_err(|reason| RegisteredEffectError::InvalidPayload {
            id: self.id.clone(),
            reason,
        })?;
        let actual = (self.measure)(payload);
        if actual > self.max_payload_bytes {
            return Err(RegisteredEffectError::PayloadTooLarge {
                id: self.id.clone(),
                actual,
                maximum: self.max_payload_bytes,
            });
        }
        Ok(actual)
    }

    fn destinations(
        &self,
        payload: &(dyn Any + Send + Sync),
    ) -> Result<Vec<OwnerKey>, RegisteredEffectError> {
        let payload =
            payload
                .downcast_ref::<P>()
                .ok_or_else(|| RegisteredEffectError::WrongPayloadType {
                    id: self.id.clone(),
                })?;
        let owners = (self.destinations)(payload).map_err(|reason| {
            RegisteredEffectError::InvalidPayload {
                id: self.id.clone(),
                reason,
            }
        })?;
        if owners.is_empty() || owners.len() > self.max_destinations {
            return Err(RegisteredEffectError::InvalidDestinationCount {
                id: self.id.clone(),
                actual: owners.len(),
                maximum: self.max_destinations,
            });
        }
        let mut seen = BTreeSet::new();
        for owner in &owners {
            if !seen.insert(*owner) {
                return Err(RegisteredEffectError::DuplicateDestination {
                    id: self.id.clone(),
                    owner: *owner,
                });
            }
        }
        Ok(owners)
    }

    fn prepare_consumer(
        &self,
        job: &OwnerJob,
        payloads: &[Arc<dyn Any + Send + Sync>],
    ) -> Result<EffectConsumerOutput, RegisteredEffectError> {
        let typed_payloads = payloads
            .iter()
            .map(|payload| {
                payload
                    .downcast_ref::<P>()
                    .ok_or_else(|| RegisteredEffectError::WrongPayloadType {
                        id: self.id.clone(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        (self.consumer)(job, &typed_payloads).map_err(|reason| {
            RegisteredEffectError::ConsumerRejected {
                id: self.id.clone(),
                reason,
            }
        })
    }
}

/// Mutable startup registry for effect kinds. Every kind has a typed payload
/// validator, destination rule, and owner-local consumer.
#[derive(Default)]
pub struct EffectKindRegistry {
    kinds: BTreeMap<EffectKindId, Arc<dyn ErasedEffectKind>>,
}

impl EffectKindRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register<P, M, V, D, C>(
        &mut self,
        id: EffectKindId,
        schema_version: u16,
        max_payload_bytes: usize,
        max_destinations: usize,
        measure: M,
        validate: V,
        destinations: D,
        consumer: C,
    ) -> Result<(), EffectRegistryError>
    where
        P: Any + Send + Sync,
        M: Fn(&P) -> usize + Send + Sync + 'static,
        V: Fn(&P) -> Result<(), String> + Send + Sync + 'static,
        D: Fn(&P) -> Result<Vec<OwnerKey>, String> + Send + Sync + 'static,
        C: Fn(&OwnerJob, &[&P]) -> Result<EffectConsumerOutput, String> + Send + Sync + 'static,
    {
        if self.kinds.contains_key(&id) {
            return Err(EffectRegistryError::DuplicateKind { id });
        }
        if self.kinds.len() >= MAX_REGISTERED_EFFECT_KINDS {
            return Err(EffectRegistryError::TooManyKinds {
                maximum: MAX_REGISTERED_EFFECT_KINDS,
            });
        }
        if schema_version == 0 {
            return Err(EffectRegistryError::ZeroSchemaVersion { id });
        }
        if max_payload_bytes == 0 || max_payload_bytes > MAX_EFFECT_KIND_PAYLOAD_BYTES {
            return Err(EffectRegistryError::InvalidPayloadLimit {
                id,
                maximum: MAX_EFFECT_KIND_PAYLOAD_BYTES,
            });
        }
        if max_destinations == 0 || max_destinations > MAX_EFFECT_DESTINATIONS {
            return Err(EffectRegistryError::InvalidDestinationLimit {
                id,
                maximum: MAX_EFFECT_DESTINATIONS,
            });
        }
        let definition = TypedEffectKind::<P, M, V, D, C> {
            id: id.clone(),
            schema_version,
            max_payload_bytes,
            max_destinations,
            measure,
            validate,
            destinations,
            consumer,
            marker: std::marker::PhantomData,
        };
        self.kinds.insert(id, Arc::new(definition));
        Ok(())
    }

    pub fn freeze(self) -> EffectKindRegistryFrozen {
        EffectKindRegistryFrozen { kinds: self.kinds }
    }
}

/// Immutable namespaced effect registry installed before gameplay starts.
#[derive(Clone, Default)]
pub struct EffectKindRegistryFrozen {
    kinds: BTreeMap<EffectKindId, Arc<dyn ErasedEffectKind>>,
}

impl EffectKindRegistryFrozen {
    #[cfg(test)]
    pub fn kind_count(&self) -> usize {
        self.kinds.len()
    }

    #[allow(
        dead_code,
        reason = "Extensions can inspect frozen effect schema declarations."
    )]
    pub fn schema_version(&self, id: &EffectKindId) -> Option<u16> {
        self.kinds.get(id).map(|kind| kind.schema_version())
    }

    #[allow(
        dead_code,
        reason = "Extensions can inspect frozen effect payload bounds."
    )]
    pub fn max_payload_bytes(&self, id: &EffectKindId) -> Option<usize> {
        self.kinds.get(id).map(|kind| kind.max_payload_bytes())
    }

    #[allow(
        dead_code,
        reason = "Extensions can inspect frozen effect fan-out bounds."
    )]
    pub fn max_destinations(&self, id: &EffectKindId) -> Option<usize> {
        self.kinds.get(id).map(|kind| kind.max_destinations())
    }

    #[allow(
        dead_code,
        reason = "Extensions can inspect the frozen effect vocabulary."
    )]
    pub fn registered_kinds(&self) -> impl Iterator<Item = &EffectKindId> {
        self.kinds.keys()
    }

    fn get(&self, id: &EffectKindId) -> Result<Arc<dyn ErasedEffectKind>, RegisteredEffectError> {
        self.kinds
            .get(id)
            .cloned()
            .ok_or_else(|| RegisteredEffectError::UnknownKind { id: id.clone() })
    }

    /// Type-erased admission for intents whose concrete payload type is only
    /// known to the producer. Validation is identical to the typed path: the
    /// kind must be registered and its payload validator and size measure
    /// must accept the value.
    fn intent_erased(
        &self,
        key: RegisteredEffectOrderKey,
        id: &EffectKindId,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<RegisteredEffectIntent, RegisteredEffectError> {
        let kind = self.get(id)?;
        let payload_bytes = kind.validate_payload(payload.as_ref())?;
        Ok(RegisteredEffectIntent {
            key,
            kind_id: id.clone(),
            kind,
            payload,
            payload_bytes,
        })
    }
}

/// Fixed-capacity effect output for one `(tick, phase, system, source owner)`.
pub struct RegisteredEffectBuffer<'a> {
    tick: TickId,
    phase: Phase,
    system: SystemId,
    source: OwnerKey,
    limit: usize,
    registry: &'a EffectKindRegistryFrozen,
    effects: Vec<RegisteredEffectIntent>,
    overflowed: bool,
    payload_bytes: usize,
}

impl<'a> RegisteredEffectBuffer<'a> {
    pub fn new(
        tick: TickId,
        phase: Phase,
        system: SystemId,
        source: OwnerKey,
        limit: usize,
        registry: &'a EffectKindRegistryFrozen,
    ) -> Result<Self, RegisteredEffectError> {
        if limit > super::MAX_EFFECTS_PER_PRODUCER_TICK {
            return Err(RegisteredEffectError::ProducerLimitTooLarge {
                requested: limit,
                maximum: super::MAX_EFFECTS_PER_PRODUCER_TICK,
            });
        }
        Ok(Self {
            tick,
            phase,
            system,
            source,
            limit,
            registry,
            effects: Vec::with_capacity(limit),
            overflowed: false,
            payload_bytes: 0,
        })
    }

    pub fn emit<P: Any + Send + Sync>(
        &mut self,
        id: &EffectKindId,
        payload: P,
    ) -> Result<(), RegisteredEffectError> {
        self.emit_erased(id, Arc::new(payload))
    }

    /// Type-erased emission for producers that only know their payload as a
    /// shared trait object. Bounds, sequencing, and validation match `emit`
    /// exactly; a failed validation rejects that emission without poisoning
    /// the buffer, while any overflow poisons the whole producer output.
    /// (Effective visibility is capped by the `pub(super)` re-export in
    /// `effects.rs`, like every other buffer method.)
    pub fn emit_erased(
        &mut self,
        id: &EffectKindId,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), RegisteredEffectError> {
        if self.overflowed || self.effects.len() >= self.limit {
            self.overflowed = true;
            return Err(RegisteredEffectError::ProducerOverflow { limit: self.limit });
        }
        let sequence = self.effects.len() as u64;
        let key = RegisteredEffectOrderKey::new(
            self.tick,
            self.phase,
            self.system.clone(),
            self.source,
            sequence,
        );
        let effect = self.registry.intent_erased(key, id, payload)?;
        let next_payload_bytes = self.payload_bytes.saturating_add(effect.payload_bytes);
        if next_payload_bytes > MAX_EFFECT_BUFFER_PAYLOAD_BYTES {
            self.overflowed = true;
            return Err(RegisteredEffectError::ProducerPayloadOverflow {
                actual: next_payload_bytes,
                limit: MAX_EFFECT_BUFFER_PAYLOAD_BYTES,
            });
        }
        self.payload_bytes = next_payload_bytes;
        self.effects.push(effect);
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<RegisteredEffectIntent>, RegisteredEffectError> {
        if self.overflowed {
            Err(RegisteredEffectError::ProducerOverflow { limit: self.limit })
        } else {
            Ok(self.effects)
        }
    }
}

/// Initial effect admission ceilings, applied after destination expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisteredEffectLimits {
    pub total_deliveries: usize,
    pub per_destination: usize,
    pub payload_bytes: usize,
}

impl Default for RegisteredEffectLimits {
    fn default() -> Self {
        Self {
            total_deliveries: super::MAX_EFFECTS_PER_BATCH,
            per_destination: super::MAX_EFFECTS_PER_OWNER,
            payload_bytes: MAX_EFFECT_BATCH_PAYLOAD_BYTES,
        }
    }
}

pub struct RegisteredEffectIntent {
    key: RegisteredEffectOrderKey,
    kind_id: EffectKindId,
    kind: Arc<dyn ErasedEffectKind>,
    payload: Arc<dyn Any + Send + Sync>,
    payload_bytes: usize,
}

impl RegisteredEffectIntent {
    #[cfg(test)]
    pub fn key(&self) -> &RegisteredEffectOrderKey {
        &self.key
    }

    #[allow(
        dead_code,
        reason = "Extensions inspecting routed intents need their registered kind."
    )]
    pub fn kind_id(&self) -> &EffectKindId {
        &self.kind_id
    }

    #[cfg(test)]
    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }

    pub fn payload<P: Any + Send + Sync>(&self) -> Option<&P> {
        self.payload.downcast_ref()
    }
}

impl Clone for RegisteredEffectIntent {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            kind_id: self.kind_id.clone(),
            kind: Arc::clone(&self.kind),
            payload: Arc::clone(&self.payload),
            payload_bytes: self.payload_bytes,
        }
    }
}

impl fmt::Debug for RegisteredEffectIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisteredEffectIntent")
            .field("key", &self.key)
            .field("kind_id", &self.kind_id)
            .field("payload_bytes", &self.payload_bytes)
            .finish_non_exhaustive()
    }
}

/// One typed effect delivered to an authoritative owner.
#[derive(Clone, Debug)]
pub struct RoutedEffectIntent {
    destination: OwnerKey,
    intent: RegisteredEffectIntent,
}

impl RoutedEffectIntent {
    #[allow(
        dead_code,
        reason = "Extension routing inspection exposes the authoritative destination."
    )]
    pub const fn destination(&self) -> OwnerKey {
        self.destination
    }

    pub fn intent(&self) -> &RegisteredEffectIntent {
        &self.intent
    }
}

#[derive(Clone, Debug)]
pub struct RoutedOwnerEffects {
    pub owner: OwnerKey,
    pub effects: Vec<RoutedEffectIntent>,
}

impl RoutedOwnerEffects {
    /// Runs each kind's consumer once for this destination owner, in stable
    /// kind order, and combines the outputs into one owner patch. Consumers
    /// therefore batch routed deliveries instead of being dispatched once per
    /// individual event or voxel.
    pub fn prepare_consumer(&self, job: &OwnerJob) -> Result<OwnerPatch, RegisteredEffectError> {
        if job.owner() != self.owner {
            return Err(RegisteredEffectError::ConsumerOwnerMismatch {
                expected: self.owner,
                actual: job.owner(),
            });
        }

        let mut grouped = BTreeMap::<
            EffectKindId,
            (Arc<dyn ErasedEffectKind>, Vec<Arc<dyn Any + Send + Sync>>),
        >::new();
        for effect in &self.effects {
            if effect.destination != self.owner {
                return Err(RegisteredEffectError::ConsumerOwnerMismatch {
                    expected: self.owner,
                    actual: effect.destination,
                });
            }
            let group = grouped
                .entry(effect.intent.kind_id.clone())
                .or_insert_with(|| (Arc::clone(&effect.intent.kind), Vec::new()));
            group.1.push(Arc::clone(&effect.intent.payload));
        }

        let mut outputs = Vec::with_capacity(grouped.len());
        let mut usage = PatchUsage::default();
        for (kind_id, (kind, payloads)) in grouped {
            let (payload, item_usage) = kind.prepare_consumer(job, &payloads)?.into_parts();
            usage.writes = usage
                .writes
                .checked_add(item_usage.writes)
                .ok_or(RegisteredEffectError::ConsumerUsageOverflow)?;
            usage.effects = usage
                .effects
                .checked_add(item_usage.effects)
                .ok_or(RegisteredEffectError::ConsumerUsageOverflow)?;
            usage.estimated_bytes = usage
                .estimated_bytes
                .checked_add(item_usage.estimated_bytes)
                .ok_or(RegisteredEffectError::ConsumerUsageOverflow)?;
            outputs.push(EffectConsumerScratch {
                kind: kind_id,
                payload,
            });
        }

        Ok(OwnerPatch::new(job, EffectConsumerBatch { outputs }, usage))
    }
}

#[derive(Clone, Debug)]
pub struct RoutedEffectBatch {
    owners: Vec<RoutedOwnerEffects>,
    #[cfg(test)]
    deliveries: usize,
}

impl RoutedEffectBatch {
    pub fn owners(&self) -> &[RoutedOwnerEffects] {
        &self.owners
    }

    #[cfg(test)]
    pub const fn delivery_count(&self) -> usize {
        self.deliveries
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteRegisteredError {
    LimitTooLarge { requested: usize, maximum: usize },
    MixedTick { first: u64, other: u64 },
    MixedPhase { first: Phase, other: Phase },
    DuplicateOrderKey { owner: OwnerKey },
    TotalOverflow { limit: usize },
    OwnerOverflow { owner: OwnerKey, limit: usize },
    PayloadOverflow { actual: usize, limit: usize },
    InvalidEffect(RegisteredEffectError),
}

/// Partitions typed effects by destination, sorts each owner by the full stable
/// producer key, and validates every fan-out before returning a batch.
pub fn route_registered_effects(
    effects: impl IntoIterator<Item = RegisteredEffectIntent>,
    limits: RegisteredEffectLimits,
) -> Result<RoutedEffectBatch, RouteRegisteredError> {
    if limits.total_deliveries > super::MAX_EFFECTS_PER_BATCH {
        return Err(RouteRegisteredError::LimitTooLarge {
            requested: limits.total_deliveries,
            maximum: super::MAX_EFFECTS_PER_BATCH,
        });
    }
    if limits.per_destination > super::MAX_EFFECTS_PER_OWNER {
        return Err(RouteRegisteredError::LimitTooLarge {
            requested: limits.per_destination,
            maximum: super::MAX_EFFECTS_PER_OWNER,
        });
    }
    if limits.payload_bytes > MAX_EFFECT_BATCH_PAYLOAD_BYTES {
        return Err(RouteRegisteredError::LimitTooLarge {
            requested: limits.payload_bytes,
            maximum: MAX_EFFECT_BATCH_PAYLOAD_BYTES,
        });
    }

    let mut routed = Vec::new();
    let mut origin: Option<(TickId, Phase)> = None;
    let mut payload_bytes = 0usize;
    for effect in effects {
        payload_bytes = payload_bytes.saturating_add(effect.payload_bytes);
        if payload_bytes > limits.payload_bytes {
            return Err(RouteRegisteredError::PayloadOverflow {
                actual: payload_bytes,
                limit: limits.payload_bytes,
            });
        }
        let current = (effect.key.tick, effect.key.phase);
        if let Some((first_tick, first_phase)) = origin {
            if current.0 != first_tick {
                return Err(RouteRegisteredError::MixedTick {
                    first: first_tick.get(),
                    other: current.0.get(),
                });
            }
            if current.1 != first_phase {
                return Err(RouteRegisteredError::MixedPhase {
                    first: first_phase,
                    other: current.1,
                });
            }
        } else {
            origin = Some(current);
        }

        let owners = effect
            .kind
            .destinations(effect.payload.as_ref())
            .map_err(RouteRegisteredError::InvalidEffect)?;
        let routed_payload_bytes = effect
            .kind
            .validate_payload(effect.payload.as_ref())
            .map_err(RouteRegisteredError::InvalidEffect)?;
        if routed_payload_bytes != effect.payload_bytes {
            return Err(RouteRegisteredError::InvalidEffect(
                RegisteredEffectError::PayloadSizeChanged {
                    id: effect.kind_id,
                    emitted: effect.payload_bytes,
                    routed: routed_payload_bytes,
                },
            ));
        }
        if routed.len().saturating_add(owners.len()) > limits.total_deliveries {
            return Err(RouteRegisteredError::TotalOverflow {
                limit: limits.total_deliveries,
            });
        }
        routed.extend(owners.into_iter().map(|owner| {
            (
                owner,
                RoutedEffectIntent {
                    destination: owner,
                    intent: effect.clone(),
                },
            )
        }));
    }

    routed.sort_by(|(owner_a, effect_a), (owner_b, effect_b)| {
        owner_a
            .cmp(owner_b)
            .then_with(|| effect_a.intent.key.cmp(&effect_b.intent.key))
    });
    let mut groups = Vec::new();
    let mut previous = None;
    #[cfg(test)]
    let deliveries = routed.len();
    for (owner, effect) in routed {
        if previous == Some((owner, effect.intent.key.clone())) {
            return Err(RouteRegisteredError::DuplicateOrderKey { owner });
        }
        previous = Some((owner, effect.intent.key.clone()));
        if groups
            .last()
            .is_none_or(|group: &RoutedOwnerEffects| group.owner != owner)
        {
            groups.push(RoutedOwnerEffects {
                owner,
                effects: Vec::new(),
            });
        }
        let owner_effects = groups.last_mut().expect("owner group was just created");
        owner_effects.effects.push(effect);
        if owner_effects.effects.len() > limits.per_destination {
            return Err(RouteRegisteredError::OwnerOverflow {
                owner,
                limit: limits.per_destination,
            });
        }
    }

    Ok(RoutedEffectBatch {
        owners: groups,
        #[cfg(test)]
        deliveries,
    })
}

#[cfg(test)]
#[path = "registered/tests.rs"]
mod tests;
