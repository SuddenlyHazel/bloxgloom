//! Explicit, copy-only conversion of a closed v4 save into a new v5 directory.

mod image;
mod legacy;
mod materialize;

#[cfg(test)]
mod tests;

pub use materialize::migrate_v4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    pub source_files: usize,
    pub edited_chunks: usize,
    pub inventories: usize,
    pub drops: usize,
    pub legacy_action_receipts: usize,
}
