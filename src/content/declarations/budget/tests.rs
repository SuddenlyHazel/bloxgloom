use super::*;

#[test]
fn rejected_admission_does_not_consume_count_or_estimated_bytes() {
    let mut budget = Budget::default();
    budget.reserve(1, MAX_BYTES, "farm:large").unwrap();
    let error = budget.reserve(1, 1, "farm:next").unwrap_err().0;
    assert!(
        error.contains("farm:next") && error.contains("estimated declaration bytes/installation")
    );
    budget.reserve(MAX_COUNT - 1, 0, "farm:remaining").unwrap();
    assert!(budget.reserve(1, 0, "farm:excess").is_err());
}
