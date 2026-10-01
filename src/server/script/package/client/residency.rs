//! Process admission for canonical bytes and copied source/asset payloads.
//! Decoded declarations and prepared resources have independent bounds; limiting
//! live artifacts also bounds repeated copies of those resources across sessions.
use super::{ScriptError, error};
use std::sync::Mutex;

const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_ARTIFACTS: usize = 32;
static BUDGET: Budget = Budget(Mutex::new((0, 0)));

#[derive(Debug)]
struct Budget(Mutex<(usize, usize)>);

#[derive(Debug)]
pub(super) struct Reservation<'a> {
    budget: &'a Budget,
    bytes: usize,
    encoded: usize,
}

pub(super) fn reserve(encoded: usize) -> Result<Reservation<'static>, ScriptError> {
    BUDGET.reserve(encoded)
}

impl Budget {
    fn reserve(&self, encoded: usize) -> Result<Reservation<'_>, ScriptError> {
        let bytes = encoded
            .checked_mul(2)
            .ok_or_else(|| error("<client-bundle>", "residency byte overflow"))?;
        let mut used = self.0.lock().unwrap();
        if used.0.saturating_add(bytes) > MAX_BYTES || used.1 == MAX_ARTIFACTS {
            return Err(error(
                "<client-bundle>",
                format!(
                    "canonical/source/asset bytes/process: attempted {}; maximum {MAX_BYTES}; artifacts/process: attempted {}; maximum {MAX_ARTIFACTS}",
                    used.0.saturating_add(bytes),
                    used.1 + 1
                ),
            ));
        }
        used.0 += bytes;
        used.1 += 1;
        Ok(Reservation {
            budget: self,
            bytes,
            encoded,
        })
    }
}

impl Reservation<'_> {
    pub(super) fn add_payload(&mut self, bytes: usize) -> Result<(), ScriptError> {
        let mut used = self.budget.0.lock().unwrap();
        let next = used.0.saturating_add(bytes);
        if next > MAX_BYTES {
            return Err(error(
                "<client-bundle>",
                format!(
                    "canonical/source/asset/texture bytes/process: attempted {next}; maximum {MAX_BYTES}"
                ),
            ));
        }
        used.0 = next;
        self.bytes += bytes;
        Ok(())
    }

    pub(super) fn resize(&mut self, encoded: usize) -> Result<(), ScriptError> {
        let bytes = self.bytes - self.encoded * 2
            + encoded
                .checked_mul(2)
                .ok_or_else(|| error("<client-bundle>", "residency byte overflow"))?;
        let mut used = self.budget.0.lock().unwrap();
        let next = used.0 - self.bytes;
        if next.saturating_add(bytes) > MAX_BYTES {
            return Err(error(
                "<client-bundle>",
                format!(
                    "canonical/source/asset bytes/process: attempted {}; maximum {MAX_BYTES}",
                    next.saturating_add(bytes)
                ),
            ));
        }
        used.0 = next + bytes;
        self.bytes = bytes;
        self.encoded = encoded;
        Ok(())
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        let mut used = self.budget.0.lock().unwrap();
        used.0 -= self.bytes;
        used.1 -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_shared_artifacts_and_rejected_growth_release_exact_admission() {
        let budget = Budget(Mutex::new((0, 0)));
        let mut first = budget.reserve(MAX_BYTES / 4).unwrap();
        let second = budget.reserve(MAX_BYTES / 4).unwrap();
        assert!(budget.reserve(1).is_err());
        assert!(first.resize(MAX_BYTES / 2).is_err());
        assert_eq!(*budget.0.lock().unwrap(), (MAX_BYTES, 2));
        drop(second);
        first.resize(MAX_BYTES / 2).unwrap();
        assert_eq!(*budget.0.lock().unwrap(), (MAX_BYTES, 1));
        drop(first);
        let cached = std::sync::Arc::new(budget.reserve(1024).unwrap());
        let session = std::sync::Arc::clone(&cached);
        assert_eq!(*budget.0.lock().unwrap(), (2048, 1));
        drop(cached);
        assert_eq!(*budget.0.lock().unwrap(), (2048, 1));
        drop(session);
        assert_eq!(*budget.0.lock().unwrap(), (0, 0));
        let small = (0..MAX_ARTIFACTS)
            .map(|_| budget.reserve(1).unwrap())
            .collect::<Vec<_>>();
        assert!(budget.reserve(1).is_err());
        drop(small);
        assert!(budget.reserve(MAX_BYTES / 2).is_ok());
    }

    #[test]
    fn wrapper_growth_keeps_texture_payload_accounted() {
        let budget = Budget(Mutex::new((0, 0)));
        let mut artifact = budget.reserve(1024).unwrap();
        artifact.add_payload(4096).unwrap();
        artifact.resize(2048).unwrap();
        assert_eq!(*budget.0.lock().unwrap(), (8192, 1));
        assert!(artifact.add_payload(MAX_BYTES).is_err());
        assert_eq!(*budget.0.lock().unwrap(), (8192, 1));
        drop(artifact);
        assert_eq!(*budget.0.lock().unwrap(), (0, 0));
    }
}
