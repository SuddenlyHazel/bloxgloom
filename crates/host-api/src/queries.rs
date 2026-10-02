//! Frozen content reads shared by authoritative callback contexts.
use crate::content::TagKind;
use std::collections::BTreeSet;

/// Installed tag expansions never change during play. A missing tag is unknown,
/// rather than an empty declaration. Implementations perform no I/O.
pub trait Tags: Send + Sync {
    fn members(&self, kind: TagKind, key: &str) -> Option<&BTreeSet<String>>;
}
mod volume;
pub use volume::{MAX_QUERY_CELLS, box_cells};
