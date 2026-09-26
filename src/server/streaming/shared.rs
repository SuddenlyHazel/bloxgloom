//! Coordinator-owned values with O(1) immutable publication captures.
//! Publication barriers drop captures before authoritative mutation, so normal
//! operation does not copy the maps. Copy-on-write also protects stale captures.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

#[derive(Debug, Default)]
pub(in crate::server) struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    pub(in crate::server) fn capture(&self) -> Arc<T> {
        Arc::clone(&self.0)
    }
    pub(in crate::server) fn matches(&self, capture: &Arc<T>) -> bool {
        Arc::ptr_eq(&self.0, capture)
    }
}
impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: Clone> DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}
