//! Typed extension-owned state shared immutably with worker snapshots.
//!
//! Values live decoded in the barrier-owned `DurableOwnerStore` and are
//! serialized only for WAL change values through each system's registered
//! `OwnerValueCodec`. Workers receive cloned snapshots and return patches;
//! they never hold the store itself.

use std::any::Any;
use std::sync::Arc;

/// Typed extension-owned state shared immutably with worker snapshots.
#[derive(Clone)]
pub(in crate::server) struct OwnerData(Arc<dyn Any + Send + Sync>);

impl OwnerData {
    #[allow(
        dead_code,
        reason = "Owner extensions construct typed seed, codec, and patch values."
    )]
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Arc::new(value))
    }

    #[allow(
        dead_code,
        reason = "Owner extensions read their typed values in codecs and handlers."
    )]
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }

    pub fn same_type(&self, other: &Self) -> bool {
        self.0.as_ref().type_id() == other.0.as_ref().type_id()
    }
}
