use crate::content::{BlockStateId, EntityTypeId};
use crate::world::ChunkKey;
use std::any::Any;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

pub const MAX_ENTITY_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_ENTITY_PUBLIC_VIEW_BYTES: usize = 4 * 1024;
pub const MAX_ENTITY_FOOTPRINT_CELLS: usize = 4_096;
pub const MAX_ENTITY_PRIVATE_BYTES_PER_CHUNK: usize = 2 * 1024 * 1024;
pub const MAX_ENTITY_PUBLIC_BYTES_PER_CHUNK: usize = 1024 * 1024;
pub const MAX_ENTITY_REFERENCES_PER_CHUNK: usize = 65_536;

/// Immutable type-erased payload kept decoded in the live store. Feature
/// modules recover their concrete type with `downcast_ref`.
#[derive(Clone)]
pub struct EntityPayload(Arc<dyn Any + Send + Sync>);

impl EntityPayload {
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Arc::new(value))
    }

    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }

    pub(super) fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for EntityPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EntityPayload(<decoded>)")
    }
}

/// Persisted, monotonic identity. Zero is reserved and IDs are never reused.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(u64);

impl EntityId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Integer world cell used by anchors and bounded structure footprints.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl CellCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn chunk(self) -> ChunkKey {
        ChunkKey {
            x: self.x.div_euclid(16),
            y: self.y.div_euclid(16),
            z: self.z.div_euclid(16),
        }
    }
}

/// Declares which owner model a registered type uses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityOwnership {
    /// A point-like actor or drop with one position and one current chunk owner.
    Mobile,
    /// A structure stored at its anchor and referenced by each touched chunk.
    Anchored {
        compatible_anchor_states: BTreeSet<BlockStateId>,
        max_footprint_cells: usize,
    },
}

impl EntityOwnership {
    pub fn anchored(
        compatible_anchor_states: impl IntoIterator<Item = BlockStateId>,
        max_footprint_cells: usize,
    ) -> Self {
        Self::Anchored {
            compatible_anchor_states: compatible_anchor_states.into_iter().collect(),
            max_footprint_cells,
        }
    }
}

/// Declarative schedule cadence. The next due tick itself is persisted per entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickPolicy {
    Never,
    EveryTick,
    Interval(u32),
}

impl TickPolicy {
    pub fn first_tick(self, spawn_tick: u64) -> Result<Option<u64>, EntityError> {
        match self {
            Self::Never => Ok(None),
            Self::EveryTick => spawn_tick
                .checked_add(1)
                .map(Some)
                .ok_or(EntityError::RevisionExhausted),
            Self::Interval(0) => Err(EntityError::InvalidType),
            Self::Interval(ticks) => spawn_tick
                .checked_add(u64::from(ticks))
                .map(Some)
                .ok_or(EntityError::RevisionExhausted),
        }
    }

    pub fn validates(self, next_tick: Option<u64>) -> bool {
        match (self, next_tick) {
            (Self::Never, None) => true,
            (Self::EveryTick | Self::Interval(_), Some(_)) => true,
            _ => false,
        }
    }
}

/// Validated canonical location stored with an entity record.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityLocation {
    Mobile {
        position: [f32; 3],
    },
    Anchored {
        anchor: CellCoord,
        anchor_state: BlockStateId,
        footprint: Vec<CellCoord>,
    },
}

impl EntityLocation {
    pub fn owner(&self) -> Result<EntityOwner, EntityError> {
        match self {
            Self::Mobile { position } => {
                let cell = position_to_cell(*position)?;
                Ok(EntityOwner::Mobile(cell.chunk()))
            }
            Self::Anchored { anchor, .. } => Ok(EntityOwner::Anchored(anchor.chunk())),
        }
    }

    pub fn touched_chunks(&self) -> Result<BTreeSet<ChunkKey>, EntityError> {
        match self {
            Self::Mobile { .. } => Ok([self.owner()?.chunk()].into_iter().collect()),
            Self::Anchored {
                anchor, footprint, ..
            } => {
                let mut chunks = BTreeSet::new();
                chunks.insert(anchor.chunk());
                for cell in footprint {
                    chunks.insert(cell.chunk());
                }
                Ok(chunks)
            }
        }
    }
}

/// Owner identity is independent of the worker that executes the entity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EntityOwner {
    Mobile(ChunkKey),
    Anchored(ChunkKey),
}

impl EntityOwner {
    pub const fn chunk(self) -> ChunkKey {
        match self {
            Self::Mobile(chunk) | Self::Anchored(chunk) => chunk,
        }
    }
}

/// Public data is encoded separately from the private server payload.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityPublicView {
    pub id: EntityId,
    pub entity_type: EntityTypeId,
    /// Revision of WAL-owned identity, payload, schedule, owner, and structure.
    pub revision: u64,
    /// Revision of checkpointed mobile position; zero for anchored entities.
    pub motion_revision: u64,
    pub owner: EntityOwner,
    pub location: EntityLocation,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityMotionSnapshot {
    pub id: EntityId,
    pub revision: u64,
    pub position: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnchorUpdate {
    pub anchor: CellCoord,
    pub anchor_state: BlockStateId,
    pub footprint: Vec<CellCoord>,
}

/// Fail-closed validation and lifecycle errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityError {
    UnknownType(EntityTypeId),
    DuplicateType(EntityTypeId),
    MissingTypeRegistration(EntityTypeId),
    TypeDefinitionMismatch(EntityTypeId),
    InvalidType,
    InvalidLocation,
    WrongOwnership,
    IncompatibleAnchorState(BlockStateId),
    FootprintOverlap(CellCoord),
    UnknownEntity(EntityId),
    StaleRevision {
        id: EntityId,
        expected: u64,
        actual: Option<u64>,
    },
    StaleMotionRevision {
        id: EntityId,
        expected: u64,
        actual: Option<u64>,
    },
    InvalidPayload,
    CodecRejected,
    PayloadTooLarge,
    PublicViewTooLarge,
    ChunkPayloadBudgetExceeded(ChunkKey),
    ChunkPublicViewBudgetExceeded(ChunkKey),
    ChunkReferenceBudgetExceeded(ChunkKey),
    TooManyEntities,
    TooManyTransactionChanges,
    TransactionTooLarge,
    InvalidTransaction,
    ConflictingTransactionKey,
    NoChanges,
    TransferRequired,
    MotionFenced,
    NotTransfer,
    RevisionExhausted,
    IdExhausted,
    SpatialQueryTooBroad,
    CorruptCheckpoint,
    UnknownRequiredType(EntityTypeId),
}

impl fmt::Display for EntityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for EntityError {}

pub fn position_to_cell(position: [f32; 3]) -> Result<CellCoord, EntityError> {
    fn axis(value: f32) -> Result<i32, EntityError> {
        if !value.is_finite() {
            return Err(EntityError::InvalidLocation);
        }
        let floor = value.floor();
        if floor < i32::MIN as f32 || floor > i32::MAX as f32 {
            return Err(EntityError::InvalidLocation);
        }
        Ok(floor as i32)
    }
    Ok(CellCoord::new(
        axis(position[0])?,
        axis(position[1])?,
        axis(position[2])?,
    ))
}
