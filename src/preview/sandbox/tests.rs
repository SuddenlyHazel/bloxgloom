use super::*;

fn chunks(fill: BlockStateId) -> HashMap<ChunkKey, Arc<world::Chunk>> {
    let mut chunks = HashMap::new();
    for z in -2..=2 {
        for x in -2..=2 {
            for y in 0..=4 {
                let key = ChunkKey { x, y, z };
                chunks.insert(
                    key,
                    Arc::new(world::Chunk::from_blocks(
                        key,
                        0,
                        vec![fill; world::CHUNK_VOLUME],
                    )),
                );
            }
        }
    }
    chunks
}

fn block(chunks: &HashMap<ChunkKey, Arc<world::Chunk>>, xyz: [i32; 3]) -> BlockStateId {
    let (key, local) = world::world_to_chunk(xyz[0], xyz[1], xyz[2]);
    chunks[&key].block(local).unwrap()
}

#[test]
fn emissive_fixture_registration_is_valid_and_isolated_from_builtins() {
    let ordinary = Catalog::builtins();
    assert!(ordinary.state_by_key("sandbox:cyan").is_none());
    let mut preview = ordinary.clone();
    install_sandbox_materials(&mut preview).unwrap();
    preview.validate().unwrap();
    for name in ["cyan", "magenta"] {
        let id = preview.state_by_key(&format!("sandbox:{name}")).unwrap();
        assert_eq!(preview.emission(id), 15);
        let texture = preview
            .texture(preview.state(id).unwrap().textures.top)
            .unwrap();
        assert_eq!(texture.emission_strength, 3.5);
    }
    assert_eq!(preview.textures().len(), ordinary.textures().len() + 4);
    assert!(ordinary.state_by_key("sandbox:magenta").is_none());
    assert!(!preview.textures().len().is_multiple_of(6));
    assert!(preview.state_by_key("sandbox:gles_reserved").is_none());
}

#[test]
fn themes_preserve_common_swatches_tree_and_camera() {
    let mut catalog = Catalog::builtins();
    install_sandbox_materials(&mut catalog).unwrap();
    for theme in Theme::ALL {
        let mut chunks = chunks(world::AIR);
        let camera = prepare_with_catalog(
            Shot {
                theme,
                view: View::Hero,
                clip: "idle",
                seconds: 0.35,
            },
            &mut chunks,
            &catalog,
        );
        assert_eq!(camera.position, super::camera(View::Hero).position);
        assert_eq!(camera.yaw, super::camera(View::Hero).yaw);
        for (index, material) in [
            world::STONE,
            world::SAND,
            world::WOOD,
            world::GRAVEL,
            world::MOSS,
            world::LEAVES,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(block(&chunks, [2 + index as i32 * 2, 34, -14]), material);
        }
        assert_eq!(block(&chunks, [-5, 35, -9]), world::WOOD);
        assert_eq!(block(&chunks, [-5, 41, -9]), world::LEAVES);
        assert_eq!(block(&chunks, [-4, 34, -20]), world::AIR);
        assert_eq!(
            block(&chunks, [-4, 38, -20]),
            if theme == Theme::Workshop {
                world::WOOD
            } else {
                world::STONE
            }
        );
        assert_eq!(block(&chunks, [-4, 35, -22]), world::GLOWSTONE);
    }
}

#[test]
fn fixture_replaces_terrain_and_character_pose_does_not_move_between_themes() {
    let mut catalog = Catalog::builtins();
    install_sandbox_materials(&mut catalog).unwrap();
    let mut first = chunks(world::AIR);
    let mut second = chunks(world::SNOW);
    let shot = Shot {
        theme: Theme::Workshop,
        view: View::Characters,
        clip: "walk",
        seconds: 0.35,
    };
    prepare_with_catalog(shot, &mut first, &catalog);
    prepare_with_catalog(shot, &mut second, &catalog);
    for key in first.keys() {
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    assert_eq!(first[key].block([x, y, z]), second[key].block([x, y, z]));
                }
            }
        }
    }
    let workshop = avatars(&first);
    prepare_with_catalog(
        Shot {
            theme: Theme::Factory,
            ..shot
        },
        &mut first,
        &catalog,
    );
    let factory = avatars(&first);
    assert_eq!(workshop.len(), 6);
    for (a, b) in workshop.iter().zip(factory) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.pose, b.pose);
        assert_eq!(a.character_recipe, b.character_recipe);
        assert_eq!(a.position.z, -10.0);
        assert!(a.character_recipe.unwrap().valid());
    }
}

#[test]
fn invalid_capture_selectors_fail_before_creating_output() {
    let path =
        std::env::temp_dir().join(format!("bloxgloom-invalid-sandbox-{}", std::process::id()));
    for selectors in [
        ("invalid", "all", "hero", "idle"),
        ("all", "invalid", "hero", "idle"),
        ("all", "all", "invalid", "idle"),
        ("all", "all", "hero", "invalid"),
    ] {
        assert!(
            render_sandbox_previews(&path, selectors.0, selectors.1, selectors.2, selectors.3)
                .is_err()
        );
    }
    assert!(!path.exists());
}

#[test]
fn animation_time_is_bounded_and_reproducible() {
    assert_eq!(parse_seconds(None).unwrap(), 0.35);
    assert_eq!(parse_seconds(Some("0.70")).unwrap(), 0.70);
    for value in ["NaN", "inf", "-1", "60.01", "words"] {
        assert!(parse_seconds(Some(value)).is_err());
    }
}
