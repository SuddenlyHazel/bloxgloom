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
