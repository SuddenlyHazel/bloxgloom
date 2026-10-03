use super::*;
#[test]
fn half_open_bounds_keep_neighboring_regions_unambiguous() {
    let mut region = Registration {
        key: "demo:west".into(),
        minimum: [0.; 3],
        maximum: [1.; 3],
        service: "demo:players".into(),
    };
    assert!(region.validate().is_ok());
    assert!(region.contains([0.; 3]));
    assert!(!region.contains([1., 0., 0.]));
    assert!(!region.contains([f32::NAN, 0., 0.]));
    region.maximum[1] = region.minimum[1];
    assert!(region.validate().is_err());
    region.maximum[1] = f32::INFINITY;
    assert!(region.validate().is_err());
}
