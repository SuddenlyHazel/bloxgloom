use super::*;

fn fixture() -> (State, Mesh) {
    let catalog = Arc::new(Catalog::builtins());
    let mut state = State::new(catalog.clone());
    state.session = 7;
    let key = TileKey {
        level: 1,
        x: -1,
        z: 0,
    };
    state.wanted.push(key);
    state.builds.insert(key, 5);
    let tile = LodTile {
        trees: Vec::new(),
        key,
        revision: 1,
        geometric_error: 0,
        columns: vec![crate::lod::Column::default(); crate::lod::TILE_COLUMNS],
    };
    let mesh = crate::render::lod::mesh(
        &tile,
        &[],
        &catalog,
        &crate::render::lod::FaceColors::new(&catalog),
    )
    .unwrap();
    (state, mesh)
}

#[test]
fn full_upload_queue_retains_completed_geometry_without_a_remesh_or_timer() {
    let (mut state, mesh) = fixture();
    let key = mesh.key;
    assert!(!offer(&mut state, 5, mesh, |mesh| Err(
        UploadError::QueueFull(Box::new(mesh))
    )));
    assert!(!retry(&mut state, |mesh| Err(UploadError::QueueFull(
        Box::new(mesh)
    ))));
    assert_eq!(state.builds[&key], 5);
    assert!(state.pending_mesh.is_empty());
    assert!(state.mesh_retry.is_empty());
    assert!(retry(&mut state, |mesh| {
        assert_eq!(mesh.key, key);
        Ok(())
    }));
    assert!(state.ready_upload.is_none());
    assert!(state.builds.is_empty());
    state.retire();
}

#[test]
fn invalidation_and_retirement_cannot_upload_a_retained_old_completion() {
    let (mut state, mesh) = fixture();
    let key = mesh.key;
    assert!(!offer(&mut state, 5, mesh, |mesh| Err(
        UploadError::QueueFull(Box::new(mesh))
    )));
    state.invalidate(7, key, 2);
    assert!(retry(&mut state, |_| panic!(
        "obsolete completion reached GPU admission"
    )));
    assert!(state.ready_upload.is_none());
    state.retire();

    let (mut state, mesh) = fixture();
    assert!(!offer(&mut state, 5, mesh, |mesh| Err(
        UploadError::QueueFull(Box::new(mesh))
    )));
    state.retire();
    assert!(state.ready_upload.is_none());
}

#[test]
fn residency_budget_rejection_does_not_block_other_completed_tiles() {
    let (mut state, mesh) = fixture();
    let key = mesh.key;
    assert!(offer(&mut state, 5, mesh, |mesh| Err(UploadError::Budget(
        Box::new(mesh)
    ))));
    assert!(state.ready_upload.is_none());
    assert!(state.pending_mesh.contains(&key));
    assert!(state.builds.is_empty());
    state.retire();
}
