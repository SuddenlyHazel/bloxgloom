use super::*;
fn effect(key: &str, speed: f32, until: Option<u64>) -> Effect {
    Effect {
        key: key.into(),
        movement: Movement {
            speed,
            ..Default::default()
        },
        expires_at: until,
    }
}
#[test]
fn modifier_state_is_bounded_canonical_and_preserves_expired_tombstones_until_cleanup() {
    let mut set = Set::default();
    set.set(effect("a:slow", 0.5, Some(20)), 1).unwrap();
    set.set(effect("b:fast", 2.0, None), 1).unwrap();
    let bytes = set.encode().unwrap();
    assert_eq!(Set::decode(&bytes).unwrap(), set);
    for mutated in [
        bytes[..bytes.len() - 1].to_vec(),
        [bytes.clone(), vec![0]].concat(),
    ] {
        assert!(Set::decode(&mutated).is_err());
    }
    let mut invalid = effect("a:x", f32::NAN, None);
    assert!(set.set(invalid.clone(), 1).is_err());
    invalid.movement.speed = 0.0;
    assert!(set.set(invalid, 1).is_err());
    let mut full = Set::default();
    for index in 0..MAX_EFFECTS {
        full.set(effect(&format!("a:x{index}"), 1.0, None), 1)
            .unwrap();
    }
    assert!(full.set(effect("a:overflow", 1.0, None), 1).is_err());
    full.set(effect("a:x0", 2.0, None), 1).unwrap();
    assert!(full.encode().unwrap().len() <= MAX_STATE_BYTES);
}
#[test]
fn modifier_aggregation_is_order_independent_capped_and_ends_at_exact_tick() {
    let mut profile = Set::default();
    profile.set(effect("z:boost", 4.0, None), 1).unwrap();
    profile.set(effect("a:slow", 0.25, None), 1).unwrap();
    let mut session = Set::default();
    session.set(effect("m:boost", 2.0, Some(20)), 1).unwrap();
    assert_eq!(aggregate(&profile, &session, 19).speed, 2.0);
    assert_eq!(aggregate(&profile, &session, 20).speed, 1.0);
    session.set(effect("m:huge", 4.0, None), 1).unwrap();
    assert_eq!(aggregate(&profile, &session, 19).speed, 4.0);
    let capped = aggregate(&profile, &session, 19).rules(crate::player::BUILTIN_RULES, false, true);
    assert!(capped.motion().budget_blocks_per_second <= 16.0);
    assert!(
        f64::from(capped.motion().intent_blocks_per_second)
            <= capped.motion().budget_blocks_per_second
    );
    let reverse = aggregate(&session, &profile, 19);
    assert_eq!(reverse, aggregate(&profile, &session, 19));
}

#[test]
fn custom_rates_and_stances_produce_valid_matching_prediction_budgets() {
    use crate::player::{BUILTIN_RULES, MotionRates, PlayerRules};
    for rate in [
        0.1_f32, 0.10001, 0.25, 0.7, 1.3, 3.33333, 6.125, 10.7, 15.999, 16.0,
    ] {
        for margin in [0.0, 0.00000001, 0.01, 0.1] {
            let base = PlayerRules::new(
                BUILTIN_RULES.body(),
                MotionRates {
                    intent_blocks_per_second: rate,
                    budget_blocks_per_second: (f64::from(rate) + margin).min(16.0),
                },
                BUILTIN_RULES.spawn(),
                BUILTIN_RULES.eye_height(),
            )
            .unwrap();
            for speed in [0.1, 0.333333, 0.7, 1.0, 1.25, 2.7, 4.0] {
                for sprint in [0.1, 1.0, 1.333333, 4.0] {
                    for crouching in [false, true] {
                        for sprinting in [false, true] {
                            let rules = Movement {
                                speed,
                                sprint,
                                ..Default::default()
                            }
                            .rules(base, crouching, sprinting);
                            assert!(rules.validate().is_ok());
                            assert!(
                                f64::from(rules.motion().intent_blocks_per_second)
                                    <= rules.motion().budget_blocks_per_second
                            );
                            if speed == 1.0
                                && sprint == 1.0
                                && base.for_movement(crouching, sprinting).validate().is_ok()
                            {
                                assert_eq!(rules, base.for_movement(crouching, sprinting));
                            }
                        }
                    }
                }
            }
        }
    }
}
