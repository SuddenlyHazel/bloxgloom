use super::*;

#[test]
fn default_only_body_hair_grouping_and_population_churn_remain_bounded() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let camera = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let mut renderer = CharacterRenderer::new(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        &camera,
        &crate::content::Catalog::builtins(),
    );
    let avatar = VisualAvatar {
        animation: Default::default(),
        model: AvatarModel::Player,
        pose: [0.0; 4],
        motion: None,
        character_pose: [0.0; 4],
        character_look: [0.0; 2],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: Some(Default::default()),
        airborne: false,
        id: 1,
        position: glam::Vec3::ZERO,
        cosmetics: [0; 4],
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    };
    let mut avatars = vec![avatar; MAX_AVATARS + 1];
    renderer.set(&queue, &avatars);
    assert_eq!(renderer.count, MAX_AVATARS as u32);
    assert_eq!(
        renderer.triangles(),
        2364 * MAX_AVATARS,
        "unselected catalog meshes must not be submitted"
    );
    assert_eq!(renderer.joints.size(), (MAX_AVATARS * JOINTS * 64) as u64);
    renderer.set(&queue, &avatars);
    assert_eq!(renderer.count, MAX_AVATARS as u32);
    renderer.set(&queue, &[]);
    assert_eq!(renderer.count, 0);
    for avatar in avatars.iter_mut().take(MAX_AVATARS) {
        avatar.model = AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE);
    }
    renderer.set(&queue, &avatars);
    assert_eq!(
        renderer.count, 0,
        "a distant player must not bypass nearest-first cap"
    );
    avatars[0].model = AvatarModel::Player;
    renderer.set(&queue, &avatars);
    assert_eq!(renderer.count, 1);
    let mut selected = [avatar; 14];
    for (index, actor) in selected.iter_mut().enumerate() {
        actor.character_recipe.as_mut().unwrap().hair = index as u8;
    }
    renderer.set(&queue, &selected);
    assert_eq!(&renderer.style_counts[..14], &[1; 14]);
    assert_eq!(&renderer.style_counts[14..], &[0; 14]);
    assert_eq!(renderer.triangles(), 13 * 2244 + 13648 - 3664);
    selected[0].character_recipe.as_mut().unwrap().hair = 255;
    renderer.set(&queue, &selected);
    assert_eq!(
        renderer.count, 13,
        "invalid local recipes cannot index GPU arrays"
    );
}

mod tint;
