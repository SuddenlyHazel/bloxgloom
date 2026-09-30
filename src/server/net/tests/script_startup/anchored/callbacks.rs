//! Boundary values and callback input/reply contracts through installed packages.
use super::*;
use bloxgloom_host_api::anchored as api;

#[test]
fn luau_anchored_callbacks_accept_full_registered_binary_limits_and_immutable_inputs() {
    let fixture = Fixture::new();
    let (main, _) = sources();
    let main = main
        .replace("max_state_bytes = 9", "max_state_bytes = 65536")
        .replace("max_public_bytes = 5", "max_public_bytes = 4096");
    let source = "return function(e)
        assert(not pcall(function() e.kind = 'bad' end))
        if e.kind == 'Initialize' then
            assert(not pcall(function() e.anchor[1] = 9 end))
            return string.rep(string.char(0,255), 32768)
        elseif e.kind == 'Validate' then assert(#e.state == 65536); return true
        elseif e.kind == 'Public' then return string.sub(e.state,1,4096)
        elseif e.kind == 'Interact' then return e.state
        elseif e.kind == 'React' then
            assert(not pcall(function() e.anchor[1] = 9 end))
            assert(not pcall(function() e.cells[1] = {} end))
            assert(not pcall(function() e.cells[1].solid = false end))
            assert(not pcall(function() e.cells[1].offset[2] = 9 end))
            assert(e.cells[1].state == 'bloxgloom:stone' and e.cells[1].solid)
            return nil
        elseif e.kind == 'Refund' then return nil end
        error('unknown event')
    end";
    package(&fixture, &main, source, true);
    let state = fixture.open().unwrap();
    let id = state
        .world
        .catalog()
        .entity_type_id_by_key("counter:post")
        .unwrap();
    let definition = state.world.catalog().anchored_entity(id).unwrap();
    let payload = definition.behavior.initialize([0, 80, 2]).unwrap();
    let encoded = definition.behavior.encode(&payload).unwrap();
    assert_eq!(encoded.len(), 65536);
    assert_eq!(&encoded[..4], &[0, 255, 0, 255]);
    let decoded = definition.behavior.decode(&encoded).unwrap();
    assert_eq!(definition.behavior.public(&decoded).unwrap().len(), 4096);
    assert_eq!(
        definition
            .behavior
            .encode(
                &definition
                    .behavior
                    .interact(&decoded, b"increment")
                    .unwrap()
            )
            .unwrap(),
        encoded
    );
    let cells = [api::Cell {
        offset: [0, -1, 0],
        state: "bloxgloom:stone",
        solid: true,
    }];
    assert!(matches!(
        definition
            .behavior
            .react(&api::Context {
                anchor: [0, 80, 2],
                tick: u64::MAX,
                state: &decoded,
                cells: &cells
            })
            .unwrap(),
        api::Reaction::Keep
    ));
    for cause in [
        api::RemovalCause::Broken,
        api::RemovalCause::Reaction,
        api::RemovalCause::WorldEdit,
    ] {
        assert_eq!(definition.behavior.refund(&decoded, cause, 2).unwrap(), 2);
    }
}

#[test]
fn luau_anchored_rejects_failed_validation_oversized_public_and_ambiguous_reaction() {
    let (main, behavior) = sources();
    for (kind, replacement) in [
        ("Validate", "return false"),
        ("Public", "return string.rep('x',6)"),
        ("React", "return {state=e.state,remove=true}"),
    ] {
        let fixture = Fixture::new();
        let source = format!(
            "local original = (function()\n{behavior}\nend)()\nreturn function(e) if e.kind == '{kind}' then {replacement} end return original(e) end"
        );
        package(&fixture, &main, &source, true);
        let state = fixture.open().unwrap();
        let id = state
            .world
            .catalog()
            .entity_type_id_by_key("counter:post")
            .unwrap();
        let definition = state.world.catalog().anchored_entity(id).unwrap();
        let payload = definition.behavior.initialize([0, 80, 2]).unwrap();
        match kind {
            "Validate" => assert!(
                definition
                    .behavior
                    .decode(&definition.behavior.encode(&payload).unwrap())
                    .is_err()
            ),
            "Public" => assert!(definition.behavior.public(&payload).is_err()),
            "React" => assert!(
                definition
                    .behavior
                    .react(&api::Context {
                        anchor: [0, 80, 2],
                        tick: 123,
                        state: &payload,
                        cells: &[]
                    })
                    .is_err()
            ),
            _ => unreachable!(),
        }
    }
}
