use super::*;

#[test]
fn sprint_rates_preserve_geometry_crouch_precedence_and_validated_burst_bound() {
    let sprint = BUILTIN_RULES.for_movement(false, true);
    sprint.validate().unwrap();
    assert_eq!(sprint.body(), BUILTIN_BODY);
    assert_eq!(sprint.motion().intent_blocks_per_second, 12.0);
    assert_eq!(sprint.motion().budget_blocks_per_second, 15.0);
    assert_eq!(
        BUILTIN_RULES.for_movement(true, true),
        BUILTIN_RULES.for_stance(true)
    );
    let fast = PlayerRules::new(
        BUILTIN_BODY,
        MotionRates {
            intent_blocks_per_second: 15.0,
            budget_blocks_per_second: 16.0,
        },
        BUILTIN_SPAWN,
        1.6,
    )
    .unwrap();
    assert_eq!(fast.for_movement(false, true), fast);
    fast.for_movement(false, true).validate().unwrap();
    assert_eq!(
        PlayerRules::from_canonical_bytes(BUILTIN_RULES.canonical_bytes()).unwrap(),
        BUILTIN_RULES
    );
}

#[test]
fn crouch_stance_derives_body_eye_and_rates_without_changing_frozen_contract() {
    let crouched = BUILTIN_RULES.for_stance(true);
    crouched.validate().unwrap();
    assert_eq!(BUILTIN_RULES.for_stance(false), BUILTIN_RULES);
    assert_eq!(crouched.body().half_width, BUILTIN_BODY.half_width);
    assert_eq!(crouched.body().foot_inset, BUILTIN_BODY.foot_inset);
    assert!(crouched.body().head_height < BUILTIN_BODY.head_height);
    assert!(crouched.eye_height() < BUILTIN_RULES.eye_height());
    assert_eq!(crouched.motion().budget_blocks_per_second, 4.0);
    assert_eq!(BUILTIN_RULES.motion().budget_blocks_per_second, 10.0);
}

#[test]
fn immutable_rules_preserve_builtin_contract() {
    let rules = PlayerRules::new(BUILTIN_BODY, BUILTIN_MOTION, BUILTIN_SPAWN, 1.6).unwrap();
    assert_eq!(rules, BUILTIN_RULES);
    assert_eq!(rules.body(), BUILTIN_BODY);
    assert_eq!(rules.motion(), BUILTIN_MOTION);
    assert_eq!(rules.spawn(), BUILTIN_SPAWN);
    assert_eq!(rules.eye_height(), 1.6);
    let mut detached_body = rules.body();
    detached_body.half_width = 0.5;
    assert_ne!(rules.body(), detached_body);
}

#[test]
fn canonical_rules_round_trip_and_reject_nonfinite_fields() {
    let bytes = BUILTIN_RULES.canonical_bytes();
    assert_eq!(
        PlayerRules::from_canonical_bytes(bytes).unwrap(),
        BUILTIN_RULES
    );
    for offset in [0, 4, 8, 12, 16, 20] {
        let mut malformed = bytes;
        malformed[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(PlayerRules::from_canonical_bytes(malformed).is_err());
    }
    let mut malformed = bytes;
    malformed[24..32].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(PlayerRules::from_canonical_bytes(malformed).is_err());
}

#[test]
fn rules_reject_invalid_geometry_rates_search_and_eye() {
    for body in [
        Body {
            half_width: f32::NAN,
            ..BUILTIN_BODY
        },
        Body {
            half_width: 0.6,
            ..BUILTIN_BODY
        },
        Body {
            foot_inset: -0.1,
            ..BUILTIN_BODY
        },
        Body {
            middle_height: 0.05,
            ..BUILTIN_BODY
        },
        Body {
            middle_height: 1.1,
            ..BUILTIN_BODY
        },
        Body {
            head_height: f32::INFINITY,
            ..BUILTIN_BODY
        },
        Body {
            head_height: 2.1,
            ..BUILTIN_BODY
        },
    ] {
        assert_eq!(
            PlayerRules::new(body, BUILTIN_MOTION, BUILTIN_SPAWN, 1.6),
            Err(InvalidPlayerRules::Body)
        );
    }
    for motion in [
        MotionRates {
            intent_blocks_per_second: f32::NAN,
            ..BUILTIN_MOTION
        },
        MotionRates {
            intent_blocks_per_second: 0.0,
            ..BUILTIN_MOTION
        },
        MotionRates {
            intent_blocks_per_second: 11.0,
            ..BUILTIN_MOTION
        },
        MotionRates {
            budget_blocks_per_second: f64::INFINITY,
            ..BUILTIN_MOTION
        },
        MotionRates {
            budget_blocks_per_second: 17.0,
            ..BUILTIN_MOTION
        },
    ] {
        assert_eq!(
            PlayerRules::new(BUILTIN_BODY, motion, BUILTIN_SPAWN, 1.6),
            Err(InvalidPlayerRules::Motion)
        );
    }
    for spawn in [
        SpawnSearch {
            headroom: 1,
            ..BUILTIN_SPAWN
        },
        SpawnSearch {
            headroom: i32::MAX,
            ..BUILTIN_SPAWN
        },
        SpawnSearch {
            max_rise: 0,
            ..BUILTIN_SPAWN
        },
        SpawnSearch {
            max_rise: i32::MAX,
            ..BUILTIN_SPAWN
        },
    ] {
        assert_eq!(
            PlayerRules::new(BUILTIN_BODY, BUILTIN_MOTION, spawn, 1.6),
            Err(InvalidPlayerRules::Spawn)
        );
    }
    for eye in [f32::NAN, f32::INFINITY, -0.1, 2.0] {
        assert_eq!(
            PlayerRules::new(BUILTIN_BODY, BUILTIN_MOTION, BUILTIN_SPAWN, eye),
            Err(InvalidPlayerRules::EyeHeight)
        );
    }
}

#[test]
fn movement_samples_and_placement_bounds_agree_at_block_edges() {
    let feet = [0.5, 1.0, 0.5];
    for block in [[0, 1, 0], [0, 2, 0]] {
        assert!(BUILTIN_BODY.intersects_block(block, feet));
        assert!(
            BUILTIN_BODY
                .collides(feet, |x, y, z| Ok::<_, ()>([x, y, z] == block))
                .unwrap()
        );
    }
    assert!(!BUILTIN_BODY.intersects_block([1, 2, 0], feet));
    assert!(
        !BUILTIN_BODY
            .collides(feet, |x, y, z| Ok::<_, ()>([x, y, z] == [1, 2, 0]))
            .unwrap()
    );
}

#[test]
fn unavailable_voxel_stops_collision_instead_of_becoming_empty() {
    let feet = [0.5, 1.0, 0.5];
    assert_eq!(
        BUILTIN_BODY.collides(feet, |_, _, _| Err::<bool, _>(
            "missing authoritative voxel"
        )),
        Err("missing authoritative voxel")
    );
    assert!(!BUILTIN_BODY.intersects_block([0, 3, 0], feet));
    assert!(!BUILTIN_BODY.intersects_block([1, 1, 0], feet));
}

#[test]
fn spawn_search_preserves_startup_highest_and_live_up_then_down_order() {
    let policy = BUILTIN_SPAWN;
    assert_eq!(policy.ceiling(120), 152);
    let startup = policy.startup_support_levels(-3, 0).collect::<Vec<_>>();
    assert_eq!(startup.first(), Some(&31));
    assert_eq!(startup.last(), Some(&-3));
    let live = policy.cached_feet_levels(2, -3).collect::<Vec<_>>();
    assert_eq!(&live[..3], &[2, 3, 4]);
    assert_eq!(&live[live.len() - 4..], &[1, 0, -1, -2]);
    assert_eq!(policy.feet(-2), [0.5, -2.0, 0.5]);
}

#[test]
fn builtin_input_rate_stays_below_authoritative_budget() {
    assert_eq!(BUILTIN_MOTION.intent_blocks_per_second, 8.0);
    assert_eq!(BUILTIN_MOTION.budget_blocks_per_second, 10.0);
    assert!(
        f64::from(BUILTIN_MOTION.intent_blocks_per_second)
            < BUILTIN_MOTION.budget_blocks_per_second
    );
}
