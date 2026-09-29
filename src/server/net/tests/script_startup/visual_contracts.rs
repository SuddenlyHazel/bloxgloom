//! Versioned shaders and typed presentation through real join/switch/restart.
use super::*;

fn copy(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    copy(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/visual-packages/prism"),
        &fixture.0.join("packages/prism"),
    );
    fixture
}
fn scalar(updates: &[crate::render::parameters::Update]) -> f32 {
    let update = updates
        .iter()
        .find(|u| u.resource == "prism:mix" && u.name == "strength")
        .unwrap();
    let crate::render::parameters::Value::Scalar(value) = update.value else {
        panic!("wrong type")
    };
    value
}

#[test]
fn negotiated_visuals_parameters_switch_and_restart_without_global_state() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let original = fixture();
    let other = fixture();
    let path = other.0.join("packages/prism/client/startup.luau");
    let source = std::fs::read_to_string(&path)
        .unwrap()
        .replace("0.35", "0.75");
    std::fs::write(path, source).unwrap();
    let state = original.open().unwrap();
    let fingerprint = state.world.catalog().fingerprint();
    let mut identity = None;
    gameplay::serve(Box::new(state), |first| {
        gameplay::serve(Box::new(other.open().unwrap()), |second| {
            for (address, expected) in [(first, 0.35), (second, 0.75), (first, 0.35)] {
                let bundle = crate::client::connect_bundle_probe(&address.to_string(), 0x70701)
                    .unwrap()
                    .unwrap();
                let catalog = bundle.session_catalog().unwrap();
                assert_eq!(catalog.fingerprint(), fingerprint);
                let material = bundle.material().unwrap().resolve(&catalog).unwrap();
                assert_eq!(material.materials[0].layers.len(), 2);
                let effect = bundle.effect().unwrap();
                assert_eq!(effect.passes[0].owner, "prism:grade");
                assert_eq!(effect.passes[1].owner, "prism:mix");
                if address == first {
                    identity = Some(bundle.cache_key());
                }
                crate::client::connect_visual_probe(&address.to_string(), 0x70702, |visual| {
                    assert_eq!(scalar(&visual.take_parameters()), expected);
                    visual.entities(
                        vec![crate::client::presentation::EntityView {
                            id: 17,
                            key: "bloxgloom:mossbun".into(),
                            position: [0.5, 80.0, 0.5],
                            revision: 1,
                            motion_revision: 2,
                            public: vec![0, 255],
                        }],
                        1,
                    );
                    visual.wait_for_test().unwrap();
                    assert_eq!(scalar(&visual.take_parameters()), 0.265625);
                })
                .unwrap();
            }
        });
    });
    let restarted = original.open().unwrap();
    assert_eq!(restarted.world.catalog().fingerprint(), fingerprint);
    gameplay::serve(Box::new(restarted), |address| {
        let bundle = crate::client::connect_bundle_probe(&address.to_string(), 0x70703)
            .unwrap()
            .unwrap();
        assert_eq!(Some(bundle.cache_key()), identity);
        crate::client::connect_visual_probe(&address.to_string(), 0x70704, |visual| {
            assert_eq!(scalar(&visual.take_parameters()), 0.35);
        })
        .unwrap();
    });
}

#[test]
fn invalid_typed_startup_parameter_refuses_real_join_with_source_identity() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = fixture();
    std::fs::write(
        fixture.0.join("packages/prism/client/startup.luau"),
        "return function(h) h.set_parameter('prism:mix','strength',true) end",
    )
    .unwrap();
    gameplay::serve(Box::new(fixture.open().unwrap()), |address| {
        let error = crate::client::connect_bundle_probe(&address.to_string(), 0x70705).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("prism@1.0.0:client_startup"), "{message}");
        assert!(message.contains("parameter type mismatch"), "{message}");
    });
}
