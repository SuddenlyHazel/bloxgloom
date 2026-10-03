use super::*;

fn source(x: f32) -> Source {
    Source {
        position: Vec3::new(x, 0.0, 0.0),
        range: 12.0,
        color: [1.0; 3],
    }
}

#[test]
fn six_faces_have_correct_axes_and_directx_depth() {
    let origin = Vec3::new(3.5, -2.5, 9.5);
    let faces = matrices(origin, 12.0);
    for (matrix, axis) in
        faces
            .into_iter()
            .zip([Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z])
    {
        let center = matrix.project_point3(origin + axis);
        assert!(center.x.abs() < 0.0001 && center.y.abs() < 0.0001);
        assert!(center.z > 0.0 && center.z < 1.0);
        let near = matrix.project_point3(origin + axis * NEAR);
        let far = matrix.project_point3(origin + axis * 12.0);
        assert!(near.z.abs() < 0.0001);
        assert!((far.z - 1.0).abs() < 0.0001);
    }
}

#[test]
fn deterministic_nearest_selection_hysteresis_and_fade_replacement() {
    let settings = Settings {
        count: 1,
        ..Default::default()
    };
    let mut slots = [Slot::default()];
    selection::select(
        &mut slots,
        Vec3::ZERO,
        &[source(4.0), source(-4.0)],
        settings,
        0.1,
    );
    assert_eq!(slots[0].source, Some(source(-4.0)));
    assert_eq!(slots[0].weight, 0.0);
    slots[0].initialized = true;
    selection::select(
        &mut slots,
        Vec3::new(0.1, 0.0, 0.0),
        &[source(4.0), source(-4.0)],
        settings,
        0.25,
    );
    assert_eq!(slots[0].source, Some(source(-4.0)));
    assert_eq!(slots[0].weight, 1.0);
    selection::select(&mut slots, Vec3::ZERO, &[source(1.0)], settings, 0.1);
    assert_eq!(slots[0].source, Some(source(-4.0)));
    assert!(slots[0].weight < 1.0);
    selection::select(&mut slots, Vec3::ZERO, &[source(1.0)], settings, 0.25);
    assert_eq!(slots[0].source, Some(source(1.0)));
    assert!(!slots[0].initialized);
    assert_eq!(slots[0].weight, 0.0);
}

#[test]
fn settings_bound_memory_work_and_bad_float_values() {
    let settings = Settings {
        count: 999,
        resolution: u32::MAX,
        range: f32::NAN,
        updates: 999,
    }
    .sanitized(512);
    assert_eq!(
        settings,
        Settings {
            count: 4,
            resolution: 512,
            range: 16.0,
            updates: 4
        }
    );
    assert!(Settings::default().validate().is_ok());
    assert!(
        Settings {
            range: f32::INFINITY,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    let off = Settings {
        count: 0,
        ..Default::default()
    }
    .sanitized(1024);
    assert_eq!(off.resolution, 1);
    assert_eq!(off.updates, 1);
}

#[test]
fn invalid_distant_and_duplicate_sources_do_not_consume_slots() {
    let settings = Settings::default();
    let mut slots = [Slot::default(); 2];
    selection::select(
        &mut slots,
        Vec3::ZERO,
        &[
            source(f32::NAN),
            source(999.0),
            source(2.0),
            source(2.0),
            source(3.0),
        ],
        settings,
        0.1,
    );
    assert_eq!(slots[0].source, Some(source(2.0)));
    assert_eq!(slots[1].source, Some(source(3.0)));
}

#[test]
fn map_reach_does_not_change_emitter_energy_for_attribution() {
    let settings = Settings {
        count: 1,
        range: 2.0,
        ..Default::default()
    };
    let mut slots = [Slot::default()];
    selection::select(&mut slots, Vec3::ZERO, &[source(1.0)], settings, 0.25);
    assert_eq!(
        slots[0].source.unwrap().range,
        12.0,
        "map reach bounds projection, not the original voxel emission level"
    );
}
