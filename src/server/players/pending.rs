//! Linearizes pending-join cancellation against admission without socket I/O.
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
#[derive(Clone, Default)]
pub(in crate::server) struct JoinGuard(Arc<AtomicU8>);
impl JoinGuard {
    pub(in crate::server) fn cancel(&self) {
        let _ = self
            .0
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
    }
    pub(in crate::server) fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire) == 2
    }
    pub(in crate::server) fn admit(&self) -> bool {
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}
