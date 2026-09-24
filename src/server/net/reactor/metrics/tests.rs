use super::*;

#[test]
fn send_age_p95_bucket_is_a_conservative_upper_bound() {
    let stats = TransportStats::default();
    for _ in 0..19 {
        stats.send_age(Duration::from_millis(3));
    }
    stats.send_age(Duration::from_millis(20));
    let snapshot = stats.snapshot();
    assert_eq!(snapshot.send_age_ms_p95_upper, 4);
    assert_eq!(snapshot.send_age_ms_max, 20);
}
