//! Old callback revisions may outlive publication. Bound their source/assets
//! together rather than assuming every advisory realm immediately runs again.
use super::{ScriptError, error};
use std::sync::Mutex;

const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_SNAPSHOTS: usize = 64;
static USED: Mutex<(usize, usize)> = Mutex::new((0, 0));

pub(super) struct Reservation(usize);
impl Reservation {
    pub(super) fn acquire(bytes: usize) -> Result<Self, ScriptError> {
        let mut used = USED.lock().unwrap_or_else(|e| e.into_inner());
        if used.0.saturating_add(bytes) > MAX_BYTES || used.1 >= MAX_SNAPSHOTS {
            return Err(error(
                "<packages>",
                "package revision residency exceeded (256 MiB / 64 snapshots); retire unused realms or restart",
            ));
        }
        used.0 += bytes;
        used.1 += 1;
        Ok(Self(bytes))
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let mut used = USED.lock().unwrap_or_else(|e| e.into_inner());
        used.0 -= self.0;
        used.1 -= 1;
    }
}
