use super::*;
fn model() -> Model {
    Model::from_glb(
        include_bytes!("../../../../fixtures/authored-model/model.glb"),
        serde_json::from_slice(include_bytes!(
            "../../../../fixtures/authored-model/controls.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn authored_front_follows_authoritative_game_heading() {
    for (yaw, expected) in [(0.0, Vec3::Z), (std::f32::consts::FRAC_PI_2, Vec3::X)] {
        // Match WGSL's x*c+z*s, z*c-x*s rotation around Y.
        let front = glam::Mat4::from_rotation_y(model_yaw(yaw)).transform_vector3(Vec3::NEG_Z);
        assert!(front.abs_diff_eq(expected, 1e-5));
    }
}
fn playback(model: &Model, name: &str) -> animation::Playback {
    animation::Playback {
        clip: Some(model.clips.iter().position(|c| c.name == name).unwrap()),
        serial: None,
        start_s: 0.0,
        speed: 1.0,
        looping: true,
        fade_s: 0.2,
    }
}
#[test]
fn interrupted_creature_crossfade_preserves_visible_pose_and_allocated_scratch() {
    let model = model();
    let mut animator = animation::Animator::new(&model);
    let pointer = animator.pose.as_ptr();
    let palette = animator.matrices.as_ptr();
    let mut bounce = playback(&model, "bounce");
    bounce.fade_s = 0.0;
    animator.step(&model, bounce, 0.0);
    for _ in 0..3 {
        animator.step(&model, bounce, 0.1);
    }
    let before: Vec<_> = animator.pose.iter().map(|p| p.matrix()).collect();
    let nod = playback(&model, "nod");
    animator.step(&model, nod, 0.0);
    assert!(
        animator
            .pose
            .iter()
            .zip(&before)
            .all(|(p, m)| p.matrix().abs_diff_eq(*m, 1e-5))
    );
    animator.step(&model, nod, 0.1);
    let interrupted: Vec<_> = animator.pose.iter().map(|p| p.matrix()).collect();
    let idle = playback(&model, "idle");
    animator.step(&model, idle, 0.0);
    assert!(
        animator
            .pose
            .iter()
            .zip(&interrupted)
            .all(|(p, m)| p.matrix().abs_diff_eq(*m, 1e-5))
    );
    for _ in 0..30 {
        animator.step(&model, idle, 0.1);
    }
    assert_eq!(pointer, animator.pose.as_ptr());
    assert_eq!(palette, animator.matrices.as_ptr());
    assert!(animator.matrices.iter().all(|m| m.is_finite()));
}
#[test]
fn playback_loop_override_and_snapshot_phase_do_not_change_shared_asset() {
    let model = model();
    let mut animator = animation::Animator::new(&model);
    let mut nod = playback(&model, "nod");
    nod.looping = false;
    nod.fade_s = 0.0;
    nod.start_s = model.clips[nod.clip.unwrap()].duration + 1.0;
    animator.step(&model, nod, 0.0);
    assert!(animator.finished(&model));
    let expected = model.sample(Some("nod"), nod.start_s).unwrap();
    assert!(
        animator
            .matrices
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff_eq(b, 1e-5))
    );
    nod.serial = Some((100, 1));
    nod.looping = true;
    nod.start_s = 0.35;
    animator.step(&model, nod, 0.0);
    assert!(!animator.finished(&model));
    let expected = model.sample(Some("nod"), 0.35).unwrap();
    assert!(
        animator
            .matrices
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff_eq(b, 1e-5))
    );
    let p = animator.pose.as_ptr();
    let w = animator.matrices.as_ptr();
    let mut pose = model.local_pose(None, 0.0).unwrap();
    let mut worlds = vec![glam::Mat4::IDENTITY; model.nodes.len()];
    let mut matrices = vec![glam::Mat4::IDENTITY; model.bindings.len()];
    model
        .local_pose_index_into(
            nod.clip,
            model.clips[nod.clip.unwrap()].duration + 0.35,
            true,
            &mut pose,
        )
        .unwrap();
    model
        .matrices_into(&pose, &mut worlds, &mut matrices)
        .unwrap();
    assert!(
        animator
            .matrices
            .iter()
            .zip(matrices)
            .all(|(a, b)| a.abs_diff_eq(b, 1e-5))
    );
    assert_eq!(p, animator.pose.as_ptr());
    assert_eq!(w, animator.matrices.as_ptr());
}
pub(in crate::render::avatars) fn catalog()
-> (crate::content::Catalog, crate::content::EntityTypeId) {
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_model_asset(&bloxgloom_host_api::model::ModelAsset {
            key: "render:creature".into(),
            glb: include_bytes!("../../../../fixtures/authored-model/model.glb").to_vec(),
            controls: include_bytes!("../../../../fixtures/authored-model/controls.json").to_vec(),
            scale: 1.0,
        })
        .unwrap();
    let mut creature = (**catalog
        .mobile_entity(crate::content::MOSSBUN_ENTITY_TYPE)
        .unwrap())
    .clone();
    creature.key = "render:actor".into();
    creature.interaction.clear();
    creature.model.clear();
    creature.authored_model = Some(bloxgloom_host_api::entity::AuthoredModel {
        key: "render:creature".into(),
        scale: 1.0,
        idle: Some("idle".into()),
        walk: Some("bounce".into()),
        run: None,
    });
    catalog.register_mobile(creature).unwrap();
    let id = catalog.entity_type_id_by_key("render:actor").unwrap();
    (catalog, id)
}
pub(in crate::render::avatars) fn avatar(
    id: u64,
    kind: crate::content::EntityTypeId,
    x: f32,
    color: [u8; 3],
) -> VisualAvatar {
    let mut visual = VisualState {
        transition_s: 0.0,
        ..VisualState::default()
    };
    visual.tints[0] = Some(bloxgloom_host_api::entity::Tint {
        rgb: color,
        mode: TintMode::Replace,
    });
    VisualAvatar {
        motion: None,
        model_pose: Some(visual),
        animation: Default::default(),
        model: AvatarModel::Registered(kind),
        pose: [0.0; 4],
        character_pose: [0.0; 4],
        character_look: [0.0; 2],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: None,
        airborne: false,
        id,
        position: Vec3::new(x, 0.0, 0.0),
        cosmetics: [0; 4],
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    }
}
#[test]
fn gpu_packaged_creatures_share_asset_but_keep_independent_looks_and_authored_clips() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (catalog, id) = catalog();
    let avatars = [
        avatar(11, id, -0.65, [255, 0, 0]),
        avatar(12, id, 0.65, [0, 0, 255]),
    ];
    let render = |a: &[VisualAvatar]| {
        super::super::tests::render_avatars(&device, &queue, &catalog, a, None)
    };
    let original = render(&avatars);
    assert!(
        original
            .chunks_exact(4)
            .filter(|p| p[1] > 10 && p[1] > p[0].saturating_mul(2) && p[1] > p[2].saturating_mul(2))
            .count()
            >= 2,
        "yaw-zero GLB creatures must show their green iris geometry toward game +Z"
    );
    let mut multiplied = avatars;
    multiplied[0].model_pose.as_mut().unwrap().tints[0]
        .as_mut()
        .unwrap()
        .mode = TintMode::Multiply;
    let textured = render(&multiplied);
    assert!(
        original
            .iter()
            .zip(&textured)
            .filter(|(a, b)| a != b)
            .count()
            > 20,
        "multiplication must preserve the embedded painted texture instead of replacing it"
    );
    let count = |image: &[u8], channel: usize| {
        image
            .chunks_exact(4)
            .filter(|p| p[channel] > 25 && p[(channel + 1) % 3] < 5 && p[(channel + 2) % 3] < 5)
            .count()
    };
    assert!(count(&original, 0) > 40);
    assert!(count(&original, 2) > 40);
    let mut changed = avatars;
    changed[1].model_pose.as_mut().unwrap().layers[1] = 1;
    let variant = render(&changed);
    assert!(
        original
            .iter()
            .zip(&variant)
            .filter(|(a, b)| a != b)
            .count()
            > 20,
        "hat layer did not alter GPU image"
    );
    changed[1].model_pose.as_mut().unwrap().playback =
        Some(bloxgloom_host_api::entity::ClipPlayback {
            clip: 2,
            speed: 1.0,
            looping: false,
            crossfade_s: 0.0,
            started_tick: 0,
            sequence: 1,
        });
    changed[1].model_pose.as_mut().unwrap().sample_tick = 20;
    changed[1].model_pose.as_mut().unwrap().sequence = 1;
    let animated = render(&changed);
    assert!(
        animated
            .iter()
            .zip(&variant)
            .filter(|(a, b)| a != b)
            .count()
            > 20,
        "baked clip did not move GPU geometry"
    );
    if let Some(path) = std::env::var_os("BLOXGLOOM_GLB_CREATURE_PREVIEW") {
        let file = std::fs::File::create(path).unwrap();
        let mut png = png::Encoder::new(
            file,
            super::super::tests::WIDTH,
            super::super::tests::HEIGHT,
        );
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .unwrap()
            .write_image_data(&animated)
            .unwrap();
    }
}

#[test]
fn authored_palette_admission_is_bounded_and_completed_clips_do_not_restart() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (catalog, id) = catalog();
    let camera = crate::render::sun_shadow::camera_layout(&device);
    let mut renderer = Renderer::new(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        &camera,
        &catalog,
    );
    let avatars: Vec<_> = (0..MAX_AVATARS + 1)
        .map(|i| avatar(i as u64, id, 0.0, [255; 3]))
        .collect();
    renderer.set_at(&queue, &avatars, 0.1);
    assert_eq!(renderer.instances.len(), MAX_AVATARS);
    assert_eq!(renderer.actors.len(), MAX_AVATARS);
    assert!(!renderer.actors.contains_key(&(MAX_AVATARS as u64)));
    let mut actor = avatars[0];
    let visual = actor.model_pose.as_mut().unwrap();
    visual.playback = Some(bloxgloom_host_api::entity::ClipPlayback {
        clip: 2,
        speed: 1.0,
        looping: false,
        crossfade_s: 0.0,
        started_tick: 0,
        sequence: 1,
    });
    visual.sample_tick = 15;
    visual.sequence = 1;
    renderer.set_at(&queue, &[actor], 0.0);
    assert_eq!(renderer.actors.len(), 1);
    let model = &renderer.assets[0].model;
    let expected = model.sample(Some("nod"), 0.3).unwrap();
    assert!(
        renderer.actors[&0]
            .animator
            .matrices
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff_eq(b, 1e-5)),
        "late snapshot must seed the baked clip phase"
    );
    for _ in 0..100 {
        renderer.set_at(&queue, &[actor], 0.1);
    }
    assert_eq!(
        renderer.actors[&0].completed,
        Some((0, 1)),
        "nonlooping clip must return to locomotion without replaying the same server command"
    );
    let visual = actor.model_pose.as_mut().unwrap();
    visual.sequence = 2;
    visual.sample_tick = 0;
    visual.playback.as_mut().unwrap().sequence = 2;
    renderer.set_at(&queue, &[actor], 0.0);
    assert_eq!(
        renderer.actors[&0].completed, None,
        "explicit restart identity must retrigger the same clip"
    );
    actor
        .model_pose
        .as_mut()
        .unwrap()
        .playback
        .as_mut()
        .unwrap()
        .clip = u16::MAX;
    renderer.set_at(&queue, &[actor], 0.1);
    assert!(
        renderer.actors[&0]
            .animator
            .matrices
            .iter()
            .all(|m| m.is_finite()),
        "invalid public clip indices must fall back without indexing outside the shared asset"
    );
}
