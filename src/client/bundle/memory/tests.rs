use super::*;

#[test]
fn simultaneous_verification_is_bounded_and_failure_releases_admission() {
    let budget = Budget(AtomicUsize::new(0));
    let first = budget.reserve(MAX_TRANSIENT_BYTES).unwrap();
    assert!(
        budget
            .reserve(1)
            .err()
            .unwrap()
            .to_string()
            .contains("memory/process")
    );
    drop(first);
    assert!(budget.reserve(MAX_TRANSIENT_BYTES).is_ok());
    assert!(budget.reserve(MAX_TRANSIENT_BYTES + 1).is_err());
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}
