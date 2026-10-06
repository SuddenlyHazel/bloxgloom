#[test]
fn diagnostic_history_budget_is_explicit_and_bounded() {
    for value in [
        None,
        Some(""),
        Some("0"),
        Some("31"),
        Some("257"),
        Some("-1"),
        Some("NaN"),
    ] {
        assert_eq!(super::parse_samples(value), 0, "{value:?}");
    }
    for value in [32, 64, 128, 256] {
        assert_eq!(super::parse_samples(Some(&value.to_string())), value);
    }
}
