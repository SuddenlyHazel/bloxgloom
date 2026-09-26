use super::*;

#[test]
fn mossbun_adapter_uses_the_negotiated_catalog_assignment() {
    let local = Catalog::builtins();
    let mut manifest = crate::content::ContentManifest::from_catalog(&local);
    manifest
        .entries
        .iter_mut()
        .find(|e| e.kind == b'E' && e.key == "bloxgloom:mossbun")
        .unwrap()
        .id = 71_234;
    manifest.entries.sort_unstable_by_key(|e| (e.kind, e.id));
    let catalog = manifest.resolve_catalog(&local).unwrap();
    let mut bun = player(91, 1);
    bun.entity_type = EntityTypeId(71_234);
    bun.payload = vec![0, 0];
    let visuals = EntityClientRegistry::builtins(&catalog)
        .project(&BTreeMap::from([(91, bun)]))
        .unwrap();
    assert_eq!(visuals.len(), 1);
    assert_eq!(visuals[0].model, crate::render::AvatarModel::Mossbun);
}

#[test]
fn mossbun_adapter_validates_payload_and_tracks_snapshot_removal_and_eviction() {
    let catalog = Catalog::builtins();
    let registry = EntityClientRegistry::builtins(&catalog);
    let mut bun = player(91, 1);
    bun.entity_type = crate::content::MOSSBUN_ENTITY_TYPE;
    bun.payload = vec![1, 1];
    let projected = registry
        .project(&BTreeMap::from([(91, bun.clone())]))
        .unwrap();
    assert_eq!(projected[0].model, crate::render::AvatarModel::Mossbun);
    assert_eq!(projected[0].pose[0], std::f32::consts::FRAC_PI_2);
    for payload in [vec![], vec![4, 0], vec![0, 4], vec![0; 3]] {
        let mut invalid = bun.clone();
        invalid.payload = payload;
        assert!(registry.project(&BTreeMap::from([(91, invalid)])).is_err());
    }
    for position in [[f32::NAN, 2.0, 0.0], [1_000_000.0, 2.0, 0.0]] {
        let mut invalid = bun.clone();
        invalid.location = PublicEntityLocation::Mobile { position };
        assert!(registry.project(&BTreeMap::from([(91, invalid)])).is_err());
    }
    let mut replicas = Replicas::default();
    let mut chunks = HashMap::new();
    let (start, pages) = snapshot(key(0), 1, 1, vec![vec![bun.clone()]], &catalog);
    accept(
        &mut replicas,
        ServerMessage::WorldSnapshotStart(start),
        &catalog,
        &mut chunks,
    );
    for page in pages {
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(page),
            &catalog,
            &mut chunks,
        );
    }
    assert_eq!(replicas.visual_avatars(glam::Vec3::ZERO, None).len(), 1);
    accept(
        &mut replicas,
        ServerMessage::WorldCommitPart(WorldCommitPart {
            commit_id: 1,
            part_index: 0,
            part_count: 1,
            key: key(0),
            epoch: 1,
            block_from: 0,
            block_to: 0,
            entity_from: 1,
            entity_to: 2,
            blocks: Vec::new(),
            entities: vec![PublicEntityChange::Remove {
                id: 91,
                revision: 2,
            }],
        }),
        &catalog,
        &mut chunks,
    );
    assert!(replicas.visual_avatars(glam::Vec3::ZERO, None).is_empty());
    let (start, pages) = snapshot(key(0), 2, 3, vec![vec![bun]], &catalog);
    accept(
        &mut replicas,
        ServerMessage::WorldSnapshotStart(start),
        &catalog,
        &mut chunks,
    );
    for page in pages {
        accept(
            &mut replicas,
            ServerMessage::EntitySnapshotPage(page),
            &catalog,
            &mut chunks,
        );
    }
    replicas.retain(|_| false);
    assert!(replicas.visual_avatars(glam::Vec3::ZERO, None).is_empty());
}
