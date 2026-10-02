use super::*;
#[test]
fn material_batches_preserve_joint_and_tint_ids_and_skip_hidden_variants() {
    let controls = serde_json::from_slice(include_bytes!(
        "../../../../assets/models/player/master/controls.json"
    ))
    .unwrap();
    let model = Model::from_glb(
        include_bytes!("../../../../assets/models/player/master/model.glb"),
        controls,
    )
    .unwrap();
    let (vertices, batches) = pack(&model);
    assert_eq!(vertices.len(), 23368);
    let appearance = model.appearance(&Default::default()).unwrap();
    let active: Vec<_> = batches
        .iter()
        .map(|b| b.visible_indices(&appearance.visible))
        .filter(|i| !i.is_empty())
        .collect();
    assert_eq!(
        active.len(),
        2,
        "hundreds of cubes must reduce to two material draws"
    );
    let actual: usize = active.iter().map(Vec::len).sum();
    let expected: usize = model
        .primitives
        .iter()
        .filter(|p| appearance.visible[p.node])
        .map(|p| p.indices.len())
        .sum();
    assert_eq!(actual, expected);
    for indices in active {
        for i in indices {
            let vertex = vertices[i as usize];
            assert!(appearance.visible[model.primitives[vertex.tint as usize].node]);
            assert!((vertex.joints[0] as usize) < model.bindings.len());
        }
    }
}
