use super::*;
use crate::raycast::Face;

pub(crate) struct ReplicationProbe {
    replicas: Replicas,
    registry: EntityClientRegistry,
    catalog: Arc<crate::content::Catalog>,
    pub(crate) chunks: HashMap<ChunkKey, Arc<Chunk>>,
}
impl std::ops::Deref for ReplicationProbe {
    type Target = HashMap<ChunkKey, Arc<Chunk>>;
    fn deref(&self) -> &Self::Target {
        &self.chunks
    }
}

impl ReplicationProbe {
    pub(crate) fn new() -> Self {
        Self {
            replicas: Replicas::default(),
            registry: EntityClientRegistry::builtins(crate::content::catalog()),
            catalog: Arc::new(crate::content::catalog().clone()),
            chunks: HashMap::new(),
        }
    }
    pub(crate) fn accept(&mut self, message: ServerMessage) {
        if matches!(
            message,
            ServerMessage::WorldSnapshotStart(_)
                | ServerMessage::EntitySnapshotPage(_)
                | ServerMessage::WorldCommitPart(_)
        ) {
            let result =
                self.replicas
                    .accept(message, &self.catalog, &mut self.chunks, &self.registry);
            if let Assembly::Resync(keys) = result {
                panic!("valid server stream required resync: {keys:?}");
            }
        }
    }
}

#[test]
fn latest_edit_mesh_survives_a_superseded_kiln_relight_backlog() {
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join("unused-kiln-backlog-config"),
    );
    let key = ChunkKey { x: 0, y: 5, z: 0 };
    let mut blocks = vec![crate::world::AIR; crate::world::CHUNK_VOLUME];
    blocks[Chunk::index([8, 1, 8]).unwrap()] =
        crate::content::BlockStateId(crate::content::KILN_DEFAULT_STATE.0 + 1);
    app.chunks
        .insert(key, Arc::new(Chunk::from_blocks(key, 1, blocks)));
    let start = Instant::now();
    let mut queued = Vec::new();
    for _ in 0..16 {
        app.queue_relight(key, false);
        let revision = app.lighting_revisions[&key];
        queued.push(MesherJob {
            chunk: app.chunks[&key].clone(),
            known: app.lighting_snapshot(key),
            catalog: app.catalog.clone(),
            seed: 7,
            revision,
            bounced_gi: false,
        });
    }
    let revision = app.lighting_revisions[&key];
    // Model jobs already queued when a newer edit invalidates their light
    // fields. Publish invalidation first so cancellation is race-independent.
    for job in queued {
        app.mesher.urgent_jobs.send(job).unwrap();
    }
    let mut obsolete = 0;
    loop {
        let result = app
            .mesher
            .results
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        if result.mesh.lighting_revision == revision {
            break;
        }
        obsolete += 1;
    }
    eprintln!(
        "latest kiln-neighborhood mesh: {:?}, obsolete completed results: {obsolete}",
        start.elapsed()
    );
    assert_eq!(
        obsolete, 0,
        "superseded jobs must be skipped before lighting and meshing"
    );
    app.config_writer.finish();
}

#[test]
fn moving_object_lighting_keeps_completed_field_during_relight_then_accepts_darkness() {
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join("unused-actor-relight-config"),
    );
    let position = Vec3::new(0.5, 80.5, 0.5);
    let key = crate::world::world_to_chunk(0, 80, 0).0;
    let lit = LightSample {
        sky: 15,
        glow: 0,
        bounce: [3, 4, 5],
    };
    app.chunks.insert(
        key,
        Arc::new(Chunk::from_blocks(
            key,
            1,
            vec![crate::world::AIR; crate::world::CHUNK_VOLUME],
        )),
    );
    app.light_samples.insert(
        key,
        (1, vec![lit; crate::world::CHUNK_VOLUME].into_boxed_slice()),
    );
    app.lighting_revisions.insert(key, 1);
    app.next_lighting_revision = 2;
    assert_eq!(app.light_at(position), lit);
    // A terrain edit invalidates the field while a worker prepares its replacement.
    app.queue_edited_chunk_relight(key);
    assert_eq!(
        app.light_at(position),
        lit,
        "pending light is not a black frame"
    );
    // Streaming a neighboring chunk as the camera moves also invalidates this
    // field. Repeated invalidation must not black out a stationary drop.
    app.queue_relight(
        crate::world::ChunkKey {
            x: key.x + 1,
            ..key
        },
        true,
    );
    assert_eq!(app.light_at(position), lit);
    let revision = app.lighting_revisions[&key];
    app.light_samples.insert(
        key,
        (
            revision,
            vec![LightSample::default(); crate::world::CHUNK_VOLUME].into_boxed_slice(),
        ),
    );
    assert_eq!(
        app.light_at(position),
        LightSample::default(),
        "completed dark caves must stay dark"
    );
    app.light_samples.remove(&key);
    assert_eq!(app.light_at(position), LightSample::default());
    app.config_writer.finish();
}

#[test]
fn moving_objects_sample_current_local_light_across_negative_chunk_seams() {
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join("unused-light-sampling-config"),
    );
    let position = Vec3::new(-0.1, 8.2, 0.5);
    let (key, local) = crate::world::world_to_chunk(-1, 8, 0);
    let lit = LightSample {
        sky: 0,
        glow: 13,
        bounce: [7, 9, 11],
    };
    let mut samples = vec![LightSample::default(); crate::world::CHUNK_VOLUME].into_boxed_slice();
    samples[Chunk::index(local).unwrap()] = lit;
    app.light_samples.insert(key, (1, samples));
    app.lighting_revisions.insert(key, 1);
    assert_eq!(app.light_at(position), lit);
    assert_eq!(
        app.light_at(Vec3::new(0.1, 8.2, 0.5)),
        LightSample::default()
    );
    app.lighting_revisions.insert(key, 2);
    assert_eq!(
        app.light_at(position),
        lit,
        "keep displayed illumination until the replacement field arrives"
    );
    app.light_samples.insert(
        key,
        (
            2,
            vec![LightSample::default(); crate::world::CHUNK_VOLUME].into_boxed_slice(),
        ),
    );
    assert_eq!(app.light_at(position), LightSample::default());
    app.config_writer.finish();
}

#[test]
fn graphics_controls_apply_save_and_preserve_values_while_disabled() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-graphics-{}-{unique}",
        std::process::id()
    ));
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        path.clone(),
    );
    app.change_setting(SettingId::Exposure, true);
    app.change_setting(SettingId::BloomStrength, true);
    let exposure = app.config.exposure;
    let strength = app.config.bloom_strength;
    assert!(exposure > 1.0 && strength > 0.12);
    app.change_setting(SettingId::Bloom, false);
    app.change_setting(SettingId::PostProcessing, false);
    assert!(!app.config.bloom_enabled && !app.config.post_processing);
    assert_eq!(app.config.exposure, exposure);
    assert_eq!(app.config.bloom_strength, strength);
    app.screen = UiScreen::Graphics;
    assert!(
        app.focus_order()
            .contains(&UiControl::Increase(SettingId::Exposure))
    );
    app.config_writer.finish();
    assert_eq!(Config::load(&path), app.config);
    std::fs::remove_file(path).unwrap();
    assert!(
        app.pending_mesh.is_empty(),
        "post controls must not remesh terrain"
    );
}

#[test]
fn lamp_edit_rebuilds_both_sides_of_a_chunk_seam_urgently() {
    use crate::world::{AIR, GLOWSTONE, STONE};

    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join(format!("bloxgloom-seam-{}.toml", std::process::id())),
    );
    let left = ChunkKey { x: 0, y: 0, z: 0 };
    let right = ChunkKey { x: 1, ..left };
    for y in -1..=1 {
        for z in -1..=1 {
            for x in -1..=2 {
                let key = ChunkKey { x, y, z };
                app.chunks.insert(
                    key,
                    Arc::new(Chunk {
                        key,
                        version: 0,
                        blocks: vec![STONE; crate::world::CHUNK_VOLUME].into(),
                    }),
                );
            }
        }
    }
    for x in 14..=18 {
        let (key, local) = crate::world::world_to_chunk(x, 8, 8);
        Arc::make_mut(app.chunks.get_mut(&key).unwrap())
            .blocks
            .set(Chunk::index(local).unwrap(), AIR);
    }
    for (version, block, expected_glow) in [(1, GLOWSTONE, 14), (2, AIR, 0)] {
        app.urgent_mesh.clear();
        app.queue_relight(right, false);
        let old_revision = app.lighting_revisions[&right];
        app.pending_upload.push_back(ChunkMesh {
            key: right,
            version: 0,
            lighting_revision: old_revision,
            vertices: Vec::new(),
            indices: Vec::new(),
            cutout_vertices: Vec::new(),
            cutout_indices: Vec::new(),
        });
        app.accept(ServerMessage::Delta {
            key: left,
            version,
            x: 15,
            y: 8,
            z: 8,
            block,
        });
        assert!(app.urgent_mesh.contains(&left));
        assert!(
            app.urgent_mesh.contains(&right),
            "the neighbour must not wait behind terrain streaming"
        );
        assert!(app.lighting_revisions[&right] > old_revision);
        assert_eq!(app.pending_mesh[&right], app.lighting_revisions[&right]);
        assert!(
            app.pending_upload.is_empty(),
            "old neighbour lighting must not reach upload"
        );
        let lighting =
            crate::lighting::LightField::build(right, &app.lighting_snapshot(right), 0xB10C_6100);
        assert_eq!(lighting.face([0, 8, 8], 1, 0).glow, expected_glow);
    }
    app.config_writer.finish();
}

#[test]
fn skylight_capture_and_invalidation_include_distant_roofs() {
    let target = ChunkKey {
        x: 16,
        y: -1,
        z: 20,
    };
    assert!(lighting_depends_on(target, ChunkKey { y: 4, ..target }));
    assert!(lighting_depends_on(target, ChunkKey { y: -2, ..target }));
    assert!(!lighting_depends_on(target, ChunkKey { y: -3, ..target }));
    assert!(!lighting_depends_on(
        target,
        ChunkKey {
            x: 18,
            y: 4,
            ..target
        }
    ));
}

#[test]
fn missing_meshes_are_nearest_first_without_delaying_urgent_edits() {
    let center = ChunkKey { x: 0, y: 0, z: 0 };
    let near = ChunkKey { x: 1, ..center };
    let far = ChunkKey { x: 5, ..center };
    assert!(mesh_priority(near, center, false, false) < mesh_priority(far, center, false, false));
    assert!(mesh_priority(far, center, false, false) < mesh_priority(near, center, false, true));
    assert!(mesh_priority(far, center, true, true) < mesh_priority(near, center, false, false));
}

#[test]
fn client_retains_the_expanded_vertical_band() {
    let center = ChunkKey { x: -2, y: -3, z: 1 };
    for offset in [-4, 4] {
        assert!(chunk_in_view(
            ChunkKey {
                y: center.y + offset,
                ..center
            },
            center,
            1
        ));
    }
    for offset in [-5, 5] {
        assert!(!chunk_in_view(
            ChunkKey {
                y: center.y + offset,
                ..center
            },
            center,
            1
        ));
    }
}

#[test]
fn action_ids_keep_session_and_order_without_reuse() {
    let first = action_id(0x1234, 1);
    let second = action_id(0x1234, 2);
    let other_session = action_id(0x1235, 1);
    assert_eq!(first >> 64, 0x1234);
    assert_eq!(first as u64, 1);
    assert!(first < second);
    assert_ne!(first, other_session);
}

#[test]
fn action_results_ack_only_a_contiguous_server_issued_session() {
    let mut tracker = ActionTracker::default();
    assert!(tracker.allocate().is_none());
    assert!(tracker.install_fresh_session(17, 1, 0).is_ok());
    assert!(tracker.install_fresh_session(18, 1, 0).is_err());
    let first = tracker.allocate().unwrap();
    let second = tracker.allocate().unwrap();
    assert_eq!(first, action_id(17, 1));
    assert_eq!(second, action_id(17, 2));
    assert_eq!(tracker.terminal_result(second).unwrap(), None);
    assert_eq!(tracker.terminal_result(first).unwrap(), Some(2));
    assert_eq!(tracker.terminal_result(first).unwrap(), None);
    assert!(tracker.terminal_result(action_id(16, 1)).is_err());
    assert!(tracker.terminal_result(action_id(17, 3)).is_err());
}

#[test]
fn escape_and_inventory_transitions_preserve_menu_flow() {
    assert_eq!(escape_screen(UiScreen::Playing), UiScreen::Pause);
    assert_eq!(escape_screen(UiScreen::Pause), UiScreen::Playing);
    assert_eq!(escape_screen(UiScreen::Settings), UiScreen::Pause);
    assert_eq!(escape_screen(UiScreen::Graphics), UiScreen::Settings);
    assert_eq!(escape_screen(UiScreen::Inventory), UiScreen::Playing);
    assert_eq!(inventory_screen(UiScreen::Playing), UiScreen::Inventory);
    assert_eq!(inventory_screen(UiScreen::Inventory), UiScreen::Playing);
    assert_eq!(inventory_screen(UiScreen::Pause), UiScreen::Pause);
}

#[test]
fn block_edit_uses_selected_hotbar_block_and_hit_face() {
    let hit = Hit {
        block: [2, 3, 4],
        adjacent: [1, 3, 4],
        block_id: crate::world::BlockId::new(3),
        distance: 2.5,
        face: Face::NegX,
    };
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::ItemId::new(1)), 2, 71),
        Some(ClientMessage::Edit {
            action_id: 71,
            x: 1,
            y: 3,
            z: 4,
            block: crate::world::BlockId::new(1),
            slot: 2,
        })
    );
    assert_eq!(
        edit_for_hit(hit, false, Some(crate::items::ItemId::new(1)), 2, 72),
        Some(ClientMessage::Edit {
            action_id: 72,
            x: 2,
            y: 3,
            z: 4,
            block: crate::world::AIR,
            slot: 2,
        })
    );
}

#[test]
fn cardinal_placement_hint_follows_camera_yaw() {
    let catalog = crate::content::catalog();
    let hit = Hit {
        block: [2, 3, 4],
        adjacent: [2, 4, 4],
        block_id: crate::world::STONE,
        distance: 2.5,
        face: Face::PosY,
    };
    for (yaw, facing) in [
        (0.0, "west"),
        (std::f32::consts::FRAC_PI_2, "north"),
        (std::f32::consts::PI, "east"),
        (-std::f32::consts::FRAC_PI_2, "south"),
    ] {
        let message = edit_for_hit_with_catalog(
            hit,
            true,
            Some(crate::content::KILN_ITEM),
            0,
            91,
            yaw,
            catalog,
        )
        .unwrap();
        let ClientMessage::Edit { block, .. } = message else {
            panic!("placement did not produce an edit");
        };
        assert_eq!(
            block,
            catalog
                .state_with_property(crate::content::KILN_DEFAULT_STATE, "facing", facing)
                .unwrap()
        );
    }
}

#[test]
fn placing_on_a_replaceable_flower_targets_its_cell() {
    let hit = Hit {
        block: [2, 3, 4],
        adjacent: [2, 4, 4],
        block_id: crate::world::RED_FLOWER,
        distance: 2.5,
        face: Face::PosY,
    };
    assert_eq!(
        edit_for_hit(
            hit,
            true,
            Some(crate::items::ItemId::new(crate::world::WOOD.get())),
            0,
            73
        ),
        Some(ClientMessage::Edit {
            action_id: 73,
            x: 2,
            y: 3,
            z: 4,
            block: crate::world::WOOD,
            slot: 0,
        })
    );
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::SEEDS), 0, 74),
        None
    );
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::SAPLING), 0, 75),
        None
    );
    assert_eq!(
        edit_for_hit(hit, true, Some(crate::items::STICK), 0, 76),
        None
    );
    assert!(edit_for_hit(hit, false, Some(crate::items::SEEDS), 0, 77).is_some());
}

#[test]
fn mapped_server_item_and_replaceable_state_drive_placement_preview() {
    use crate::content::{BlockStateId, ContentManifest};
    use crate::items::ItemId;
    use glam::Vec3;

    let local = crate::content::Catalog::builtins();
    let mut manifest = ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        match (entry.kind, entry.key.as_str()) {
            (b'B', "bloxgloom:red_flower") => entry.id = 65_536,
            (b'S', "bloxgloom:red_flower") => entry.id = 65_537,
            (b'I', "bloxgloom:red_flower") => entry.id = 65_538,
            _ => {}
        }
    }
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let catalog = manifest.resolve_catalog(&local).unwrap();
    let flower = BlockStateId::new(65_537);
    let item = ItemId::new(65_538);
    assert!(catalog.state(crate::world::RED_FLOWER).is_none());

    let hit = raycast::raycast_with_catalog(
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::X,
        7.0,
        |x, y, z| {
            Some(if x == 2 && y == 0 && z == 0 {
                flower
            } else {
                crate::world::AIR
            })
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(hit.block_id, flower);
    assert_eq!(
        edit_for_hit_with_catalog(hit, true, Some(item), 0, 91, 0.0, &catalog),
        Some(ClientMessage::Edit {
            action_id: 91,
            x: 2,
            y: 0,
            z: 0,
            block: flower,
            slot: 0,
        })
    );
}

#[test]
fn client_cache_uses_server_view_radius() {
    let center = ChunkKey { x: -10, y: 4, z: 5 };
    assert!(chunk_in_view(
        ChunkKey {
            x: -16,
            y: 5,
            z: 11
        },
        center,
        6
    ));
    assert!(!chunk_in_view(
        ChunkKey {
            x: -16,
            y: 5,
            z: 11
        },
        center,
        3
    ));
    assert!(!chunk_in_view(ChunkKey { x: -10, y: 9, z: 5 }, center, 6));
}
