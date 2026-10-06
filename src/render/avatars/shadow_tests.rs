//! Read back real color/depth passes to protect caster and receiver contracts.
use super::*;
use crate::render::{daylight::Atmosphere, sun_shadow};
use glam::Quat;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;

fn avatar(model: AvatarModel) -> VisualAvatar {
    VisualAvatar {
        motion: None,
        animation: Default::default(),
        model,
        pose: [0.0; 4],
        model_pose: None,
        character_pose: [0.3, 0.2, 0.7, 0.4],
        character_look: [0.2, -0.1],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: Some(Default::default()),
        airborne: false,
        id: 1,
        position: Vec3::ZERO,
        cosmetics: [0; 4],
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_color: [0; 3],
        glow_direction: [0; 3],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    }
}

struct Scene {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: AvatarRenderer,
    caster: wgpu::BindGroup,
    shadow_uniform: wgpu::Buffer,
    shadow_data: [f32; 24],
    shadow: wgpu::Texture,
    depth: wgpu::Texture,
    color: wgpu::Texture,
    readback: wgpu::Buffer,
    depth_copy: wgpu::RenderPipeline,
    depth_copy_group: wgpu::BindGroup,
}

impl Scene {
    fn new(catalog: &crate::content::Catalog) -> Self {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        eprintln!("avatar shadow GPU: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let matrix = glam::camera::rh::proj::directx::orthographic(-1.4, 1.4, -0.1, 1.9, 0.1, 10.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Vec3::Y);
        let mut atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
        atmosphere.sun = Vec3::Z;
        atmosphere.strength = 1.0;
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(
                &atmosphere.camera_data(matrix, Vec3::new(0.0, 0.0, 4.0)),
            ),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut shadow_data = [0.0; 24];
        shadow_data[..16].copy_from_slice(&matrix.to_cols_array());
        shadow_data[16..20].copy_from_slice(&[1.0 / WIDTH as f32, 40.0, 1.0, 0.0]);
        shadow_data[23] = 9.9;
        let mut padded_shadow = shadow_data.to_vec();
        padded_shadow.resize(crate::render::sun_shadow::UNIFORM_BYTES as usize / 4, 0.0);
        let shadow_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&padded_shadow),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let texture = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let shadow = texture(
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        let dummy = texture(DEPTH_FORMAT, wgpu::TextureUsages::TEXTURE_BINDING);
        let depth = texture(DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let color = texture(
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        // Downlevel OpenGL exposes comparison sampling but neither direct
        // depth copies nor depth textureLoad. Reconstruct 24-bit depth with
        // nearest comparisons, then pack a float into RGBA8 for CPU checks.
        let copy_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("portable shadow depth readback"),
            source: wgpu::ShaderSource::Wgsl(r#"
                @group(0) @binding(0) var depth: texture_depth_2d;
                @group(0) @binding(1) var compare_depth: sampler_comparison;
                @vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                    let p = array<vec2f, 3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));
                    return vec4f(p[index],0.0,1.0);
                }
                @fragment fn fs(@builtin(position) position: vec4f) -> @location(0) vec4f {
                    let uv = position.xy / vec2f(textureDimensions(depth));
                    var low = 0.0;
                    var high = 1.0;
                    for (var step = 0u; step < 24u; step += 1u) {
                        let mid = (low + high) * 0.5;
                        if textureSampleCompareLevel(depth,compare_depth,uv,mid) > 0.5 {
                            low = mid;
                        } else { high = mid; }
                    }
                    let value = select(low,1.0,textureSampleCompareLevel(depth,compare_depth,uv,1.0) > 0.5);
                    let bits = bitcast<u32>(value);
                    return vec4f(vec4u(bits & 255u,(bits >> 8u) & 255u,(bits >> 16u) & 255u,bits >> 24u))/255.0;
                }
            "#.into()),
        });
        let depth_copy = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &copy_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &copy_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let copy_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let depth_copy_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &depth_copy.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &shadow.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&copy_sampler),
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let group = |view: &wgpu::TextureView| {
            sun_shadow::group(&device, &camera, &shadow_uniform, view, &sampler)
        };
        let caster = group(&dummy.create_view(&Default::default()));
        let mut renderer = AvatarRenderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8Unorm,
            &camera,
            catalog,
        );
        renderer.set_camera_group(group(&shadow.create_view(&Default::default())));
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            device,
            queue,
            renderer,
            caster,
            shadow_uniform,
            shadow_data,
            shadow,
            depth,
            color,
            readback,
            depth_copy,
            depth_copy_group,
        }
    }

    fn render(
        &mut self,
        avatars: &[VisualAvatar],
        caster: bool,
        occluded: bool,
        enabled: bool,
    ) -> Vec<u8> {
        self.render_with_visibility(avatars, caster, occluded, enabled, true)
    }

    fn render_with_visibility(
        &mut self,
        avatars: &[VisualAvatar],
        caster: bool,
        occluded: bool,
        enabled: bool,
        visible: bool,
    ) -> Vec<u8> {
        self.renderer.set(&self.queue, avatars);
        self.shadow_data[18] = f32::from(enabled);
        self.queue.write_buffer(
            &self.shadow_uniform,
            0,
            bytemuck::cast_slice(&self.shadow_data),
        );
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let view = self.shadow.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if occluded { 0.0 } else { 1.0 }),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if caster {
                assert!(self.renderer.draw_shadow(&mut pass, &self.caster) > 0);
            }
        }
        if caster {
            let color = self.color.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.depth_copy);
            pass.set_bind_group(0, &self.depth_copy_group, &[]);
            pass.draw(0..3, 0..1);
        }
        if !caster {
            let color = self.color.create_view(&Default::default());
            let depth = self.depth.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
            let draws = self.renderer.draw(&mut pass);
            if visible {
                assert!(draws > 0);
            } else {
                assert_eq!(draws, 0);
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let bytes = self.readback.slice(..).get_mapped_range().unwrap().to_vec();
        self.readback.unmap();
        bytes
    }
}

#[test]
fn gpu_character_casters_keep_full_world_rig_and_selected_hair_in_first_person() {
    let mut scene = Scene::new(&crate::content::Catalog::builtins());
    let mut actor = avatar(AvatarModel::Player);
    let mut previous = Vec::new();
    for (body, hair, crouch, tool) in [
        (0, 0, 0.0, None),
        (0, 9, 0.0, None),
        (1, 3, 1.0, Some((true, 0.3))),
    ] {
        actor.character_recipe = Some(crate::appearance::CharacterRecipe {
            body,
            hair,
            ..Default::default()
        });
        actor.character_crouch = crouch;
        actor.character_tool = tool;
        scene.renderer.set_first_person(None);
        let third_person = scene.render(&[actor], true, false, true);
        let visible = scene.render(&[actor], false, false, true);
        let depths: &[f32] = bytemuck::cast_slice(&third_person);
        assert!(
            depths.iter().filter(|depth| **depth < 1.0).count() > 300,
            "full caster must draw"
        );
        assert_ne!(
            third_person, previous,
            "body/hair/animation must affect shadow geometry"
        );
        for pitch in [-1.2, 0.0, 1.0] {
            scene.renderer.set_first_person(Some(FirstPersonView {
                id: actor.id,
                eye_height: 1.6 - crouch * 0.55,
                pitch,
            }));
            assert_eq!(
                third_person,
                scene.render(&[actor], true, false, true),
                "camera framing/head hiding leaked into world shadow"
            );
        }
        assert_ne!(
            visible,
            scene.render(&[actor], false, false, true),
            "the color pass must still use first-person clipping/framing"
        );
        previous = third_person;
    }
}

#[test]
fn gpu_avatar_receivers_remove_only_direct_sun_and_off_matches_unoccluded() {
    let mut scene = Scene::new(&crate::content::Catalog::builtins());
    for model in [
        AvatarModel::Player,
        AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE),
    ] {
        let mut actor = avatar(model);
        for levels in [[15, 0, 0, 0], [0; 4], [0, 15, 0, 0]] {
            actor.light_levels = levels;
            actor.glow_color = [255, 219, 153];
            actor.bounce = [25, 40, 10, 0];
            actor.glow_bounce = [18, 12, 6, 0];
            let lit = scene.render(&[actor], false, false, true);
            let shadow = scene.render(&[actor], false, true, true);
            let off = scene.render(&[actor], false, true, false);
            assert_eq!(
                lit, off,
                "Off must leave the calibrated base lighting unchanged"
            );
            if levels[0] == 0 {
                assert_eq!(
                    lit, shadow,
                    "cave floor, glow and bounced fill must not be sun-shadowed"
                );
            } else {
                assert_ne!(
                    lit, shadow,
                    "outdoor characters and public avatars must receive shadows"
                );
                assert!(
                    shadow
                        .chunks_exact(4)
                        .filter(|p| p[..3].iter().any(|c| *c > 8))
                        .count()
                        > 100,
                    "indirect light must remain visible"
                );
                assert!(
                    lit.iter().zip(&shadow).all(|(a, b)| a >= b),
                    "a shadow must never add light"
                );
            }
        }
    }
}

#[test]
fn gpu_public_casters_share_creature_and_rigid_animated_geometry() {
    let catalog = moving_tests::catalog([0.5, 0.1, 0.1]);
    let mut scene = Scene::new(&catalog);
    let mut creature = avatar(AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE));
    let resting = scene.render(&[creature], true, false, true);
    creature.pose = [0.6, 0.8, 0.1, 0.3];
    assert_ne!(resting, scene.render(&[creature], true, false, true));
    let mut rigid = avatar(AvatarModel::Moving(
        catalog.entity_type_id_by_key("demo:projectile").unwrap(),
    ));
    rigid.position.y = 0.8;
    rigid.motion = Some(MovingVisual {
        tick: 1,
        revision: 1,
        orientation: Quat::IDENTITY.to_array(),
        velocity: [0.0; 3],
        stopped: false,
    });
    let horizontal = scene.render(&[rigid], true, false, true);
    rigid.motion.as_mut().unwrap().orientation =
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
    assert_ne!(horizontal, scene.render(&[rigid], true, false, true));
}

#[test]
fn gpu_packaged_glb_casters_and_receivers_preserve_layers_clips_and_indirect_light() {
    let (catalog, id) = authored::tests::catalog();
    let mut scene = Scene::new(&catalog);
    let mut actor = authored::tests::avatar(91, id, 0.0, [240, 180, 100]);
    actor.model_pose.as_mut().unwrap().playback = Some(bloxgloom_host_api::entity::ClipPlayback {
        clip: 0,
        speed: 0.0,
        looping: true,
        crossfade_s: 0.0,
        started_tick: 0,
        sequence: 0,
    });
    let resting = scene.render(&[actor], true, false, true);
    actor.model_pose.as_mut().unwrap().layers[1] = 1;
    let hat = scene.render(&[actor], true, false, true);
    assert_ne!(
        resting, hat,
        "visible hat must cast its authored silhouette"
    );
    actor.model_pose.as_mut().unwrap().playback = Some(bloxgloom_host_api::entity::ClipPlayback {
        clip: 2,
        speed: 1.0,
        looping: false,
        crossfade_s: 0.0,
        started_tick: 0,
        sequence: 1,
    });
    actor.model_pose.as_mut().unwrap().sample_tick = 20;
    actor.model_pose.as_mut().unwrap().sequence = 1;
    assert_ne!(
        hat,
        scene.render(&[actor], true, false, true),
        "baked animation must also change caster geometry"
    );
    // Hold the authored idle at one exact pose while changing only lighting.
    // GPU readback duration under parallel tests must not advance the geometry.
    let mut scene = Scene::new(&catalog);
    let visual = actor.model_pose.as_mut().unwrap();
    visual.sequence = 2;
    visual.playback = Some(bloxgloom_host_api::entity::ClipPlayback {
        clip: 0,
        speed: 0.0,
        looping: true,
        crossfade_s: 0.0,
        started_tick: 0,
        sequence: 2,
    });
    for levels in [[15, 0, 0, 0], [0; 4], [0, 15, 0, 0]] {
        actor.light_levels = levels;
        actor.glow_color = [255, 219, 153];
        actor.bounce = [30, 40, 20, 0];
        actor.glow_bounce = [20, 15, 10, 0];
        let lit = scene.render(&[actor], false, false, true);
        let shaded = scene.render(&[actor], false, true, true);
        let off = scene.render(&[actor], false, true, false);
        assert_eq!(lit, off);
        if levels[0] == 0 {
            assert_eq!(lit, shaded);
        } else {
            assert_ne!(lit, shaded);
            assert!(lit.iter().zip(shaded).all(|(a, b)| *a >= b));
        }
    }
}

#[test]
fn packed_local_light_retains_color_and_signed_direction_without_extra_attributes() {
    let mut actor = avatar(AvatarModel::Player);
    actor.light_levels = [11, 13, 0, 0];
    actor.glow_color = [255, 83, 9];
    actor.glow_direction = [-127, 0, 64];
    let instance = AvatarInstance::from(&actor);
    assert_eq!(instance.light_levels[0].to_le_bytes(), [11, 13, 255, 83]);
    assert_eq!(instance.light_levels[1].to_le_bytes(), [9, 129, 0, 64]);
    assert_eq!(std::mem::size_of_val(&instance.light_levels), 8);
}

#[test]
fn gpu_all_actor_materials_receive_colored_directional_local_light() {
    let (catalog, kind) = authored::tests::catalog();
    let mut scene = Scene::new(&catalog);
    let mut authored = authored::tests::avatar(91, kind, 0.0, [240, 240, 240]);
    authored.model_pose.as_mut().unwrap().playback =
        Some(bloxgloom_host_api::entity::ClipPlayback {
            clip: 0,
            speed: 0.0,
            looping: true,
            crossfade_s: 0.0,
            started_tick: 0,
            sequence: 0,
        });
    for mut actor in [
        avatar(AvatarModel::Player),
        avatar(AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE)),
        authored,
    ] {
        actor.light_levels = [0, 15, 0, 0];
        actor.glow_color = [255, 0, 0];
        actor.glow_direction = [0, 0, 127];
        let red = scene.render(&[actor], false, false, false);
        actor.glow_color = [0, 0, 255];
        let blue = scene.render(&[actor], false, false, false);
        assert!(
            red != blue,
            "local source color must reach {:?}",
            actor.model
        );
        actor.glow_direction = [0, 0, -127];
        let behind = scene.render(&[actor], false, false, false);
        assert!(
            blue != behind,
            "local direction must reach {:?}",
            actor.model
        );
        actor.light_levels[1] = 0;
        let dark = scene.render(&[actor], false, false, false);
        actor.glow_color = [255; 3];
        actor.glow_direction = [0; 3];
        assert_eq!(
            dark,
            scene.render(&[actor], false, false, false),
            "zero local level must stay dark"
        );
    }
}

#[test]
fn gpu_procedural_and_authored_actors_project_moving_point_shadows() {
    let (catalog, id) = authored::tests::catalog();
    let mut authored = authored::tests::avatar(91, id, 0.0, [240; 3]);
    authored.model_pose.as_mut().unwrap().playback =
        Some(bloxgloom_host_api::entity::ClipPlayback {
            clip: 0,
            speed: 0.0,
            looping: true,
            crossfade_s: 0.0,
            started_tick: 0,
            sequence: 0,
        });
    for actor in [
        avatar(AvatarModel::Player),
        avatar(AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE)),
        authored,
    ] {
        crate::render::local_shadow::gpu_tests::verify_actor_projection(&catalog, actor);
    }
}

#[test]
fn gpu_primitive_first_person_owner_keeps_world_caster_and_visible_model_ranges() {
    let catalog = moving_tests::catalog([0.5, 0.1, 0.1]);
    let mut scene = Scene::new(&catalog);
    let mut owner = avatar(AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE));
    owner.pose = [0.6, 0.8, 0.1, 0.3];
    let full = scene.render(&[owner], true, false, true);
    assert!(
        full.chunks_exact(4).any(|p| p[0] < 255),
        "primitive owner must cast actual depth"
    );
    scene.renderer.set_first_person(Some(FirstPersonView {
        id: owner.id,
        eye_height: 1.6,
        pitch: 0.3,
    }));
    assert_eq!(
        full,
        scene.render(&[owner], true, false, true),
        "first-person framing must retain the complete current primitive caster"
    );
    let hidden = scene.render_with_visibility(&[owner], false, false, false, false);
    assert!(
        hidden.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]),
        "the primitive owner must stay hidden in the camera color pass"
    );
    let mut other = avatar(AvatarModel::Moving(
        catalog.entity_type_id_by_key("demo:projectile").unwrap(),
    ));
    other.id = 2;
    other.position.y = 0.8;
    other.motion = Some(MovingVisual {
        tick: 1,
        revision: 1,
        orientation: Quat::IDENTITY.to_array(),
        velocity: [0.0; 3],
        stopped: false,
    });
    let visible = scene.render(&[other], false, false, false);
    assert!(
        visible.chunks_exact(4).any(|p| p[..3] != [0, 0, 0]),
        "independent moving model must be visible"
    );
    assert_eq!(
        visible,
        scene.render(&[owner, other], false, false, false),
        "a hidden owner must not shift the following model's visible instance range"
    );
    owner.position.x = 0.4;
    assert_ne!(
        full,
        scene.render(&[owner], true, false, true),
        "world owner movement must update the current raster caster"
    );
}
