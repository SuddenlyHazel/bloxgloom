#[test]
fn measured_exact_paths_default_on_and_explicit_zero_retains_controls() {
    assert!(super::enabled(None));
    assert!(super::enabled(Some("1")));
    assert!(super::enabled(Some("")));
    assert!(!super::enabled(Some("0")));
}
