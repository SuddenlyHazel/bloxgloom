//! Frozen declarations reject invalid bounds and shared block lifecycle ownership.
use super::*;

#[test]
fn luau_anchored_registration_rejects_unknown_sources_fields_and_out_of_range_bounds() {
    let (main, behavior) = sources();
    for (before, after) in [
        (
            "module = \"counter:behavior\"",
            "module = \"counter:missing\"",
        ),
        ("entity = \"counter:post\"", "entity = \"other:post\""),
        ("interval = 20", "interval = 0"),
        ("interval = 20", "interval = 20, surprise = true"),
        ("removal_refund = 2", "removal_refund = 4"),
        ("schema_version = 1", "schema_version = 65536"),
        ("max_state_bytes = 9", "max_state_bytes = 65537"),
        ("max_public_bytes = 5", "max_public_bytes = 4097"),
        ("{offset = {0, 1, 0}", "{offset = {0, 17, 0}"),
    ] {
        let fixture = Fixture::new();
        let source = main.replace(before, after);
        assert_ne!(source, main, "case must exercise its specified field");
        package(&fixture, &source, &behavior, true);
        assert!(
            fixture.open().is_err(),
            "invalid declaration admitted: {after}"
        );
        assert!(!fixture.0.join("save").exists());
    }
}

#[test]
fn luau_anchored_then_storage_or_machine_same_block_rejects_caught_ownership_collision() {
    let (main, behavior) = sources();
    for other in [
        "h.register_storage('counter:store','counter:block','Counter store',9,3)",
        "h.register_machine{entity='counter:machine',block='counter:block',module='counter:behavior',schema=1,revision=1,interval=20,title='COUNTER',hint='TEST',recipe={key='counter:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=1,pulses=1}}",
    ] {
        let fixture = Fixture::new();
        let prefix = main.trim_end().strip_suffix("end").unwrap();
        let source = format!("{prefix}\n pcall(function() {other} end)\nend\n");
        package(&fixture, &source, &behavior, true);
        let manifest = fixture.0.join("packages/counter/package.txt");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(manifest,format!("{text}requires bloxgloom:storage/v1\nrequires bloxgloom:inventory_screens/v1\nrequires bloxgloom:machines/v1\nrequires bloxgloom:actions/v1\n")).unwrap();
        assert!(
            fixture.open().is_err(),
            "caught ownership collision must poison startup"
        );
        assert!(!fixture.0.join("save").exists());
    }
}
