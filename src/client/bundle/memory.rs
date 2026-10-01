//! Process-wide admission for temporary download and verification storage.
//! The separate single-entry artifact cache has an encoded/payload bound of
//! twice MAX_BUNDLE_BYTES; prepared resources retain their own expansion caps.
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_TRANSIENT_BYTES: usize = 128 * 1024 * 1024;
static BUDGET: Budget = Budget(AtomicUsize::new(0));

struct Budget(AtomicUsize);

pub(super) struct Reservation<'a> {
    budget: &'a Budget,
    bytes: usize,
}

pub(super) fn reserve(payload: usize) -> io::Result<Reservation<'static>> {
    // Also charge the raw protocol frame and decoded part scratch buffers.
    let bytes = payload
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(2 * crate::protocol::MAX_FRAME))
        .ok_or_else(|| io::Error::other("client bundle verification memory overflow"))?;
    BUDGET.reserve(bytes)
}

impl Budget {
    fn reserve(&self, bytes: usize) -> io::Result<Reservation<'_>> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|total| *total <= MAX_TRANSIENT_BYTES)
            })
            .map_err(|used| io::Error::other(format!(
                "client bundle download/verification memory/process: attempted {}; maximum {MAX_TRANSIENT_BYTES}",
                used.saturating_add(bytes)
            )))?;
        Ok(Reservation {
            budget: self,
            bytes,
        })
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.budget.0.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests;
