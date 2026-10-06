use super::*;
use crate::content::Catalog;

/// Runs without a window; exercises the actual avatar mesh, pipeline, palette
/// shader and instance bytes. Set BLOXGLOOM_APPEARANCE_PREVIEW to retain a PNG.
#[test]
fn gpu_registered_player_palettes_preserve_default_and_color_all_three_parts() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let builtin = Catalog::builtins();
    let mut authored = builtin.clone();
    authored
        .register_player_appearance(bloxgloom_host_api::appearance::Appearance {
            key: "demo:wardrobe".into(),
            revision: 1,
            model: bloxgloom_host_api::appearance::MODEL.into(),
            palettes: [
                vec![[0.0, 1.0, 0.0]],
                vec![[1.0, 0.0, 0.0]],
                vec![[0.0, 0.0, 1.0]],
            ],
        })
        .unwrap();
    let recipe = crate::appearance::CharacterRecipe {
        body: 1,
        ..Default::default()
    };
    let original = render_recipe(
        &device,
        &queue,
        &builtin,
        [0; 4],
        [1.0; 3],
        None,
        Some(recipe),
    );
    let changed = render_recipe(
        &device,
        &queue,
        &authored,
        [6, 8, 6, 0],
        [1.0; 3],
        None,
        Some(recipe),
    );
    let tinted = render_recipe(
        &device,
        &queue,
        &authored,
        [6, 8, 6, 0],
        [0.0, 1.0, 0.0],
        None,
        Some(recipe),
    );
    for row in 0..HEIGHT as usize {
        let start = row * WIDTH as usize * 4;
        assert_eq!(
            &original[start..start + WIDTH as usize * 2],
            &changed[start..start + WIDTH as usize * 2],
            "builtin default changed"
        );
        assert_eq!(
            &changed[start..start + WIDTH as usize * 2],
            &tinted[start..start + WIDTH as usize * 2],
            "tint changed another avatar"
        );
    }
    let shirt = ((1.9 - 1.07) / 2.0 * HEIGHT as f32) as usize * WIDTH as usize
        + ((0.65 + 1.4) / 2.8 * WIDTH as f32) as usize;
    assert!(
        changed[shirt * 4] > tinted[shirt * 4] + 25,
        "shirt pixel {:?} tinted {:?}",
        &changed[shirt * 4..shirt * 4 + 4],
        &tinted[shirt * 4..shirt * 4 + 4]
    );
    // Samples are on the front face, avoiding eyes, seams, hair and silhouettes.
    for (x, y, channel) in [(0.65, 0.98, 1), (0.65, 1.07, 0), (0.53, 0.65, 2)] {
        let px = ((x + 1.4) / 2.8 * WIDTH as f32) as usize;
        let py = ((1.9 - y) / 2.0 * HEIGHT as f32) as usize;
        let pixel = &changed[(py * WIDTH as usize + px) * 4..][..4];
        assert!(pixel[channel] > 25, "part not drawn: {pixel:?}");
        for other in 0..3 {
            if other != channel {
                assert!(pixel[other] < 3, "wrong palette part: {pixel:?}");
            }
        }
    }
    if let Some(path) = std::env::var_os("BLOXGLOOM_APPEARANCE_PREVIEW") {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&changed)
            .unwrap();
    }
    if let Some(path) = std::env::var_os("BLOXGLOOM_TINT_PREVIEW") {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&tinted)
            .unwrap();
    }
}

pub(super) const WIDTH: u32 = 128;
pub(super) const HEIGHT: u32 = 96;
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    catalog: &Catalog,
    selection: [u8; 4],
    tint: [f32; 3],
    clip: Option<(&'static str, f32)>,
) -> Vec<u8> {
    render_recipe(device, queue, catalog, selection, tint, clip, None)
}
fn render_recipe(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    catalog: &Catalog,
    selection: [u8; 4],
    tint: [f32; 3],
    clip: Option<(&'static str, f32)>,
    recipe: Option<crate::appearance::CharacterRecipe>,
) -> Vec<u8> {
    let avatars = [(-0.65, [0; 4]), (0.65, selection)].map(|(x, cosmetics)| VisualAvatar {
        motion: None,
        model_pose: None,
        character_pose: [0.0; 4],
        character_look: [0.0; 2],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: if x > 0.0 {
            recipe.or_else(|| clip.map(|_| Default::default()))
        } else {
            clip.map(|_| Default::default())
        },
        animation: Default::default(),
        model: AvatarModel::Player,
        pose: [0.0; 4],
        airborne: false,
        id: if x < 0.0 { 1 } else { 2 },
        position: Vec3::new(x, 0.0, 0.0),
        cosmetics,
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_color: [0; 3],
        glow_direction: [0; 3],
        glow_bounce: [0; 4],
        tint: if x > 0.0 { tint } else { [1.0; 3] },
    });
    render_avatars(device, queue, catalog, &avatars, clip)
}

pub(super) fn render_avatars(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    catalog: &Catalog,
    avatars: &[VisualAvatar],
    clip: Option<(&'static str, f32)>,
) -> Vec<u8> {
    render_avatars_with_view(device, queue, catalog, avatars, clip, None)
}
pub(super) fn render_avatars_with_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    catalog: &Catalog,
    avatars: &[VisualAvatar],
    clip: Option<(&'static str, f32)>,
    view: Option<super::FirstPersonView>,
) -> Vec<u8> {
    let camera = glam::camera::rh::proj::directx::orthographic(-1.4, 1.4, -0.1, 1.9, 0.1, 10.0)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Vec3::Y);
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(
            &crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS)
                .camera_data(camera, Vec3::new(0.0, 0.0, 4.0)),
        ),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut renderer = AvatarRenderer::new(
        device,
        queue,
        wgpu::TextureFormat::Rgba8Unorm,
        &camera,
        catalog,
    );
    if let Some((clip, time)) = clip {
        renderer.preview_character_clip(clip, time);
    }
    renderer.set_first_person(view);
    renderer.set(queue, avatars);
    let size = wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    let texture = |format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(WIDTH * HEIGHT * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color.create_view(&Default::default()),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.create_view(&Default::default()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        assert!(renderer.draw(&mut pass) > 0);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
        },
        size,
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    bytes
}

#[test]
fn authored_gpu_character_draws_textured_animated_geometry_and_instance_tint() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let catalog = Catalog::builtins();
    let idle = render(
        &device,
        &queue,
        &catalog,
        [0; 4],
        [1.0; 3],
        Some(("idle", 0.0)),
    );
    let walk = render(
        &device,
        &queue,
        &catalog,
        [0; 4],
        [1.0; 3],
        Some(("walk", 0.2)),
    );
    let crouch = render(
        &device,
        &queue,
        &catalog,
        [0; 4],
        [1.0; 3],
        Some(("crouch", 1.0)),
    );
    let tinted = render(
        &device,
        &queue,
        &catalog,
        [0; 4],
        [0.0; 3],
        Some(("idle", 0.0)),
    );
    assert!(idle.chunks_exact(4).filter(|p| p[0] > 20).count() > 100);
    assert_ne!(idle, walk, "authored walk should deform the mesh");
    assert_ne!(idle, crouch, "authored crouch should deform the mesh");
    for row in 0..HEIGHT as usize {
        let start = row * WIDTH as usize * 4;
        assert_eq!(
            &idle[start..start + WIDTH as usize * 2],
            &tinted[start..start + WIDTH as usize * 2],
            "another instance changed"
        );
    }
    assert!(
        tinted.iter().map(|&n| u64::from(n)).sum::<u64>()
            < idle.iter().map(|&n| u64::from(n)).sum::<u64>()
    );
}

#[test]
fn different_recipes_color_only_selected_irises_and_swap_hair_per_instance() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let catalog = Catalog::builtins();
    let render = |recipe| {
        render_recipe(
            &device,
            &queue,
            &catalog,
            [0; 4],
            [1.0; 3],
            Some(("idle", 0.0)),
            Some(recipe),
        )
    };
    let default = crate::appearance::CharacterRecipe::default();
    let original = render(default);
    let tinted = render(crate::appearance::CharacterRecipe {
        iris: Some([255, 0, 0]),
        ..default
    });
    assert_ne!(original, tinted, "open irises must accept color");
    for row in 0..HEIGHT as usize {
        let start = row * WIDTH as usize * 4;
        assert_eq!(
            &original[start..start + WIDTH as usize * 2],
            &tinted[start..start + WIDTH as usize * 2],
            "neighbor recipe changed"
        );
        if row > 32 {
            assert_eq!(
                &original[start..start + WIDTH as usize * 4],
                &tinted[start..start + WIDTH as usize * 4],
                "iris tint changed body pixels"
            );
        }
    }
    assert_ne!(
        original,
        render(crate::appearance::CharacterRecipe { hair: 0, ..default })
    );
    assert_ne!(
        original,
        render(crate::appearance::CharacterRecipe { hair: 2, ..default })
    );
    for hair in 0..crate::appearance::HAIR.len() as u8 {
        if hair == default.hair {
            continue;
        }
        let changed = render(crate::appearance::CharacterRecipe { hair, ..default });
        assert_ne!(original, changed, "every selected hair style must draw");
        // Hair 0 sorts the right actor before the left one. Its origin, joints,
        // face and texture must remain together after the instance regrouping.
        for row in 0..HEIGHT as usize {
            let start = row * WIDTH as usize * 4;
            assert_eq!(
                &original[start..start + WIDTH as usize * 2],
                &changed[start..start + WIDTH as usize * 2],
                "hair grouping changed a neighbor instance"
            );
        }
    }
    assert!(!crate::appearance::CharacterRecipe { eyes: 1, ..default }.valid());
    assert!(
        !crate::appearance::CharacterRecipe {
            mouth: 1,
            ..default
        }
        .valid()
    );
}

#[test]
fn weather_fog_avatar_shader_validates_without_a_gpu() {
    let source = super::appearance::shader(crate::content::catalog());
    let module =
        wgpu::naga::front::wgsl::parse_str(&source).expect("valid avatar weather fog WGSL");
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("valid avatar weather fog shader module");
}

#[test]
fn both_bodies_all_hairstyles_support_independent_rgb_without_neighbor_changes() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let catalog = Catalog::builtins();
    let draw = |body, hair, color| {
        render_recipe(
            &device,
            &queue,
            &catalog,
            [0; 4],
            [1.0; 3],
            Some(("idle", 0.0)),
            Some(crate::appearance::CharacterRecipe {
                body,
                hair,
                hair_color: color,
                ..Default::default()
            }),
        )
    };
    for body in 0..2 {
        for hair in 0..14 {
            let red = draw(body, hair, [220, 35, 40]);
            let blue = draw(body, hair, [40, 100, 225]);
            if hair == 0 {
                assert_eq!(red, blue, "bald recipe must ignore hair color");
            } else {
                assert_ne!(red, blue, "body {body} hair {hair} must visibly recolor");
            }
            for row in 0..HEIGHT as usize {
                let start = row * WIDTH as usize * 4;
                if red[start..start + WIDTH as usize * 2] != blue[start..start + WIDTH as usize * 2]
                {
                    let directory = std::env::temp_dir().join(format!(
                        "bloxgloom-avatar-rgb-failure-{}",
                        std::process::id()
                    ));
                    std::fs::create_dir_all(&directory).unwrap();
                    for (name, pixels) in [("red", &red), ("blue", &blue)] {
                        let path = directory.join(format!("body-{body}-hair-{hair}-{name}.png"));
                        let file = std::fs::File::create(&path).unwrap();
                        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder
                            .write_header()
                            .unwrap()
                            .write_image_data(pixels)
                            .unwrap();
                        eprintln!("avatar RGB failure image: {}", path.display());
                    }
                }
                assert_eq!(
                    &red[start..start + WIDTH as usize * 2],
                    &blue[start..start + WIDTH as usize * 2],
                    "hair RGB leaked to another actor: body {body}, hair {hair}, row {row}"
                );
            }
        }
    }
    assert_ne!(
        draw(0, 0, [0; 3]),
        draw(1, 0, [0; 3]),
        "body selection must select distinct geometry"
    );
}
