use super::*;
fn channel(interpolation: Interpolation, property: Property, values: Vec<[f32; 4]>) -> Channel {
    Channel {
        node: 0,
        property,
        interpolation,
        times: vec![0.0, 1.0],
        values,
    }
}
#[test]
fn keyframe_sampling_clamps_steps_and_preserves_cubic_tangents() {
    let keys = vec![[2.0, 0.0, 0.0, 0.0], [4.0, 0.0, 0.0, 0.0]];
    let linear = channel(Interpolation::Linear, Property::Translation, keys.clone());
    assert_eq!(linear.sample(0.5)[0], 3.0);
    assert_eq!(linear.sample(2.0)[0], 4.0);
    assert_eq!(linear.sample(-1.0)[0], 2.0);
    let step = channel(Interpolation::Step, Property::Translation, keys);
    assert_eq!(step.sample(0.99)[0], 2.0);
    assert_eq!(step.sample(1.0)[0], 4.0);
    let cubic = channel(
        Interpolation::CubicSpline,
        Property::Translation,
        vec![
            [0.0; 4],
            [0.0; 4],
            [2.0, 0.0, 0.0, 0.0],
            [0.0; 4],
            [1.0, 0.0, 0.0, 0.0],
            [0.0; 4],
        ],
    );
    assert_eq!(cubic.sample(0.5)[0], 0.75);
}
#[test]
fn rotations_take_shortest_path_and_cubic_output_stays_normalized() {
    let end = Quat::from_rotation_y(1.0);
    let linear = channel(
        Interpolation::Linear,
        Property::Rotation,
        vec![Quat::IDENTITY.to_array(), (-end).to_array()],
    );
    let actual = Quat::from_array(linear.sample(0.5));
    assert!(actual.abs_diff_eq(Quat::from_rotation_y(0.5), 0.0001));
    let cubic = channel(
        Interpolation::CubicSpline,
        Property::Rotation,
        vec![
            [0.0; 4],
            Quat::IDENTITY.to_array(),
            [0.0; 4],
            [0.0; 4],
            end.to_array(),
            [0.0; 4],
        ],
    );
    assert!((Quat::from_array(cubic.sample(0.37)).length_squared() - 1.0).abs() < 0.00001);
}
