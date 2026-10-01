use super::*;
#[test]
fn wind_matches_upstream_c_reference() {
    // NoiseMachine e709f125, cc -O2, seed123, speed10m/s, bearing0.8, gain0.5.
    let expected = [
        [1.133_188_9e-6, 2.224_041e-6],
        [-9.476_577e-6, 2.361_895e-5],
        [-4.830_49e-5, -2.159_668e-5],
        [-7.132_094e-5, -4.452_542_6e-5],
        [-7.364_296e-5, 1.676_033e-5],
        [-6.883_75e-5, 6.087_612e-5],
        [-9.611_163e-5, 3.165_375e-5],
        [-1.158_104_2e-4, 5.990_588_3e-6],
    ];
    let mut wind = Wind::new(123);
    wind.follow(10.0, 0.8);
    for reference in expected {
        let actual = wind.next();
        for ear in 0..2 {
            assert!(
                (actual[ear] - reference[ear]).abs() < 1e-9,
                "{actual:?} != {reference:?}"
            );
        }
    }
}
