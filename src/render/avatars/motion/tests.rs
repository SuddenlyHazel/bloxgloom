use super::*;

#[test]
fn color_and_motion_positions_are_invariant_for_equal_depth() {
    use wgpu::naga::{Binding, BuiltIn, ShaderStage, TypeInner};
    let catalog = crate::content::catalog();
    for (name, source) in [
        ("procedural", super::super::appearance::shader(catalog)),
        ("character", super::super::character_shader(catalog)),
        (
            "authored",
            shader(
                crate::render::daylight::shader(include_str!("../authored.wgsl")),
                3,
            ),
        ),
    ] {
        let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        for entry in ["vs_main", "vs_motion"] {
            let result = module
                .entry_points
                .iter()
                .find(|point| point.stage == ShaderStage::Vertex && point.name == entry)
                .unwrap()
                .function
                .result
                .as_ref()
                .unwrap();
            let TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
                panic!("{name} {entry} must return a vertex output struct");
            };
            assert!(
                members.iter().any(|member| matches!(
                    member.binding,
                    Some(Binding::BuiltIn(BuiltIn::Position { invariant: true }))
                )),
                "{name} {entry} must preserve position for the Equal-depth motion pass"
            );
        }
    }
}

fn stage(history: &mut History, id: u64, identity: u64, x: f32) {
    history.stage(
        id,
        identity,
        Vec3::new(x, 0.0, 0.0),
        vec![Mat4::from_translation(Vec3::new(x, 0.0, 0.0))],
    );
}
#[test]
fn motion_tracks_presented_identity_through_repacking_and_skipped_frames() {
    let mut history = History::default();
    stage(&mut history, 1, 4, 0.0);
    stage(&mut history, 2, 4, 4.0);
    assert_eq!(history.palette(true), vec![[0.0; 16]; 2]);
    history.submitted();
    history.clear_pending();
    stage(&mut history, 2, 4, 4.2);
    stage(&mut history, 1, 4, 0.2);
    let previous = history.palette(true);
    assert_eq!(previous[0][12], 4.0);
    assert_eq!(previous[1][12], 0.0);
    history.clear_pending(); // Preparation was skipped; no nonexistent pose.
    stage(&mut history, 1, 4, 0.4);
    assert_eq!(history.palette(true)[0][12], 0.0);
    history.submitted();
    history.clear_pending();
    stage(&mut history, 1, 4, 0.6);
    stage(&mut history, 2, 4, 4.3); // Disappeared from last presented set.
    assert_eq!(history.palette(true)[0][12], 0.4);
    assert_eq!(history.palette(true)[1], [0.0; 16]);
}
#[test]
fn teleports_appearance_changes_camera_cuts_and_nonfinite_poses_are_reactive() {
    let mut history = History::default();
    stage(&mut history, 1, 4, 0.0);
    history.submitted();
    for (identity, x, valid) in [
        (5, 0.1, true),
        (4, 2.0, true),
        (4, 0.1, false),
        (4, f32::NAN, true),
    ] {
        history.clear_pending();
        stage(&mut history, 1, identity, x);
        assert_eq!(history.palette(valid), vec![[0.0; 16]]);
    }
}

/// Uses production character and packaged-creature vertex/alpha pipelines. The
/// camera is fixed: any nonzero velocity must come from actual object motion.
#[test]
fn gpu_motion_covers_skinning_translation_repacking_and_first_person_resets() {
    use crate::render::avatars::{AvatarModel, AvatarRenderer, authored};
    use wgpu::util::DeviceExt;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (catalog, kind) = authored::tests::catalog();
    let mut player = authored::tests::avatar(1, kind, -0.65, [255; 3]);
    player.model = AvatarModel::Player;
    player.model_pose = None;
    player.character_recipe = Some(Default::default());
    let creature = authored::tests::avatar(2, kind, 0.65, [255; 3]);
    let mut actors = [player, creature];
    let size = wgpu::Extent3d {
        width: 128,
        height: 96,
        depth_or_array_layers: 1,
    };
    let matrix = glam::camera::rh::proj::directx::orthographic(-1.4, 1.4, -0.1, 1.9, 0.1, 10.0)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Vec3::Y);
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("motion test camera"),
        contents: bytemuck::cast_slice(
            &crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS)
                .camera_data(matrix, Vec3::new(0.0, 0.0, 4.0)),
        ),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let mut renderer = AvatarRenderer::new(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8Unorm,
        &camera,
        &catalog,
    );
    renderer.preview_animation_dt(0.0);
    renderer.enable_motion(true);
    renderer.preview_character_clip("walk", 0.0);
    let texture = |format| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("motion test target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let color = texture(wgpu::TextureFormat::Rgba8Unorm).create_view(&Default::default());
    let depth = texture(crate::render::DEPTH_FORMAT).create_view(&Default::default());
    let motion = texture(FORMAT).create_view(&Default::default());
    let render = |renderer: &mut AvatarRenderer, actors: &[_], jitter: glam::Vec2| {
        let raster =
            crate::render::post::temporal::jitter_matrix(matrix, jitter, size.width, size.height);
        queue.write_buffer(
            &camera,
            0,
            bytemuck::cast_slice(
                &crate::render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS)
                    .camera_data(raster, Vec3::new(0.0, 0.0, 4.0)),
            ),
        );
        renderer.set(&queue, actors);
        renderer.prepare_motion(
            &queue,
            &Frame {
                previous: matrix.to_cols_array(),
                viewport: [128.0, 96.0, jitter.x, jitter.y],
                depth: [0.1, 10.0, 1.0, 0.0],
            },
        );
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(size.width * size.height * 8),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[attachment(&color)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            renderer.draw(&mut pass);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[attachment(&motion)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            renderer.draw_motion(&mut pass);
        }
        encoder.copy_texture_to_buffer(
            motion.texture().as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size.width * 8),
                    rows_per_image: Some(size.height),
                },
            },
            size,
        );
        queue.submit(Some(encoder.finish()));
        renderer.submitted();
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .unwrap()
            .chunks_exact(8)
            .map(|pixel| {
                std::array::from_fn::<_, 4, _>(|i| {
                    let half = u16::from_le_bytes([pixel[i * 2], pixel[i * 2 + 1]]);
                    let magnitude = half & 0x7fff;
                    if magnitude < 0x400 {
                        0.0
                    } else {
                        f32::from_bits(
                            (u32::from(half & 0x8000) << 16)
                                | (((u32::from(magnitude) >> 10) + 112) << 23)
                                | ((u32::from(magnitude) & 1023) << 13),
                        )
                    }
                })
            })
            .collect::<Vec<_>>();
        readback.unmap();
        pixels
    };
    let first = render(&mut renderer, &actors, glam::Vec2::ZERO);
    assert!(first.iter().filter(|p| p[3] < 0.0).count() > 100);
    assert!(!first.iter().any(|p| p[3] > 0.0));
    let still = render(&mut renderer, &actors, glam::Vec2::ZERO);
    assert!(still.iter().filter(|p| p[3] > 0.0).count() > 100);
    assert!(still.iter().all(|p| p[0].abs() < 0.03 && p[1].abs() < 0.03));
    for frame in 0..8 {
        let pixels = render(
            &mut renderer,
            &actors,
            crate::render::post::temporal::jitter(frame),
        );
        assert!(
            pixels
                .iter()
                .all(|p| p[0].abs() < 0.03 && p[1].abs() < 0.03),
            "jitter {frame} created false object motion: {:?}",
            pixels
                .iter()
                .copied()
                .max_by(|a, b| (a[0].abs() + a[1].abs()).total_cmp(&(b[0].abs() + b[1].abs())))
        );
    }
    actors[0].position.x += 0.15;
    actors[1].position.x -= 0.15;
    let translated = render(&mut renderer, &actors, glam::Vec2::ZERO);
    let expected = 0.15 * 128.0 / 2.8;
    assert!(
        translated
            .iter()
            .filter(|p| p[3] > 0.0 && (p[0] + expected).abs() < 0.05)
            .count()
            > 100,
        "character translation missing"
    );
    assert!(
        translated
            .iter()
            .filter(|p| p[3] > 0.0 && (p[0] - expected).abs() < 0.05)
            .count()
            > 50,
        "GLB translation missing"
    );
    renderer.preview_character_clip("walk", 0.23);
    let visual = actors[1].model_pose.as_mut().unwrap();
    visual.playback = Some(bloxgloom_host_api::entity::ClipPlayback {
        clip: 2,
        speed: 1.0,
        looping: true,
        crossfade_s: 0.0,
        started_tick: 0,
        sequence: 1,
    });
    visual.sample_tick = 15;
    let skinned = render(&mut renderer, &actors, glam::Vec2::ZERO);
    for side in 0..2 {
        assert!(
            skinned
                .iter()
                .enumerate()
                .filter(|(i, p)| i % 128 / 64 == side
                    && p[3] > 0.0
                    && p[0].abs() + p[1].abs() > 0.1)
                .count()
                > 10,
            "actor side {side} has no deformation velocity"
        );
    }
    actors.reverse();
    let reordered = render(&mut renderer, &actors, glam::Vec2::ZERO);
    assert!(
        reordered
            .iter()
            .all(|p| p[0].abs() < 0.03 && p[1].abs() < 0.03),
        "packed instance order leaked into previous pose lookup"
    );
    renderer.set_first_person(Some(crate::render::avatars::FirstPersonView {
        id: 1,
        eye_height: 1.6,
        pitch: -0.5,
    }));
    let first_person = render(&mut renderer, &actors, glam::Vec2::ZERO);
    assert!(
        first_person.iter().any(|p| p[3] < 0.0),
        "new view mode must reject body history"
    );
    assert!(
        first_person.iter().any(|p| p[3] > 0.0),
        "unchanged GLB neighbor lost its history"
    );
    renderer.set_first_person(None);
    let mut primitive = player;
    primitive.id = 3;
    primitive.model = AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE);
    primitive.position = Vec3::ZERO;
    render(&mut renderer, &[primitive], glam::Vec2::ZERO);
    primitive.pose = [0.2, 0.7, 0.1, 0.12];
    let procedural = render(&mut renderer, &[primitive], glam::Vec2::ZERO);
    assert!(
        procedural
            .iter()
            .filter(|p| p[3] > 0.0 && p[0].abs() + p[1].abs() > 0.1)
            .count()
            > 10,
        "procedural foot, squash and ear motion must retain previous pose"
    );
    renderer.enable_motion(false);
    renderer.set(&queue, &actors);
    renderer.submitted();
    assert!(renderer.motion.history.pending.is_empty());
    assert!(renderer.characters.motion.history.pending.is_empty());
    renderer.enable_motion(true);
    let enabled_again = render(&mut renderer, &actors, glam::Vec2::ZERO);
    assert!(enabled_again.iter().any(|p| p[3] < 0.0));
    assert!(
        !enabled_again.iter().any(|p| p[3] > 0.0),
        "reenabling motion reused stale actor poses"
    );
}
