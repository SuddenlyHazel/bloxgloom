use super::*;
fn record() -> Record {
    Record {
        simulation_tick: 20,
        motion: Motion {
            position: [0.5, 10.0, 0.5],
            velocity: [1.0, 2.0, 3.0],
            acceleration: [0.0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
            revision: 19,
            grounded: false,
        },
        remaining_ticks: 100,
        next_behavior_tick: Some(21),
        source: Some(9),
        source_ticks: 3,
        contact: Some(Target::Terrain {
            cell: [0, 9, 0],
            state: "builtin:stone".into(),
        }),
        pending: Some(Pending::Impact(Impact {
            entity: 10,
            motion_revision: 19,
            tick: 20,
            position: [0.5, 10.0, 0.5],
            normal: [0.0, 1.0, 0.0],
            incoming_velocity: [1.0, -2.0, 3.0],
            target: Target::Entity {
                id: 11,
                revision: 5,
            },
            blocked: false,
        })),
        state: vec![0, 255, 3],
    }
}
#[test]
fn pending_reaction_roundtrip_and_truncations() {
    let value = record();
    let bytes = value.encode().unwrap();
    assert_eq!(Record::decode(&bytes).unwrap(), value);
    for n in 0..bytes.len() {
        assert!(Record::decode(&bytes[..n]).is_err(), "{n}");
    }
    let mut extended = bytes;
    extended.push(0);
    assert!(Record::decode(&extended).is_err());
}
#[test]
fn reject_noncanonical_pose_and_impact_identity() {
    let mut value = record();
    value.motion.orientation = [0.0; 4];
    assert!(value.encode().is_err());
    value = record();
    if let Some(Pending::Impact(impact)) = &mut value.pending {
        impact.motion_revision += 1;
    }
    assert!(value.encode().is_err());
    value = record();
    value.motion.position[1] = f32::NAN;
    assert!(value.encode().is_err());
    value = record();
    value.motion.velocity = [64.0, 64.0, 0.0];
    assert!(value.encode().is_err());
}
#[test]
fn public_projection_keeps_private_state_separate() {
    let value = record();
    let public = Projection {
        motion: value.motion,
        tick: 20,
        stopped: true,
        data: vec![4, 5],
    };
    let bytes = public.encode().unwrap();
    assert_eq!(Projection::decode(&bytes).unwrap(), public);
    assert!(Record::decode(&bytes).is_err());
    let mut malformed = bytes;
    malformed.push(1);
    assert!(Projection::decode(&malformed).is_err());
}
#[test]
fn expiry_roundtrip() {
    let mut value = record();
    value.pending = Some(Pending::Expiry {
        entity: 10,
        motion_revision: 19,
        tick: 200,
        reason: ExpiryReason::WorldBoundary,
    });
    assert_eq!(Record::decode(&value.encode().unwrap()).unwrap(), value);
}
