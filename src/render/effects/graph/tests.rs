use super::*;

fn passes() -> Vec<Pass> {
    let snapshot = crate::server::PackageSnapshot::discover(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/visual-packages"),
    )
    .unwrap();
    snapshot.client_bundle().effect().unwrap().passes.clone()
}

#[test]
fn inputs_override_order_and_invalid_graphs_keep_resource_identity() {
    let mut valid = passes();
    valid.reverse();
    let prepared = prepare(valid.clone()).unwrap();
    assert_eq!(prepared.passes[0].owner, "prism:grade");
    assert_eq!(prepared.passes[1].owner, "prism:mix");
    let mut missing = valid.clone();
    missing[0].descriptor.inputs[0] = "prism:missing".into();
    assert!(
        prepare(missing)
            .unwrap_err()
            .contains("prism:mix: missing effect input")
    );
    let mut cycle = valid.clone();
    cycle[1].descriptor.inputs[0] = "prism:color".into();
    assert!(prepare(cycle).unwrap_err().contains("cyclic"));
    let mut duplicate = valid.clone();
    duplicate[1].descriptor.output = duplicate[0].descriptor.output.clone();
    assert!(prepare(duplicate).unwrap_err().contains("duplicate"));
    let mut finals = valid.clone();
    finals[1].descriptor.final_output = true;
    assert!(prepare(finals).unwrap_err().contains("exactly one final"));
    let mut disconnected = valid;
    disconnected[0].descriptor.inputs = vec![SCENE.into()];
    disconnected[0].descriptor.after.clear();
    assert!(prepare(disconnected).unwrap_err().contains("disconnected"));
}

#[test]
fn after_dependencies_use_the_same_cycle_and_missing_checks() {
    let mut valid = passes();
    valid[0].descriptor.after.push("prism:mix".into());
    assert!(prepare(valid).unwrap_err().contains("cyclic"));
    let mut missing = passes();
    missing[0].descriptor.after.push("prism:unknown".into());
    assert!(
        prepare(missing)
            .unwrap_err()
            .contains("prism:grade: missing effect dependency")
    );
}
