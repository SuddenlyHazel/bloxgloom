use super::*;
#[test]
fn durable_state_is_canonical_and_bounded() {
    for state in [
        State::default(),
        State {
            current: 0,
            max: 999,
            life: 91,
            respawn: None,
        },
        State {
            current: MAX_HEALTH,
            max: MAX_HEALTH,
            life: u64::MAX,
            respawn: None,
        },
    ] {
        assert_eq!(State::decode(&state.encode().unwrap()).unwrap(), state);
    }
    for state in [
        State {
            current: 1,
            max: 0,
            life: 1,
            respawn: None,
        },
        State {
            current: 101,
            max: 100,
            life: 1,
            respawn: None,
        },
        State {
            current: 1,
            max: MAX_HEALTH + 1,
            life: 1,
            respawn: None,
        },
        State {
            current: 0,
            max: 100,
            life: 0,
            respawn: None,
        },
    ] {
        assert!(state.encode().is_err());
    }
    let mut bytes = State::default().encode().unwrap();
    bytes.push(0);
    assert!(State::decode(&bytes).is_err());
    bytes.remove(0);
    assert!(State::decode(&bytes).is_err());
}
