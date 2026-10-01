//! Retry downloaded artifact verification once after retiring an unused memo.
//! Ordinary corruption keeps the cache. Under typed residency pressure the
//! unused memo may be discarded even if the retry later finds invalid metadata.
use super::{Arc, ClientBundle, Mutex};
use crate::server::client_bundle::ScriptError;

pub(super) fn decode<T>(
    cache: &Mutex<Option<Arc<ClientBundle>>>,
    mut verify: impl FnMut() -> Result<T, ScriptError>,
) -> Result<T, ScriptError> {
    let error = match verify() {
        Ok(bundle) => return Ok(bundle),
        Err(error) if error.is_bundle_residency_exhausted() => error,
        Err(error) => return Err(error),
    };
    let retired = {
        let mut cache = cache.lock().unwrap();
        if cache
            .as_ref()
            .is_some_and(|bundle| Arc::strong_count(bundle) == 1)
        {
            cache.take()
        } else {
            None
        }
    };
    if let Some(retired) = retired {
        // Free the residency reservation before trying the existing bytes.
        // Active/retiring sessions retain an Arc and can never take this path.
        drop(retired);
        verify()
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests;
