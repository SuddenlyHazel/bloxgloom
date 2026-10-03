use super::*;
use crate::world::{LEAVES, STONE};

pub(super) fn avatar(position: Vec3) -> VisualAvatar {
    VisualAvatar {
        motion: None,
        animation: Default::default(),
        model: AvatarModel::Player,
        pose: [0.0; 4],
        model_pose: None,
        character_pose: [0.0; 4],
        character_look: [0.0; 2],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: None,
        airborne: false,
        id: 1,
        position,
        cosmetics: [0; 4],
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_color: [0; 3],
        glow_direction: [0; 3],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    }
}

fn flat(_: i32, y: i32, _: i32) -> Option<BlockId> {
    Some(if y <= 0 { STONE } else { AIR })
}

fn build(avatar: VisualAvatar, block: impl FnMut(i32, i32, i32) -> Option<BlockId>) -> Vec<Patch> {
    patches(
        &[avatar],
        avatar.position + Vec3::Y * 2.0,
        crate::content::catalog(),
        block,
    )
}

#[test]
fn floor_patches_remain_clipped_and_continuous_across_negative_chunk_seams() {
    let feet = Vec3::new(-16.02, 1.0, 15.99);
    let patches = build(avatar(feet), flat);
    assert_eq!(patches.len(), 4);
    for patch in &patches {
        let [x0, z0, x1, z1] = patch.bounds;
        assert!(x0 < x1 && z0 < z1);
        assert!(x1 - x0 <= 1.0 && z1 - z0 <= 1.0);
        assert!((patch.center[2] - 1.003).abs() < 0.0001);
        assert_eq!(patch.center[0], feet.x);
        assert_eq!(patch.center[1], feet.z);
    }
    assert!(patches.iter().any(|p| p.bounds[2] == -16.0));
    assert!(patches.iter().any(|p| p.bounds[0] == -16.0));
    assert!(patches.iter().any(|p| p.bounds[3] == 16.0));
    assert!(patches.iter().any(|p| p.bounds[1] == 16.0));
}

#[test]
fn ledges_do_not_project_onto_lower_floors_or_into_air() {
    let patches = build(avatar(Vec3::new(0.95, 1.0, 0.5)), |x, y, _| {
        Some(if y <= if x <= 0 { 0 } else { -1 } {
            STONE
        } else {
            AIR
        })
    });
    assert!(!patches.is_empty());
    assert!(patches.iter().all(|patch| patch.bounds[2] <= 1.0));
    assert!(patches.iter().all(|patch| patch.center[2] > 1.0));
    assert!(build(avatar(Vec3::new(1.5, 3.0, 0.5)), flat).is_empty());
}

#[test]
fn walls_block_receiver_tiles_and_disconnected_diagonal_corners() {
    let patches = build(avatar(Vec3::new(0.95, 1.0, 0.95)), |x, y, z| {
        Some(
            if y == 0 || (y == 1 && ((x == 1 && z == 0) || (x == 0 && z == 1))) {
                STONE
            } else {
                AIR
            },
        )
    });
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].bounds[2..], [1.0, 1.0]);
}

#[test]
fn stacked_floors_receive_only_the_nearest_supported_top() {
    let patches = build(avatar(Vec3::new(0.5, 3.1, 0.5)), |_, y, _| {
        Some(if y == 0 || y == 2 { STONE } else { AIR })
    });
    assert!(!patches.is_empty());
    assert!(
        patches
            .iter()
            .all(|patch| (patch.center[2] - 3.003).abs() < 0.0001)
    );
}

#[test]
fn unknown_cells_and_cutout_support_fail_closed() {
    let player = avatar(Vec3::new(0.5, 1.0, 0.5));
    assert!(build(player, |_, _, _| None).is_empty());
    assert!(build(player, |x, y, z| if y == 1 { None } else { flat(x, y, z) }).is_empty());
    assert!(
        build(player, |x, y, z| if y == 0 {
            Some(LEAVES)
        } else {
            flat(x, y, z)
        })
        .is_empty()
    );
    let edge = build(avatar(Vec3::new(15.95, 1.0, 0.5)), |x, y, z| {
        if x >= 16 { None } else { flat(x, y, z) }
    });
    assert!(!edge.is_empty());
    assert!(edge.iter().all(|patch| patch.bounds[2] <= 16.0));
}

#[test]
fn height_and_airborne_fade_without_following_visual_body_bob() {
    let player = avatar(Vec3::new(0.5, 1.0, 0.5));
    let grounded = build(player, flat)[0];
    let airborne = build(
        VisualAvatar {
            airborne: true,
            ..player
        },
        flat,
    )[0];
    let raised = build(
        VisualAvatar {
            position: player.position + Vec3::Y * 0.7,
            ..player
        },
        flat,
    )[0];
    assert!(airborne.light[0] < grounded.light[0]);
    assert!(raised.light[0] < airborne.light[0]);
    let bobbed = build(
        VisualAvatar {
            pose: [0.0, 0.0, 0.5, 0.0],
            ..player
        },
        flat,
    )[0];
    assert_eq!(bobbed.center, grounded.center);
    assert_eq!(bobbed.light, grounded.light);
    assert!(
        build(
            VisualAvatar {
                position: player.position + Vec3::Y * MAX_HEIGHT,
                ..player
            },
            flat
        )
        .is_empty()
    );
}

#[test]
fn sealed_darkness_produces_no_shadow_but_glow_can_support_contact() {
    let player = avatar(Vec3::new(0.5, 1.0, 0.5));
    assert!(
        build(
            VisualAvatar {
                light_levels: [0; 4],
                ..player
            },
            flat
        )
        .is_empty()
    );
    let torch = build(
        VisualAvatar {
            light_levels: [0, 8, 0, 0],
            ..player
        },
        flat,
    );
    assert!(!torch.is_empty());
    assert_eq!(torch[0].light[1], 0.0);
    assert!(torch[0].light[2] > 0.0);
}

#[test]
fn finite_player_near_field_and_geometry_work_are_bounded() {
    let player = avatar(Vec3::new(0.5, 1.0, 0.5));
    assert!(
        build(
            VisualAvatar {
                position: Vec3::NAN,
                ..player
            },
            flat
        )
        .is_empty()
    );
    assert!(
        build(
            VisualAvatar {
                model: AvatarModel::Moving(crate::content::MOSSBUN_ENTITY_TYPE),
                ..player
            },
            flat
        )
        .is_empty()
    );
    assert!(
        patches(
            &[player],
            Vec3::splat(100.0),
            crate::content::catalog(),
            flat
        )
        .is_empty()
    );
    let mut queries = 0;
    let patches = patches(
        &vec![player; MAX_AVATARS + 1],
        player.position,
        crate::content::catalog(),
        |x, y, z| {
            queries += 1;
            flat(x, y, z)
        },
    );
    assert_eq!(patches.len(), MAX_CHARACTERS * build(player, flat).len());
    assert!(patches.len() <= MAX_PATCHES);
    assert!(queries <= MAX_CHARACTERS * MAX_CELLS * 3);
}

#[test]
fn contact_shadow_shader_validates_without_a_gpu() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("shader.wgsl")).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn emissive_floor_textures_and_voxel_emitters_never_receive_contact() {
    use crate::content::{BlockStateId, BlockTypeId};
    let player = avatar(Vec3::new(0.5, 1.0, 0.5));
    assert!(
        build(player, |_, y, _| Some(if y == 0 {
            crate::world::GLOWSTONE
        } else {
            AIR
        }))
        .is_empty()
    );

    let mut catalog = Catalog::builtins();
    let mut texture = catalog
        .texture(catalog.state(STONE).unwrap().textures.top)
        .unwrap()
        .clone();
    texture.key = "test:contact_emission".into();
    texture.emission_strength = 2.0;
    let texture = catalog.register_texture(texture).unwrap();
    let mut block = catalog.block(STONE).unwrap().clone();
    block.id = BlockTypeId(70_010);
    block.key = "test:contact_emission".into();
    block.textures.top = texture;
    assert_eq!(
        block.emission, 0,
        "fixture must isolate texture-only emission"
    );
    catalog.register_block(block).unwrap();
    catalog
        .register_state(BlockStateId(70_010), BlockTypeId(70_010), vec![], None)
        .unwrap();
    assert!(
        patches(&[player], player.position, &catalog, |_, y, _| Some(
            if y == 0 { BlockStateId(70_010) } else { AIR }
        ))
        .is_empty()
    );
}

#[test]
fn registered_creatures_receive_body_sized_floor_validated_contacts() {
    let mut creature = avatar(Vec3::new(0.95, 1.0, 0.95));
    creature.model = AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE);
    let patches = build(creature, flat);
    assert!(!patches.is_empty());
    let body = crate::content::catalog()
        .mobile_entity(crate::content::MOSSBUN_ENTITY_TYPE)
        .unwrap();
    let expected = (body.body.half_width * 1.25 + 0.12).clamp(0.2, 1.35);
    assert_eq!(patches[0].center[3], expected);
    assert!(patches.len() <= MAX_CELLS);
    let ledge = build(
        creature,
        |x, y, z| {
            if x >= 1 { None } else { flat(x, y, z) }
        },
    );
    assert!(!ledge.is_empty());
    assert!(ledge.iter().all(|p| p.bounds[2] <= 1.0));
    assert!(build(creature, |_, _, _| None).is_empty());
    creature.position.y += MAX_HEIGHT;
    assert!(build(creature, flat).is_empty());
}

#[test]
fn largest_registered_body_remains_bounded_and_keeps_whole_footprint() {
    let mut catalog = Catalog::builtins();
    let mut definition = (**catalog
        .mobile_entity(crate::content::MOSSBUN_ENTITY_TYPE)
        .unwrap())
    .clone();
    definition.key = "test:wide-contact".into();
    definition.body.half_width = 1.0;
    catalog.register_mobile(definition).unwrap();
    let mut actor = avatar(Vec3::new(0.99, 1.0, -0.99));
    actor.model =
        AvatarModel::Registered(catalog.entity_type_id_by_key("test:wide-contact").unwrap());
    let mut queries = 0;
    let result = patches(&[actor], actor.position, &catalog, |x, y, z| {
        queries += 1;
        flat(x, y, z)
    });
    assert!(result.len() > 4);
    assert!(result.len() <= MAX_CELLS);
    assert!(queries <= MAX_CELLS * 3);
    assert_eq!(result[0].center[3], 1.35);
    let upload = crate::render::scene_contact::data(&result, 1.0);
    assert_eq!(upload[0] as usize, result.len());
}

#[test]
fn packaged_players_keep_native_ground_contacts() {
    let native = avatar(Vec3::new(0.5, 1.0, 0.5));
    let mut packaged = native;
    packaged.model = AvatarModel::PackagedPlayer(0);
    let native_patches = build(native, flat);
    let packaged_patches = build(packaged, flat);
    assert!(!packaged_patches.is_empty());
    assert_eq!(packaged_patches.len(), native_patches.len());
    for (packaged, native) in packaged_patches.iter().zip(&native_patches) {
        assert_eq!(packaged.bounds, native.bounds);
        assert_eq!(packaged.center, native.center);
        assert_eq!(packaged.light, native.light);
    }
    assert!(build(packaged, |_, _, _| None).is_empty());
}
