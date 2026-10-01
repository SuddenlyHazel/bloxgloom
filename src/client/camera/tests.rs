use super::*;

#[test]
fn perspective_changes_preserve_eye_reach_and_only_use_installed_terrain() {
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join(format!("third-person-{}", std::process::id())),
    );
    app.pitch = 0.0;
    app.perspective = Perspective::Behind;
    assert_eq!(
        app.view_camera().position,
        app.camera().position,
        "unknown terrain retracts camera"
    );
    assert!(!app.show_local_avatar(app.view_camera()));
    for x in -1..=0 {
        for z in -1..=0 {
            let key = ChunkKey { x, y: 2, z };
            let mut blocks = vec![AIR; crate::world::CHUNK_VOLUME];
            if x == 0 && z == 0 {
                blocks[Chunk::index([3, 9, 0]).unwrap()] = crate::world::STONE;
            }
            app.chunks
                .insert(key, Arc::new(Chunk::from_blocks(key, 1, blocks)));
        }
    }
    let hit = app.aimed_block().unwrap();
    assert_eq!(hit.block, [3, 41, 0]);
    let eye = app.camera().position;
    assert!((app.view_camera().position.distance(eye) - 4.0).abs() < 0.001);
    assert!(app.show_local_avatar(app.view_camera()));
    app.cycle_perspective();
    assert_eq!(
        app.aimed_block().unwrap(),
        hit,
        "front view does not move interaction origin or heading"
    );
    assert_eq!(app.camera().position, eye);
    app.cycle_perspective();
    assert_eq!(app.perspective, Perspective::FirstPerson);
    assert!(app.show_local_avatar(app.view_camera()));
}
