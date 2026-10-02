use super::*;
#[test]
fn native_glb_gpu_preview_changes_with_authored_pose_variants_and_color_modes() {
    let controls = serde_json::from_slice(include_bytes!(
        "../../../fixtures/authored-model/controls.json"
    ))
    .unwrap();
    let model = Model::from_glb(
        include_bytes!("../../../fixtures/authored-model/model.glb"),
        controls,
    )
    .unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-native-model-gpu-{}-{stamp}.png",
        std::process::id()
    ));
    let render = |clip: Option<&str>, seconds: f32, look: Look| {
        pollster::block_on(render_async(
            &model,
            &model.sample(clip, seconds).unwrap(),
            model.appearance(&look).unwrap(),
            &path,
        ))
        .unwrap()
    };
    let rest = render(None, 0.0, Look::default());
    let posed = render(Some("nod"), 0.4, Look::default());
    let eyes = render(
        None,
        0.0,
        serde_json::from_str(r#"{"variants":{"eyes":"sleepy"}}"#).unwrap(),
    );
    let hat = render(
        None,
        0.0,
        serde_json::from_str(r#"{"layers":{"hat":true}}"#).unwrap(),
    );
    let multiply = render(
        None,
        0.0,
        serde_json::from_str(r#"{"tints":{"body":{"rgb":[230,110,145],"mode":"multiply"}}}"#)
            .unwrap(),
    );
    let replace = render(
        None,
        0.0,
        serde_json::from_str(r#"{"tints":{"body":{"rgb":[230,110,145],"mode":"replace"}}}"#)
            .unwrap(),
    );
    let changed = |a: &[u8], b: &[u8]| {
        a.chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
    };
    assert!(
        changed(&rest, &posed) > 1000,
        "authored pose must move visible geometry"
    );
    assert!(
        changed(&rest, &eyes) > 200,
        "variant selection must change the eyes"
    );
    assert!(
        changed(&rest, &hat) > 500,
        "optional layers must change the silhouette"
    );
    assert!(
        changed(&rest, &multiply) > 10000,
        "color parameters must reach the fragment shader"
    );
    assert!(
        changed(&multiply, &replace) > 1000,
        "replace must differ from texture multiplication"
    );
    std::fs::remove_file(path).unwrap();
}
