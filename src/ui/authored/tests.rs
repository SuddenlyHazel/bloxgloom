use super::*;
use crate::server::client_bundle::{CacheKey, ClientBundle};
use sha2::{Digest, Sha256};
use std::sync::Arc;

type Assets = BTreeMap<String, (u32, Vec<u8>)>;

fn sample() -> Arc<ClientBundle> {
    Arc::clone(
        crate::server::PackageSnapshot::discover(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages"),
        )
        .unwrap()
        .client_bundle(),
    )
}

fn encode(assets: &Assets) -> Vec<u8> {
    encode_source(
        assets,
        include_bytes!("../../../fixtures/packages/uidemo/client/view.luau"),
        1,
    )
}

fn encode_source(assets: &Assets, source: &[u8], side: usize) -> Vec<u8> {
    fn count(out: &mut Vec<u8>, value: usize) {
        out.extend_from_slice(&(value as u32).to_le_bytes());
    }
    fn field(out: &mut Vec<u8>, value: &[u8]) {
        count(out, value.len());
        out.extend_from_slice(value);
    }
    let mut bytes = b"BGCLIENT\x07".to_vec();
    count(&mut bytes, 2);
    // An exact direct dependency still grants no cross-package UI authority.
    field(&mut bytes, b"other");
    field(&mut bytes, b"1.0.0");
    for _ in 0..3 {
        count(&mut bytes, 0);
    }
    field(&mut bytes, b"uidemo");
    field(&mut bytes, b"1.0.0");
    count(&mut bytes, 1);
    field(&mut bytes, b"other");
    field(&mut bytes, b"1.0.0");
    count(&mut bytes, 1);
    field(&mut bytes, b"view");
    count(&mut bytes, side);
    field(&mut bytes, source);
    count(&mut bytes, assets.len());
    for (name, (kind, data)) in assets {
        field(&mut bytes, name.as_bytes());
        count(&mut bytes, *kind as usize);
        field(&mut bytes, data);
    }
    count(&mut bytes, 0);
    bytes
}

fn dynamic(source: &str, side: usize) -> Session {
    let bundle = sample();
    let assets = &bundle.packages()["uidemo"].ui_assets;
    let bytes = encode_source(assets, source.as_bytes(), side);
    let bundle =
        ClientBundle::decode_verify(&bytes, CacheKey::from_bytes(Sha256::digest(&bytes).into()))
            .unwrap();
    let mut session = Session::new(Arc::clone(bundle.ui().unwrap()));
    session.resize(640, 360, 1.0);
    session.tab(false);
    session.tab(false);
    session
}

#[test]
fn action_callback_cannot_supply_target_or_authorization_claims() {
    let mut session = dynamic(
        "return function(_) return {{op='action', key='uidemo:store', target={10000,80,0}, entity=99, entity_revision=999, slot=12, inventory_revision=999, permission=true}} end",
        1,
    );
    session.activate();
    session.wait_for_presentation().unwrap();
    // Only the semantic key crosses the presentation boundary. All other
    // fields are ignored, not forwarded to request composition or the wire.
    assert_eq!(session.take_action().as_deref(), Some("uidemo:store"));
    assert!(session.take_action().is_none());
}

#[test]
fn egui_input_dispatches_unicode_text_and_preserves_busy_value() {
    let mut session = dynamic(
        "return function(input) return {{op='text',node='uidemo:welcome/title',value=input.value}} end",
        1,
    );
    session.apply_egui(EguiIntent::Input(4, "café".into()));
    assert_eq!(session.inputs[4], "café");
    session.apply_egui(EguiIntent::Input(4, "discarded".into()));
    assert_eq!(session.inputs[4], "café");
    session.wait_for_presentation().unwrap();
    assert_eq!(session.text_at(1), "café");
}

#[test]
fn replica_module_cannot_write_another_packages_document() {
    let startup = crate::client::startup::State {
        replica: Some(Arc::new(crate::client::presentation::Script {
            module: "other@1.0.0:replica".into(),
            source: "return function(_) return {{op='text',node='uidemo:welcome/title',value='FORGED'}} end".into(),
        })),
        ..Default::default()
    };
    let mut session = Session::with_startup(Arc::clone(sample().ui().unwrap()), startup);
    let before = session.text_at(1).to_owned();
    session.replica_event("replica:inventory", "revision=1".into());
    assert!(
        session
            .wait_for_presentation()
            .unwrap_err()
            .contains("invalid local-ui target")
    );
    assert_eq!(session.text_at(1), before);
}

#[test]
fn worker_dispatches_click_and_input_with_bounded_admission_and_explicit_state() {
    let mut session = Session::new(Arc::clone(sample().ui().unwrap()));
    session.resize(640, 360, 1.0);
    session.tab(false);
    session.edit(false, Some("!"));
    assert_eq!(session.pending, Some(1));
    // No polling: even a completed-but-unapplied result retains backpressure.
    session.edit(false, Some("lost"));
    assert_eq!(session.inputs[4], "Moss & stone!");
    assert_eq!(session.event_status(), "BUSY: INPUT PAUSED");
    session.wait_for_presentation().unwrap();
    assert_eq!(session.text_at(1), "Garden: Moss & stone!");
    let rect = session.clips[5];
    session.click(rect.x + 1.0, rect.y + 1.0);
    session.activate(); // Repeated click while pending is rejected, not replayed.
    assert_eq!(session.sequence, 2);
    session.wait_for_presentation().unwrap();
    assert_eq!(session.take_action().as_deref(), Some("uidemo:store"));
    session.action_submitted(42);
    session.activate();
    assert_eq!(session.sequence, 2);
    session.action_result(42, false, "needs stick");
    assert_eq!(
        session.feedback.as_deref(),
        Some("SERVER DENIED: needs stick")
    );
    session.activate();
    session.wait_for_presentation().unwrap();
    assert_eq!(session.take_action().as_deref(), Some("uidemo:store"));
}

#[test]
fn switching_document_discards_pending_result_and_resets_state_not_event_sequence() {
    let mut session = dynamic(
        include_str!("../../../fixtures/packages/uidemo/client/view.luau"),
        2,
    );
    session.activate();
    session.next_document();
    assert_eq!(session.pending, Some(1));
    session.wait_for_presentation().unwrap();
    assert_eq!(session.text_at(1), "Welcome to the garden");
    assert_eq!(session.text_at(5), "Store one stick [server]");
    assert!(session.state.is_empty());
    assert!(session.is_visible(3));
    session.resize(640, 360, 1.0);
    session.tab(false);
    session.tab(false);
    session.activate();
    session.wait_for_presentation().unwrap();
    assert_eq!(session.sequence, 2);
    assert_eq!(session.take_action().as_deref(), Some("uidemo:store"));
}

#[test]
fn late_authoritative_result_does_not_repaint_a_reset_document() {
    let mut session = Session::new(Arc::clone(sample().ui().unwrap()));
    session.resize(640, 360, 1.0);
    session.tab(false);
    session.tab(false);
    session.activate();
    session.wait_for_presentation().unwrap();
    assert_eq!(session.take_action().as_deref(), Some("uidemo:store"));
    session.action_submitted(42);
    session.next_document(); // Even cycling back to the same document invalidates feedback.
    session.action_result(42, true, "");
    assert_eq!(session.feedback, None);
    assert!(session.in_flight.is_none());
}

#[test]
fn handlers_fail_closed_atomically_with_module_attribution_and_sandbox_limits() {
    for source in [
        "return function(i) return {{op='text',node='uidemo:welcome/title',value='partial'}, {op='text',node='other:welcome/title',value='escape'}} end",
        "return function(i) return {{op='text',node='uidemo:welcome/title',value='partial'}, {op='action',key='other:give'}} end",
        "return function(i) return {{op='action',key='uidemo:store'}, {op='action',key='uidemo:store'}} end",
        "return function(i) return {{op='text',node='uidemo:welcome/title',value='partial'}, {op='text',node='uidemo:other/title',value='escape'}} end",
        "return function(i) return {{op='world',value='stone'}} end",
        "return function(i) return {{op='visible',node='uidemo:welcome/title',value='false'}} end",
        "return function(i) return {{op='state',value=42}} end",
        "return function(i) return {{op='state',value=string.rep('x',129)}} end",
        "return function(i) return {hidden={op='state',value='bad'}} end",
        "return function(i) local t={} for n=1,17 do t[n]={op='state',value='bad'} end return t end",
        "return function(i) error('broken handler') end",
        "return function(i) while true do end end",
        "return function(i) return string.rep('x',16000000) end",
        "return function(i) return require('other:module') end",
    ] {
        let mut session = dynamic(source, 1);
        session.activate();
        let error = session.wait_for_presentation().unwrap_err();
        assert!(error.contains("uidemo:view"), "{error}");
        assert_eq!(session.text_at(1), "Welcome to the garden");
        assert!(session.state.is_empty());
        session.activate();
        assert!(session.pending.is_none());
        assert_eq!(session.sequence, 1);
    }
}

#[test]
fn shared_handler_has_no_native_authority_and_hidden_ancestor_removes_focus() {
    let mut session = dynamic(
        "return function(i) assert(io == nil and os == nil and debug == nil and require == nil and print == nil and getfenv == nil and setfenv == nil and game == nil and world == nil and inventory == nil); assert(i.sequence == 1 and i.event == 'uidemo:store'); return {{op='visible',node='uidemo:welcome/root',value=false}} end",
        2,
    );
    session.activate();
    session.wait_for_presentation().unwrap();
    assert!(session.focused.is_none());
    session.tab(false);
    assert!(session.focused.is_none());
    assert!(!session.is_visible(5));
}

#[test]
fn verified_bundle_requires_explicit_supported_owned_capability_and_module() {
    let bundle = sample();
    let assets = &bundle.packages()["uidemo"].ui_assets;
    for (key, value) in [
        ("capability", "world"),
        ("module", "other:view"),
        ("module", "uidemo:main"),
        ("module", "uidemo:missing"),
    ] {
        let assets = change_json(assets, "welcome", |v| v["presentation"][key] = value.into());
        assert!(decode(&assets).is_err());
    }
}

fn decode(assets: &Assets) -> std::result::Result<ClientBundle, String> {
    let bytes = encode(assets);
    ClientBundle::decode_verify(&bytes, CacheKey::from_bytes(Sha256::digest(&bytes).into()))
        .map_err(|error| format!("{error:?}"))
}

fn change_json(assets: &Assets, key: &str, change: impl FnOnce(&mut serde_json::Value)) -> Assets {
    let mut assets = assets.clone();
    let bytes = &mut assets.get_mut(key).unwrap().1;
    let mut value = serde_json::from_slice(bytes).unwrap();
    change(&mut value);
    *bytes = serde_json::to_vec(&value).unwrap();
    assets
}

#[test]
fn verified_discovery_freezes_resources_and_tampering_fails_before_publication() {
    let original = sample();
    let bundle = ClientBundle::decode_verify(original.bytes(), original.cache_key()).unwrap();
    let resources = bundle.ui().unwrap();
    assert_eq!(resources.documents[0].id, "uidemo:welcome");
    assert_eq!(resources.documents[0].nodes.len(), 6);
    assert_eq!(resources.fonts["uidemo:body"].len(), 95);
    assert_eq!(resources.images["uidemo:icon"].width, 16.0);
    assert!(resources.pixels.iter().any(|&p| p != 0));
    assert_eq!(bundle.packages()["uidemo"].sources.len(), 1);
    let mut tampered = original.bytes().to_vec();
    let at = tampered.windows(7).position(|s| s == b"Welcome").unwrap();
    tampered[at] = b'w';
    assert!(ClientBundle::decode_verify(&tampered, original.cache_key()).is_err());
    assert!(decode(&original.packages()["uidemo"].ui_assets).is_ok());
}

#[test]
fn verified_bundle_rejects_cross_package_references_and_unsupported_features() {
    let bundle = sample();
    let assets = &bundle.packages()["uidemo"].ui_assets;
    for (node, field, value) in [
        (1, "style", "other:label"),
        (2, "image", "other:icon"),
        (4, "event", "other:changed"),
        (4, "kind", "scroll"),
    ] {
        let changed = change_json(assets, "welcome", |v| {
            v["nodes"][node][field] = value.into()
        });
        assert!(decode(&changed).is_err(), "{field}: {value}");
    }
    let unicode = change_json(assets, "welcome", |v| {
        v["nodes"][4]["text"] = "Moss & café".into();
    });
    assert!(decode(&unicode).is_ok());
    for (field, value) in [
        ("font", serde_json::json!("other:body")),
        ("height", serde_json::json!(0)),
        ("on_click", serde_json::json!("evil()")),
    ] {
        let changed = change_json(assets, "label", |v| v[field] = value);
        assert!(decode(&changed).is_err(), "{field}");
    }
}

#[test]
fn verified_bundle_bounds_tree_depth_count_text_and_input_capacity() {
    let bundle = sample();
    let assets = &bundle.packages()["uidemo"].ui_assets;
    for changed in [
        change_json(assets, "welcome", |v| v["nodes"][1]["parent"] = 1.into()),
        change_json(assets, "welcome", |v| v["nodes"][2]["parent"] = 1.into()),
        change_json(assets, "welcome", |v| v["nodes"][1]["id"] = "root".into()),
        change_json(assets, "welcome", |v| {
            v["nodes"][1]["text"] = "x".repeat(129).into()
        }),
        change_json(assets, "welcome", |v| {
            v["nodes"] = serde_json::json!((0..17).map(|i| serde_json::json!({"id":format!("n{i}"),"kind":"panel","style":"uidemo:panel","parent":if i==0 {None} else {Some(i-1)}})).collect::<Vec<_>>());
        }),
        change_json(assets, "welcome", |v| {
            v["nodes"] = serde_json::json!((0..65).map(|i| serde_json::json!({"id":format!("n{i}"),"kind":"panel","style":"uidemo:panel","parent":if i==0 {None} else {Some(0)}})).collect::<Vec<_>>());
        }),
        change_json(assets, "welcome", |v| {
            let root = v["nodes"][0].clone();
            let mut nodes = vec![root];
            nodes.extend((0..33).map(|i| serde_json::json!({"id":format!("n{i}"),"parent":0,"kind":"input","style":"uidemo:field"})));
            v["nodes"] = nodes.into();
        }),
    ] {
        assert!(decode(&changed).is_err());
    }
    let mut oversized = assets.clone();
    oversized.get_mut("welcome").unwrap().1 = vec![b' '; 16 * 1024 + 1];
    assert!(decode(&oversized).is_err());
}

#[test]
fn verified_bundle_bounds_image_dimensions_font_expansion_and_document_count() {
    let mut assets = sample().packages()["uidemo"].ui_assets.clone();
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 257, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255; 257 * 4])
            .unwrap();
    }
    assets.get_mut("icon").unwrap().1 = png_bytes;
    assert!(decode(&assets).is_err());
    let mut assets = sample().packages()["uidemo"].ui_assets.clone();
    let font = &mut assets.get_mut("body").unwrap().1;
    let face = ttf_parser::Face::parse(font, 0).unwrap();
    let maxp = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"maxp"))
        .unwrap();
    let at = maxp.as_ptr() as usize - font.as_ptr() as usize;
    font[at + 4..at + 6].copy_from_slice(&129u16.to_be_bytes());
    assert!(decode(&assets).is_err());
    let mut assets = sample().packages()["uidemo"].ui_assets.clone();
    for i in 0..8 {
        assets.insert(format!("document{i}"), assets["welcome"].clone());
    }
    assert!(decode(&assets).is_err());
}

#[test]
fn live_session_uses_taffy_geometry_for_clipped_focus_and_bounded_local_editing() {
    let bundle = sample();
    // Preserve coverage of legacy, inert documents without opt-in capability.
    let assets = change_json(&bundle.packages()["uidemo"].ui_assets, "welcome", |v| {
        v.as_object_mut().unwrap().remove("presentation");
    });
    let bundle = decode(&assets).unwrap();
    let mut session = Session::new(Arc::clone(bundle.ui().unwrap()));
    for (width, height, scale) in [(1280, 720, 1.0), (640, 360, 1.0), (640, 360, 2.0)] {
        session.resize(width, height, scale);
        assert_eq!(session.rects.len(), 6);
        assert!(session.rects[1].y < session.rects[2].y);
        assert!(session.rects[0].x >= 0.0);
        for clip in &session.clips {
            assert!(clip.x + clip.width <= width as f32);
            assert!(clip.y + clip.height <= height as f32);
        }
    }
    session.resize(640, 360, 1.0);
    session.tab(false);
    assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
    assert_eq!(session.event(), Some("uidemo:name-changed"));
    session.edit(true, None);
    session.edit(false, Some("!"));
    assert_eq!(session.inputs[4], "Moss & ston!");
    session.edit(false, Some(&"a".repeat(200)));
    assert_eq!(session.inputs[4].len(), MAX_TEXT);
    session.tab(false);
    assert_eq!(session.focused_id(), Some("uidemo:welcome/confirm"));
    session.edit(false, Some("ignored"));
    assert!(session.inputs[5].is_empty());
    session.tab(true);
    assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
    let clip = session.clips[5];
    session.click(clip.x + 1.0, clip.y + 1.0);
    assert_eq!(session.focused_id(), Some("uidemo:welcome/confirm"));
    session.click(-1.0, -1.0);
    assert!(session.focused_id().is_none());
    // Fully clipped controls cannot retain/acquire focus on a tiny viewport.
    session.resize(40, 40, 1.0);
    session.tab(false);
    assert!(session.focused_id().is_none());
    session.next_document();
    assert_eq!(session.inputs[4], "Moss & stone");
    assert!(session.focused_id().is_none());
    // A new session never carries text/focus across bundle/reconnect identity.
    let fresh = Session::new(Arc::clone(bundle.ui().unwrap()));
    assert!(fresh.focused_id().is_none());
    assert_eq!(fresh.inputs[4], "Moss & stone");
}
