use super::*;
use crate::server::client_bundle::{CacheKey, ScriptFailure};
use sha2::{Digest, Sha256};

fn bundle() -> Arc<ClientBundle> {
    let mut bytes = b"BGCLIENT\x07".to_vec();
    bytes.extend(0_u32.to_le_bytes()); // packages
    bytes.extend(0_u32.to_le_bytes()); // declarations
    let key = CacheKey::from_bytes(Sha256::digest(&bytes).into());
    Arc::new(ClientBundle::decode_verify(&bytes, key).unwrap())
}

fn pressure() -> ScriptError {
    ScriptError {
        module: "farm:declaration farm:crop".into(),
        failure: ScriptFailure::BundleResidency("estimated declaration process admission".into()),
    }
}

#[test]
fn unused_cache_is_released_before_one_local_verification_retry() {
    let old = bundle();
    let weak = Arc::downgrade(&old);
    let cache = Mutex::new(Some(old));
    let mut calls = 0;
    let result = decode(&cache, || {
        calls += 1;
        if calls == 1 {
            return Err(pressure());
        }
        assert!(
            weak.upgrade().is_none(),
            "retired artifact must release admission before retry"
        );
        Ok(42)
    })
    .unwrap();
    assert_eq!(result, 42);
    assert_eq!(calls, 2);
    assert!(cache.lock().unwrap().is_none());
}

#[test]
fn active_and_retiring_references_prevent_eviction_and_retry() {
    let live = bundle();
    let cache = Mutex::new(Some(Arc::clone(&live)));
    let mut calls = 0;
    let result = decode::<()>(&cache, || {
        calls += 1;
        Err(pressure())
    });
    assert!(result.unwrap_err().is_bundle_residency_exhausted());
    assert_eq!(calls, 1);
    assert!(Arc::ptr_eq(cache.lock().unwrap().as_ref().unwrap(), &live));
}

#[test]
fn ordinary_corruption_keeps_cache_and_retry_is_strictly_bounded() {
    let cache = Mutex::new(Some(bundle()));
    let ordinary = ScriptError {
        module: "<client-bundle>".into(),
        failure: ScriptFailure::Package("SHA-256 integrity mismatch".into()),
    };
    let mut calls = 0;
    let result = decode::<()>(&cache, || {
        calls += 1;
        Err(ordinary.clone())
    });
    assert_eq!(result.unwrap_err(), ordinary);
    assert_eq!(calls, 1);
    assert!(cache.lock().unwrap().is_some());
    let mut calls = 0;
    let result = decode::<()>(&cache, || {
        calls += 1;
        Err(pressure())
    });
    assert!(result.unwrap_err().is_bundle_residency_exhausted());
    assert_eq!(calls, 2);
    assert!(cache.lock().unwrap().is_none());
}

#[test]
fn metadata_corruption_after_pressure_does_not_restore_retired_memo() {
    let cache = Mutex::new(Some(bundle()));
    let mut calls = 0;
    let result = decode::<()>(&cache, || {
        calls += 1;
        Err(if calls == 1 {
            pressure()
        } else {
            ScriptError {
                module: "<client-bundle>".into(),
                failure: ScriptFailure::Package("invalid metadata".into()),
            }
        })
    });
    assert!(!result.unwrap_err().is_bundle_residency_exhausted());
    assert_eq!(calls, 2);
    assert!(cache.lock().unwrap().is_none());
}
