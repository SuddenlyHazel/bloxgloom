//! Bounded, typed cross-owner effects and multi-chunk structure footprints.
//!
//! Effects are only routed here. Every routed effect is delivered at the same
//! interaction/commit barrier, including effects whose destination is the
//! producer's own chunk. Applying an effect must not immediately run the
//! destination system again; newly scheduled simulation work starts next tick.

use super::simulation::{OrderKey, Phase, TickId};
use crate::world::{self, BlockId, CHUNK_SIZE, ChunkKey, world_to_chunk};
use std::collections::{HashMap, HashSet};

pub const MAX_EFFECTS_PER_PRODUCER_TICK: usize = 4_096;
pub const MAX_EFFECTS_PER_BATCH: usize = 16_384;
pub const MAX_EFFECTS_PER_OWNER: usize = 4_096;
// The server has no multi-chunk structures yet. These narrowly scoped
// allowances keep the transaction core available for that migration without
// masking unused effect-routing code elsewhere in this module.
#[allow(dead_code)]
pub const MAX_STRUCTURE_PLANS_PER_BATCH: usize = 256;
#[allow(dead_code)]
pub const MAX_PLACEMENTS_PER_STRUCTURE: usize = 1_024;
#[allow(dead_code)]
pub const MAX_PLACEMENTS_PER_BATCH: usize = 4_096;

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

#[allow(dead_code)]
pub type StructureId = u64;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPlacement {
    pub cell: CellCoord,
    pub block: BlockId,
}

/// Complete proposed footprint for one structure. Its anchor is also one of its placements.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructurePlan {
    pub id: StructureId,
    pub anchor: CellCoord,
    pub placements: Vec<BlockPlacement>,
}

#[allow(dead_code)]
impl StructurePlan {
    pub fn anchor_owner(&self) -> ChunkKey {
        self.anchor.owner()
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct StructureRecord {
    owner: ChunkKey,
    cells: Vec<CellCoord>,
    chunks: Vec<ChunkKey>,
}

/// Authoritative footprint index. Structure state is owned by its anchor chunk;
/// every chunk touched by the footprint indexes the same stable structure ID.
#[allow(dead_code)]
#[derive(Default)]
pub struct FootprintIndex {
    occupied: HashMap<CellCoord, StructureId>,
    structures: HashMap<StructureId, StructureRecord>,
    by_chunk: HashMap<ChunkKey, HashSet<StructureId>>,
}

#[allow(dead_code)]
impl FootprintIndex {
    /// Validates every plan and overlap before mutating any index, then claims all plans.
    ///
    /// Call this at the transaction barrier and apply the associated world writes
    /// as one operation only after it succeeds.
    pub fn claim_batch(&mut self, plans: &[StructurePlan]) -> Result<(), FootprintError> {
        if plans.len() > MAX_STRUCTURE_PLANS_PER_BATCH {
            return Err(FootprintError::TooManyPlans {
                limit: MAX_STRUCTURE_PLANS_PER_BATCH,
            });
        }

        let mut ordered: Vec<&StructurePlan> = plans.iter().collect();
        ordered.sort_by_key(|plan| plan.id);

        let mut previous_id = None;
        let mut total_placements = 0usize;
        for plan in &ordered {
            if plan.id == 0 {
                return Err(FootprintError::InvalidId);
            }
            if previous_id == Some(plan.id) {
                return Err(FootprintError::DuplicateStructureId { id: plan.id });
            }
            previous_id = Some(plan.id);
            if self.structures.contains_key(&plan.id) {
                return Err(FootprintError::AlreadyClaimed { id: plan.id });
            }
            if plan.placements.is_empty() {
                return Err(FootprintError::EmptyFootprint { id: plan.id });
            }
            if plan.placements.len() > MAX_PLACEMENTS_PER_STRUCTURE {
                return Err(FootprintError::TooManyPlacements {
                    id: plan.id,
                    limit: MAX_PLACEMENTS_PER_STRUCTURE,
                });
            }
            total_placements = total_placements.saturating_add(plan.placements.len());
            if total_placements > MAX_PLACEMENTS_PER_BATCH {
                return Err(FootprintError::BatchTooLarge {
                    limit: MAX_PLACEMENTS_PER_BATCH,
                });
            }

            let mut cells = HashSet::with_capacity(plan.placements.len());
            let mut includes_anchor = false;
            for placement in &plan.placements {
                if !world::valid_block(placement.block) {
                    return Err(FootprintError::InvalidBlock {
                        id: plan.id,
                        block: placement.block,
                    });
                }
                if !cells.insert(placement.cell) {
                    return Err(FootprintError::DuplicateCell {
                        id: plan.id,
                        cell: placement.cell,
                    });
                }
                includes_anchor |= placement.cell == plan.anchor;
            }
            if !includes_anchor {
                return Err(FootprintError::AnchorOutsideFootprint {
                    id: plan.id,
                    anchor: plan.anchor,
                });
            }
        }

        let mut batch_cells: HashMap<CellCoord, StructureId> = HashMap::new();
        for plan in &ordered {
            let mut placements = plan.placements.iter().collect::<Vec<_>>();
            placements.sort_by_key(|placement| placement.cell);
            for placement in placements {
                if let Some(first_structure) = self.occupied.get(&placement.cell) {
                    return Err(FootprintError::Conflict {
                        cell: placement.cell,
                        first_structure: *first_structure,
                        second_structure: plan.id,
                    });
                }
                if let Some(first_structure) = batch_cells.get(&placement.cell) {
                    return Err(FootprintError::Conflict {
                        cell: placement.cell,
                        first_structure: *first_structure,
                        second_structure: plan.id,
                    });
                }
                batch_cells.insert(placement.cell, plan.id);
            }
        }

        for plan in ordered {
            let mut cells = plan
                .placements
                .iter()
                .map(|placement| placement.cell)
                .collect::<Vec<_>>();
            cells.sort_unstable();
            let mut chunks = cells.iter().map(|cell| cell.owner()).collect::<Vec<_>>();
            chunks.sort_by_key(|owner| owner_order(*owner));
            chunks.dedup();

            for cell in &cells {
                self.occupied.insert(*cell, plan.id);
            }
            for owner in &chunks {
                self.by_chunk.entry(*owner).or_default().insert(plan.id);
            }
            self.structures.insert(
                plan.id,
                StructureRecord {
                    owner: plan.anchor_owner(),
                    cells,
                    chunks,
                },
            );
        }
        Ok(())
    }

    pub fn anchor_owner(&self, id: StructureId) -> Option<ChunkKey> {
        self.structures.get(&id).map(|record| record.owner)
    }

    pub fn structure_at(&self, cell: CellCoord) -> Option<StructureId> {
        self.occupied.get(&cell).copied()
    }

    /// Returns stable structure IDs indexed by this footprint chunk.
    pub fn structures_in_chunk(&self, owner: ChunkKey) -> Vec<StructureId> {
        let mut structures = self
            .by_chunk
            .get(&owner)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
            .collect::<Vec<_>>();
        structures.sort_unstable();
        structures
    }

    pub fn remove(&mut self, id: StructureId) -> bool {
        let Some(record) = self.structures.remove(&id) else {
            return false;
        };
        for cell in record.cells {
            self.occupied.remove(&cell);
        }
        for owner in record.chunks {
            if let Some(ids) = self.by_chunk.get_mut(&owner) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.by_chunk.remove(&owner);
                }
            }
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.structures.is_empty()
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootprintError {
    InvalidId,
    TooManyPlans {
        limit: usize,
    },
    DuplicateStructureId {
        id: StructureId,
    },
    AlreadyClaimed {
        id: StructureId,
    },
    EmptyFootprint {
        id: StructureId,
    },
    TooManyPlacements {
        id: StructureId,
        limit: usize,
    },
    BatchTooLarge {
        limit: usize,
    },
    InvalidBlock {
        id: StructureId,
        block: BlockId,
    },
    DuplicateCell {
        id: StructureId,
        cell: CellCoord,
    },
    AnchorOutsideFootprint {
        id: StructureId,
        anchor: CellCoord,
    },
    Conflict {
        cell: CellCoord,
        first_structure: StructureId,
        second_structure: StructureId,
    },
}

#[cfg(test)]
mod tests;
