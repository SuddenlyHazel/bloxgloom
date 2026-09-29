use super::*;

#[test]
fn updates_are_owned_typed_atomic_and_coalesced() {
    let definitions: Vec<Definition> = serde_json::from_str(
        r#"[
        {"name":"gain","kind":"float","default":0.5,"min":0,"max":1},
        {"name":"color","kind":"color","default":[1,1,1,1]}
    ]"#,
    )
    .unwrap();
    let mut state = State::default();
    state.register("demo:surface", &definitions).unwrap();
    let update = |resource: &str, name: &str, value| Update {
        resource: resource.into(),
        name: name.into(),
        value,
    };
    let gain = update("demo:surface", "gain", Value::Scalar(0.25));
    for invalid in [
        update("other:surface", "gain", Value::Scalar(0.5)),
        update("demo:missing", "gain", Value::Scalar(0.5)),
        update("demo:surface", "gain", Value::Bool(true)),
        update("demo:surface", "gain", Value::Scalar(f32::NAN)),
        update("demo:surface", "gain", Value::Scalar(2.0)),
        update("demo:surface", "color", Value::Vector(vec![1.0; 3])),
    ] {
        assert!(state.apply("demo", &[gain.clone(), invalid]).is_err());
        assert!(state.take_updates().is_empty());
    }
    state.apply("demo", &[gain]).unwrap();
    state
        .apply(
            "demo",
            &[update("demo:surface", "gain", Value::Scalar(0.75))],
        )
        .unwrap();
    let updates = state.take_updates();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].value, Value::Scalar(0.75));
    assert!(state.take_updates().is_empty());
}

#[test]
fn schema_rejects_duplicates_bad_defaults_and_resource_limits() {
    let definition: Definition =
        serde_json::from_str(r#"{"name":"enabled","kind":"bool","default":true}"#).unwrap();
    assert!(defaults(&vec![definition.clone(); 9]).is_err());
    assert!(defaults(&[definition.clone(), definition.clone()]).is_err());
    let mut bad = definition;
    bad.default = Value::Scalar(1.0);
    assert!(defaults(&[bad]).is_err());
}
