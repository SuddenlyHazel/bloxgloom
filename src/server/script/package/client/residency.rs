//! Process admission for canonical/source/asset bytes and conservative owned
//! content-declaration estimates (including copied texture PNGs). Prepared UI and
//! render resources retain independent bounds; the artifact count also bounds
//! their repeated preparation across sessions.
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

fn exhausted(message: String) -> ScriptError {
    ScriptError {
        module: "<client-bundle>".into(),
        failure: crate::server::script::ScriptFailure::BundleResidency(message),
    }
}

impl Budget {
    fn reserve(&self, encoded: usize) -> Result<Reservation<'_>, ScriptError> {
        let bytes = encoded
            .checked_mul(2)
            .ok_or_else(|| error("<client-bundle>", "residency byte overflow"))?;
        let mut used = self.0.lock().unwrap();
        if used.0.saturating_add(bytes) > MAX_BYTES || used.1 == MAX_ARTIFACTS {
            return Err(exhausted(format!(
                "canonical/source/asset/declaration bytes/process: attempted {}; maximum {MAX_BYTES}; artifacts/process: attempted {}; maximum {MAX_ARTIFACTS}",
                used.0.saturating_add(bytes),
                used.1 + 1
            )));
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
    // The estimate includes copied PNGs; do not charge those separately.
    pub(super) fn reserve_declaration(
        &mut self,
        budget: &mut crate::content::declarations::budget::Budget,
        bytes: usize,
        key: &str,
        owner: &str,
    ) -> Result<(), ScriptError> {
        budget
            .reserve(1, bytes, key)
            .map_err(|error| super::error(owner, error.0))?;
        self.add_payload(bytes).map_err(|mut error| {
            error.module = format!("{owner}:declaration {key}");
            error
        })
    }

    pub(super) fn add_payload(&mut self, bytes: usize) -> Result<(), ScriptError> {
        let mut used = self.budget.0.lock().unwrap();
        let next = used.0.saturating_add(bytes);
        if next > MAX_BYTES {
            return Err(exhausted(format!(
                "canonical/source/asset/declaration bytes/process: attempted {next}; maximum {MAX_BYTES}"
            )));
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
            return Err(exhausted(format!(
                "canonical/source/asset/declaration bytes/process: attempted {}; maximum {MAX_BYTES}",
                next.saturating_add(bytes)
            )));
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
mod tests;
