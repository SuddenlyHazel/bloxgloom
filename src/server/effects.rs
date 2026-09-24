//! Bounded, typed cross-owner effects.
//!
//! Effects are only routed here. Every routed effect is delivered at the same
//! interaction/commit barrier, including effects whose destination is the
//! producer's own chunk. Applying an effect must not immediately run the
//! destination system again; newly scheduled simulation work starts next tick.

use super::simulation::{OrderKey, Phase, TickId};
use crate::world::{CHUNK_SIZE, ChunkKey, world_to_chunk};
use std::collections::HashMap;

mod registered;
#[allow(unused_imports)]
pub(super) use registered::{
    EffectConsumerBatch, EffectConsumerOutput, EffectConsumerScratch, EffectKindId,
    EffectKindRegistry, EffectKindRegistryFrozen, EffectRegistryError, RegisteredEffectBuffer,
    RegisteredEffectError, RegisteredEffectIntent, RegisteredEffectLimits,
    RegisteredEffectOrderKey, RouteRegisteredError, RoutedEffectBatch, RoutedEffectIntent,
    RoutedOwnerEffects, route_registered_effects,
};

pub const MAX_EFFECTS_PER_PRODUCER_TICK: usize = 4_096;
pub const MAX_EFFECTS_PER_BATCH: usize = 16_384;
pub const MAX_EFFECTS_PER_OWNER: usize = 4_096;

/// World-space integer block coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl CellCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn owner(self) -> ChunkKey {
        world_to_chunk(self.x, self.y, self.z).0
    }
}

/// One simulation request addressed to authoritative state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    // `WakeDrop` was kept as an explicit target type for the live drop-owner
    // routing requested by the server integration. Current edits emit the
    // broader `BlockChanged` notification until drop ownership is chunk-indexed.
    #[allow(dead_code)]
    WakeDrop { id: u64, owner: ChunkKey },
    /// Notify drop owners whose radius-0.18 body could overlap this changed block.
    BlockChanged { cell: CellCoord },
}

impl Effect {
    fn destinations(self) -> Vec<ChunkKey> {
        match self {
            Self::BlockChanged { cell } => block_change_owners(cell),
            Self::WakeDrop { owner, .. } => vec![owner],
        }
    }
}

/// An emitted effect with its stable producer ordering key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectEnvelope {
    pub key: OrderKey,
    pub effect: Effect,
}

/// Fixed-capacity output buffer owned by one producer for one tick.
///
/// The producer ID must be stable across worker scheduling orders. Sequence
/// numbers are assigned in emission order, so producers should enumerate their
/// candidates deterministically.
pub struct EffectBuffer {
    tick: TickId,
    source: u64,
    limit: usize,
    effects: Vec<EffectEnvelope>,
    overflowed: bool,
}

impl EffectBuffer {
    pub fn new(tick: TickId, source: u64, limit: usize) -> Result<Self, EffectBufferError> {
        if limit > MAX_EFFECTS_PER_PRODUCER_TICK {
            return Err(EffectBufferError::LimitTooLarge {
                requested: limit,
                maximum: MAX_EFFECTS_PER_PRODUCER_TICK,
            });
        }
        Ok(Self {
            tick,
            source,
            limit,
            effects: Vec::with_capacity(limit),
            overflowed: false,
        })
    }

    /// Returns an error and poisons the whole buffer when its bound is reached.
    /// `finish` then rejects all buffered effects, so callers cannot accidentally
    /// route a truncated producer result.
    pub fn emit(&mut self, effect: Effect) -> Result<(), EffectBufferError> {
        if self.overflowed || self.effects.len() >= self.limit {
            self.overflowed = true;
            return Err(EffectBufferError::Overflow { limit: self.limit });
        }
        let sequence = self.effects.len() as u64;
        self.effects.push(EffectEnvelope {
            key: OrderKey::new(self.tick, self.source, sequence),
            effect,
        });
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<EffectEnvelope>, EffectBufferError> {
        if self.overflowed {
            Err(EffectBufferError::Overflow { limit: self.limit })
        } else {
            Ok(self.effects)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectBufferError {
    LimitTooLarge { requested: usize, maximum: usize },
    Overflow { limit: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectLimits {
    /// Maximum number of destination deliveries after effects such as
    /// `BlockChanged` have been expanded to neighboring owners.
    pub total: usize,
    pub per_owner: usize,
}

impl Default for EffectLimits {
    fn default() -> Self {
        Self {
            total: MAX_EFFECTS_PER_BATCH,
            per_owner: MAX_EFFECTS_PER_OWNER,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoutedEffect {
    pub key: OrderKey,
    pub effect: Effect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerEffects {
    pub owner: ChunkKey,
    /// Stable `OrderKey` order, independent of input iteration and worker order.
    pub effects: Vec<RoutedEffect>,
}

/// Successful output of routing. Owners and effects are each deterministically ordered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectBatch {
    owners: Vec<OwnerEffects>,
}

impl EffectBatch {
    /// All effects, local or remote, commit in this same phase.
    pub const fn commit_phase(&self) -> Phase {
        Phase::InteractionCommit
    }

    pub fn owners(&self) -> &[OwnerEffects] {
        &self.owners
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteError {
    LimitTooLarge { requested: usize, maximum: usize },
    TotalOverflow { limit: usize },
    OwnerOverflow { owner: ChunkKey, limit: usize },
    MixedTicks { first: u64, other: u64 },
    DuplicateOrderKey { owner: ChunkKey },
}

/// Groups effects by their authoritative destination without applying any of them.
///
/// This function has all-or-error behavior: on any capacity, tick, or ordering
/// violation it returns no batch for partial application. Sorting by destination
/// and `OrderKey` makes local and cross-chunk deliveries obey identical ordering.
pub fn route_effects(
    effects: impl IntoIterator<Item = EffectEnvelope>,
    limits: EffectLimits,
) -> Result<EffectBatch, RouteError> {
    if limits.total > MAX_EFFECTS_PER_BATCH {
        return Err(RouteError::LimitTooLarge {
            requested: limits.total,
            maximum: MAX_EFFECTS_PER_BATCH,
        });
    }
    if limits.per_owner > MAX_EFFECTS_PER_OWNER {
        return Err(RouteError::LimitTooLarge {
            requested: limits.per_owner,
            maximum: MAX_EFFECTS_PER_OWNER,
        });
    }

    let mut routed = Vec::new();
    let mut origin_tick = None;

    for envelope in effects {
        let tick = envelope.key.tick().get();
        if let Some(first) = origin_tick {
            if first != tick {
                return Err(RouteError::MixedTicks { first, other: tick });
            }
        } else {
            origin_tick = Some(tick);
        }

        for owner in envelope.effect.destinations() {
            if routed.len() >= limits.total {
                return Err(RouteError::TotalOverflow {
                    limit: limits.total,
                });
            }
            routed.push((owner, envelope));
        }
    }

    routed.sort_by(|(owner_a, effect_a), (owner_b, effect_b)| {
        owner_order(*owner_a)
            .cmp(&owner_order(*owner_b))
            .then_with(|| effect_a.key.cmp(&effect_b.key))
    });

    let mut counts: HashMap<ChunkKey, usize> = HashMap::new();
    let mut owners: Vec<OwnerEffects> = Vec::new();
    let mut previous: Option<(ChunkKey, OrderKey)> = None;
    for (owner, envelope) in routed {
        if previous.is_some_and(|(previous_owner, previous_key)| {
            previous_owner == owner && previous_key == envelope.key
        }) {
            return Err(RouteError::DuplicateOrderKey { owner });
        }
        previous = Some((owner, envelope.key));

        let count = counts.entry(owner).or_default();
        *count += 1;
        if *count > limits.per_owner {
            return Err(RouteError::OwnerOverflow {
                owner,
                limit: limits.per_owner,
            });
        }

        if owners.last().is_none_or(|group| group.owner != owner) {
            owners.push(OwnerEffects {
                owner,
                effects: Vec::new(),
            });
        }
        owners.last_mut().unwrap().effects.push(RoutedEffect {
            key: envelope.key,
            effect: envelope.effect,
        });
    }

    Ok(EffectBatch { owners })
}

fn owner_order(owner: ChunkKey) -> (i32, i32, i32) {
    (owner.x, owner.y, owner.z)
}

/// Owners whose drop centers can be within one drop radius of the changed cell.
///
/// This is a conservative broad phase: it includes every face/edge/corner owner
/// touching a chunk seam, then the destination owner checks the actual drop body.
pub fn block_change_owners(cell: CellCoord) -> Vec<ChunkKey> {
    let xs = boundary_coordinates(cell.x);
    let ys = boundary_coordinates(cell.y);
    let zs = boundary_coordinates(cell.z);
    let mut owners = Vec::with_capacity(8);
    owners.push(cell.owner());
    for &x in &xs {
        for &y in &ys {
            for &z in &zs {
                let owner = world_to_chunk(x, y, z).0;
                if !owners.contains(&owner) {
                    owners.push(owner);
                }
            }
        }
    }
    owners.sort_by_key(|owner| owner_order(*owner));
    owners
}

fn boundary_coordinates(coordinate: i32) -> Vec<i32> {
    let mut coordinates = vec![coordinate];
    let local = coordinate.rem_euclid(CHUNK_SIZE as i32);
    let adjacent = if local == 0 {
        coordinate.checked_sub(1)
    } else if local == CHUNK_SIZE as i32 - 1 {
        coordinate.checked_add(1)
    } else {
        None
    };
    if let Some(adjacent) = adjacent {
        coordinates.push(adjacent);
    }
    coordinates
}

#[cfg(test)]
mod tests;
