//! Batched WAL validation, append, and receipt bookkeeping.

use super::codec::{encode_frame, invalid_data};
use super::writer::Request;
use super::{CommitReceipt, Journal, KnownRecord, MAX_JOURNAL_BYTES, Transaction};
use std::collections::HashMap;
use std::io::{self, Write};

impl Journal {
    pub(super) fn append_batch(
        &mut self,
        requests: Vec<Request>,
    ) -> Vec<io::Result<CommitReceipt>> {
        if let Some((kind, message)) = &self.poisoned {
            return requests
                .iter()
                .map(|_| Err(io::Error::new(*kind, message.clone())))
                .collect();
        }

        enum Status {
            Existing(CommitReceipt),
            Candidate(usize),
            Repeated(usize),
            Error(io::ErrorKind, String),
        }

        let mut statuses = Vec::with_capacity(requests.len());
        let mut candidates = Vec::<Transaction>::new();
        let mut in_batch = HashMap::<u128, usize>::new();
        let mut projected_next_id = self.next_transaction_id;
        // Overlay only keys touched by this batch; cloning the journal's full
        // latest-key map on every fsync would scale with lifetime world edits.
        let mut projected_latest = HashMap::new();
        for request in &requests {
            let tx = match request.transaction.clone().canonicalize() {
                Ok(tx) => tx,
                Err(error) => {
                    statuses.push(Status::Error(error.kind(), error.to_string()));
                    continue;
                }
            };
            if let Some(record) = self.known.get(&tx.id) {
                if self.records[record.index] == tx {
                    statuses.push(Status::Existing(CommitReceipt {
                        id: tx.id,
                        sequence: record.sequence,
                        duplicate: true,
                    }));
                } else {
                    statuses.push(Status::Error(
                        io::ErrorKind::AlreadyExists,
                        "journal transaction ID reused with different data".into(),
                    ));
                }
                continue;
            }
            if self.manifest.is_some() && tx.id < projected_next_id {
                statuses.push(Status::Error(
                    io::ErrorKind::AlreadyExists,
                    "journal transaction ID is below the generation watermark".into(),
                ));
                continue;
            }
            if let Some(&index) = in_batch.get(&tx.id) {
                if candidates[index] == tx {
                    statuses.push(Status::Repeated(index));
                } else {
                    statuses.push(Status::Error(
                        io::ErrorKind::AlreadyExists,
                        "journal transaction ID repeated with different data in batch".into(),
                    ));
                }
                continue;
            }
            if tx.changes.iter().any(|change| {
                projected_latest
                    .get(&change.key)
                    .or_else(|| self.latest.get(&change.key))
                    .is_some_and(|after| after != &change.before)
            }) {
                statuses.push(Status::Error(
                    io::ErrorKind::InvalidData,
                    "journal state-key before value breaks committed history".into(),
                ));
                continue;
            }
            for change in &tx.changes {
                projected_latest.insert(change.key.clone(), change.after.clone());
            }
            if self.manifest.is_some() {
                projected_next_id = tx.id.saturating_add(1);
            }
            let index = candidates.len();
            in_batch.insert(tx.id, index);
            candidates.push(tx);
            statuses.push(Status::Candidate(index));
        }

        if candidates.is_empty() {
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(invalid_data("internal journal batch state"))
                    }
                })
                .collect();
        }

        let first_sequence = match self.physical_records.checked_add(1) {
            Some(sequence) => sequence,
            None => {
                return statuses
                    .into_iter()
                    .map(|status| match status {
                        Status::Existing(receipt) => Ok(receipt),
                        Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                        Status::Candidate(_) | Status::Repeated(_) => {
                            Err(invalid_data("journal sequence exhausted"))
                        }
                    })
                    .collect();
            }
        };
        if self
            .physical_records
            .checked_add(candidates.len() as u64)
            .is_none()
        {
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(invalid_data("journal sequence exhausted"))
                    }
                })
                .collect();
        }

        let mut next_transaction_id = self.next_transaction_id;
        for transaction in &candidates {
            let Some(next) = transaction.id.checked_add(1) else {
                return statuses
                    .into_iter()
                    .map(|status| match status {
                        Status::Existing(receipt) => Ok(receipt),
                        Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                        Status::Candidate(_) | Status::Repeated(_) => {
                            Err(invalid_data("journal transaction ID space exhausted"))
                        }
                    })
                    .collect();
            };
            next_transaction_id = next_transaction_id.max(next);
        }

        let mut frames = Vec::with_capacity(candidates.len());
        for transaction in &candidates {
            match encode_frame(transaction) {
                Ok(frame) => frames.push(frame),
                Err(error) => {
                    return statuses
                        .into_iter()
                        .map(|status| match status {
                            Status::Existing(receipt) => Ok(receipt),
                            Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                            Status::Candidate(_) | Status::Repeated(_) => {
                                Err(io::Error::new(error.kind(), error.to_string()))
                            }
                        })
                        .collect();
                }
            }
        }

        let append_bytes = frames
            .iter()
            .try_fold(0u64, |total, frame| total.checked_add(frame.len() as u64));
        let Some(projected_bytes) =
            append_bytes.and_then(|bytes| self.log_bytes.checked_add(bytes))
        else {
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(io::Error::other("journal byte length overflow"))
                    }
                })
                .collect();
        };
        if projected_bytes > MAX_JOURNAL_BYTES {
            let message = format!(
                "journal capacity reached ({} of {} bytes); checkpoint rotation is required",
                self.log_bytes, MAX_JOURNAL_BYTES
            );
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(io::Error::other(message.clone()))
                    }
                })
                .collect();
        }

        let append_result = (|| {
            for frame in &frames {
                self.file.write_all(frame)?;
            }
            self.file.sync_all()
        })();
        if let Err(error) = append_result {
            self.poisoned = Some((error.kind(), error.to_string()));
            return statuses
                .into_iter()
                .map(|status| match status {
                    Status::Existing(receipt) => Ok(receipt),
                    Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                    Status::Candidate(_) | Status::Repeated(_) => {
                        Err(io::Error::new(error.kind(), error.to_string()))
                    }
                })
                .collect();
        }

        for (index, transaction) in candidates.into_iter().enumerate() {
            let sequence = first_sequence + index as u64;
            let record_index = self.records.len();
            self.known.insert(
                transaction.id,
                KnownRecord {
                    index: record_index,
                    sequence,
                },
            );
            self.records.push(transaction);
        }
        self.physical_records += frames.len() as u64;
        self.log_bytes = projected_bytes;
        self.next_transaction_id = next_transaction_id;
        self.latest.extend(projected_latest);
        let first_record = self.records.len() - frames.len();
        for (record_offset, transaction) in self.records[first_record..].iter().enumerate() {
            for (change_index, change) in transaction.changes.iter().enumerate() {
                self.history
                    .entry(change.key.clone())
                    .or_default()
                    .push((first_record + record_offset, change_index));
            }
        }

        statuses
            .into_iter()
            .map(|status| match status {
                Status::Existing(receipt) => Ok(receipt),
                Status::Error(kind, message) => Err(io::Error::new(kind, message)),
                Status::Candidate(index) => Ok(CommitReceipt {
                    id: self.records[self.records.len() - frames.len() + index].id,
                    sequence: first_sequence + index as u64,
                    duplicate: false,
                }),
                Status::Repeated(index) => Ok(CommitReceipt {
                    id: self.records[self.records.len() - frames.len() + index].id,
                    sequence: first_sequence + index as u64,
                    duplicate: true,
                }),
            })
            .collect()
    }
}
