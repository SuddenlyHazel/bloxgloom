use super::*;
#[test]
fn session_modifiers_end_on_disconnect_and_expiry_without_becoming_profile_data() {
    let mut runtime = Runtime::default();
    let movement = Movement {
        speed: 0.5,
        ..Default::default()
    };
    runtime
        .apply(
            1,
            10,
            "demo:slow",
            Some(Effect {
                key: "demo:slow".into(),
                movement,
                expires_at: Some(30),
            }),
            1,
        )
        .unwrap();
    assert_eq!(
        runtime.capture(1, 10).get("demo:slow").unwrap().movement,
        movement
    );
    assert!(runtime.capture(1, 11).iter().next().is_none());
    runtime.expire(30);
    assert!(runtime.capture(1, 10).iter().next().is_none());
    runtime
        .apply(
            1,
            10,
            "demo:slow",
            Some(Effect {
                key: "demo:slow".into(),
                movement,
                expires_at: None,
            }),
            31,
        )
        .unwrap();
    runtime.leaving(1, 10);
    assert!(runtime.capture(1, 10).iter().next().is_none());
    assert!(Runtime::default().capture(1, 10).iter().next().is_none());
}
