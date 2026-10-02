use super::*;
#[test]
fn live_decay_change_preserves_the_tail_and_changes_its_remaining_energy() {
    let mut short = Reverb::new([739, 953, 1151, 1327, 1471, 1663], 44100.0, 0.65, 0.16);
    let mut long = Reverb::new([739, 953, 1151, 1327, 1471, 1663], 44100.0, 0.65, 0.16);
    for i in 0..4096 {
        assert_eq!(
            short.next(if i == 0 { 1.0 } else { 0.0 }),
            long.next(if i == 0 { 1.0 } else { 0.0 })
        );
    }
    short.configure(44100.0, 0.1, 0.16);
    long.configure(44100.0, 10.0, 0.16);
    let mut early = 0.0f64;
    let mut energy = [0.0f64; 2];
    for i in 0..44100 {
        let a = short.next(0.0);
        let b = long.next(0.0);
        assert!(a.iter().chain(&b).all(|v| v.is_finite()));
        if i < 1000 {
            early += a.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        }
        if i > 22050 {
            energy[0] += a.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
            energy[1] += b.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        }
    }
    assert!(
        early > 0.0,
        "configuring must not erase existing reflections"
    );
    assert!(energy[1] > 100.0 * energy[0]);
}
