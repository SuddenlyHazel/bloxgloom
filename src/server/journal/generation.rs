//! Journal-generation switching at an externally checkpointed sequence.

use super::rotation;
use super::{DropCompaction, Journal, RotationReceipt, StateKey};
use std::collections::HashMap;
use std::io;
use std::path::Path;

impl Journal {
    pub(super) fn rotate(&mut self, expected_sequence: u64) -> io::Result<RotationReceipt> {
        self.rotate_using(
            expected_sequence,
            None,
            |path, manifest, cut, next_id, values, closed| {
                rotation::rotate(path, manifest, cut, next_id, values, closed)
            },
        )
    }

    pub(super) fn rotate_with_drop_compaction(
        &mut self,
        expected_sequence: u64,
        compaction: DropCompaction,
    ) -> io::Result<RotationReceipt> {
        self.rotate_using(
            expected_sequence,
            Some(compaction),
            |path, manifest, cut, next_id, values, closed| {
                rotation::rotate(path, manifest, cut, next_id, values, closed)
            },
        )
    }

    #[cfg(test)]
    pub(super) fn rotate_crashing_at(
        &mut self,
        expected_sequence: u64,
        point: rotation::CrashPoint,
    ) -> io::Result<RotationReceipt> {
        self.rotate_using(
            expected_sequence,
            None,
            |path, manifest, cut, next_id, values, closed| {
                rotation::rotate_crashing_at(path, manifest, cut, next_id, values, closed, point)
            },
        )
    }

    #[cfg(test)]
    pub(super) fn rotate_with_drop_compaction_crashing_at(
        &mut self,
        expected_sequence: u64,
        compaction: DropCompaction,
        point: rotation::CrashPoint,
    ) -> io::Result<RotationReceipt> {
        self.rotate_using(
            expected_sequence,
            Some(compaction),
            |path, manifest, cut, next_id, values, closed| {
                rotation::rotate_crashing_at(path, manifest, cut, next_id, values, closed, point)
            },
        )
    }

    fn rotate_using(
        &mut self,
        expected_sequence: u64,
        compaction: Option<DropCompaction>,
        switch: impl FnOnce(
            &Path,
            Option<&rotation::Manifest>,
            u64,
            u128,
            &HashMap<StateKey, Vec<u8>>,
            bool,
        ) -> io::Result<rotation::SwitchedGeneration>,
    ) -> io::Result<RotationReceipt> {
        if let Some((kind, message)) = &self.poisoned {
            return Err(io::Error::new(*kind, message.clone()));
        }
        if expected_sequence != self.physical_records {
            return Err(super::invalid_data(
                "journal rotation sequence does not match durable sequence",
            ));
        }

        if let Some(compaction) = compaction {
            let compacted_drops = compaction.into_values()?;
            self.latest.retain(|key, _| {
                !matches!(
                    key.domain.as_str(),
                    "bloxgloom:drop_owner" | "bloxgloom:drop_position" | "bloxgloom:drop_allocator"
                )
            });
            self.latest.extend(compacted_drops);
            self.latest.shrink_to_fit();
            self.drop_owner_set_closed = true;
        }

        let generation = match switch(
            &self.path,
            self.manifest.as_ref(),
            expected_sequence,
            self.next_transaction_id,
            &self.latest,
            self.drop_owner_set_closed,
        ) {
            Ok(switched) => switched,
            Err(error) => {
                // A failed directory sync may leave the new manifest visible.
                // Stop writes until reopen resolves the authoritative generation.
                self.poisoned = Some((error.kind(), error.to_string()));
                return Err(error);
            }
        };

        let old_file = std::mem::replace(&mut self.file, generation.file);
        self.generation = generation.manifest.generation;
        self.manifest = Some(generation.manifest);
        // Only recovery/startup uses the base anchor. Live append validation
        // uses `latest`, so do not duplicate a potentially large compacted
        // world after the generation switch.
        self.base_anchor = HashMap::new();
        self.records = Vec::new();
        self.known = HashMap::new();
        self.history = HashMap::new();
        self.log_bytes = generation.tail_bytes;
        drop(old_file);
        rotation::cleanup_old_files(&self.path, &generation.old_files);
        Ok(RotationReceipt {
            cut_sequence: expected_sequence,
            generation: self.generation,
        })
    }
}
