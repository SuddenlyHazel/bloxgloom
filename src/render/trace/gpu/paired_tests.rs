//! Actual submitted histories retain static convergence while native poses move.
#[path = "paired_tests/history.rs"]
mod history_tests;
#[path = "paired_tests/lobes.rs"]
mod lobe_tests;
#[path = "paired_tests/scheduling.rs"]
mod scheduling_tests;
#[path = "paired_tests/vector.rs"]
mod vector_tests;
use super::*;
use crate::render::trace::{
    dynamic::{DynamicAsset, DynamicInstance, DynamicTargets, Material, Vertex},
    scene::{Chunk, Triangle, surface},
    tests::{denoise, transport::Fixture},
};
use std::sync::Arc;

fn texture(f: &Fixture, width: u32, height: u32, data: &[[f32; 4]]) -> wgpu::TextureView {
    f.device
        .create_texture_with_data(
            &f.queue,
            &wgpu::TextureDescriptor {
                label: Some("paired current receiver"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(data),
        )
        .create_view(&Default::default())
}
fn read(f: &Fixture, texture: &wgpu::TextureView) -> Vec<[f32; 4]> {
    let source = r#"
@group(0) @binding(0) var input:texture_2d<f32>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(p*2.0-1.0,0.0,1.0);
}
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {return textureLoad(input,vec2i(p.xy),0);}
"#;
    let layout = f
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(texture),
        }],
    });
    let size = texture.texture().size();
    denoise::draw(
        &f.device,
        &f.queue,
        source,
        size.width,
        size.height,
        &[&group],
        &[Some(&layout)],
    )
}
fn quad(z: f32, material: u32, uv: [f32; 2]) -> Vec<Triangle> {
    let points = [
        [-64.0, -64.0, z],
        [64.0, -64.0, z],
        [64.0, 64.0, z],
        [-64.0, 64.0, z],
    ];
    [[0, 1, 2], [0, 2, 3]]
        .into_iter()
        .map(|ids| Triangle {
            a: [points[ids[0]][0], points[ids[0]][1], z, material as f32],
            b: [points[ids[1]][0], points[ids[1]][1], z, 0.0],
            c: [points[ids[2]][0], points[ids[2]][1], z, 0.0],
            uv_ab: [uv[0], uv[1], uv[0], uv[1]],
            uv_c: uv,
            normal: [0.0, 0.0, 1.0, 0.0],
            surface_color: 0,
            surface_flags: 0,
        })
        .collect()
}
fn drop_target(material: u32) -> DynamicInstance {
    let positions = [
        [-2.0, -2.0, 0.0],
        [2.0, -2.0, 0.0],
        [2.0, 2.0, 0.0],
        [-2.0, 2.0, 0.0],
    ];
    let vertices = positions
        .into_iter()
        .map(|position| Vertex {
            position,
            normal: [0.0, 0.0, 1.0],
            uv: [0.5; 2],
            joints: [0; 4],
            weights: [0.0; 4],
            part: 0,
        })
        .collect();
    let mut native = Material::flat([1.0; 3]);
    native.kind = 3;
    native.catalog_layer = material;
    native.double_sided = true;
    DynamicInstance::rigid(
        DynamicAsset::build(
            vertices,
            vec![([0, 1, 2], 0, 0), ([0, 2, 3], 0, 0)],
            vec![native],
            Vec::new(),
        ),
        Mat4::from_translation(Vec3::new(0.0, 0.0, -2.5)),
        1.0,
    )
}
fn ready(f: &Fixture, gpu: &mut Gpu, targets: &DynamicTargets) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !gpu.prepare_dynamic(&f.device, &f.queue, targets) {
        assert!(
            std::time::Instant::now() < deadline,
            "current native asset admission timed out"
        );
        std::thread::yield_now();
    }
}
#[test]
fn gpu_paired_history_survives_native_idle_and_applies_current_offscreen_drop_removal() {
    let catalog = crate::content::Catalog::builtins();
    let f = Fixture::new(&catalog);
    let id = |stem: &str| {
        catalog
            .textures()
            .iter()
            .position(|t| t.key.ends_with(&format!(":{stem}")))
            .unwrap() as u32
    };
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
        "../../../../assets/textures/blocks/jg_deepslate_iron_ore_s.png"
    )));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    let at = bytes[..info.buffer_size()]
        .chunks_exact(4)
        .position(|p| p[1] >= 230)
        .unwrap();
    let uv = [
        (at % info.width as usize) as f32 / info.width as f32 + 0.5 / info.width as f32,
        (at / info.width as usize) as f32 / info.height as f32 + 0.5 / info.height as f32,
    ];
    let mut triangles = quad(1.0, id("jg_deepslate_iron_ore"), uv);
    triangles.extend(quad(-4.0, 0, [0.5; 2]).into_iter().map(|t| {
        t.with_surface(
            [1.0; 4],
            surface::LOD | surface::COARSE_COLOR | surface::NO_WIND | (15 << surface::GLOW_SHIFT),
        )
    }));
    let scene = Scene::build([Arc::new(Chunk {
        triangles,
        ..Default::default()
    })]);
    let width = 32;
    let height = 16;
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mut gpu = Gpu::new_with_lod(&f.device, &scene, &[], size, &f.material_layout);
    let receivers = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let nx = (x as f32 + 0.5) / width as f32 * 2.0 - 1.0;
                let ny = 1.0 - (y as f32 + 0.5) / height as f32 * 2.0;
                [1.0, 1.0, 0.18, (nx * nx + ny * ny + 1.0).sqrt()]
            })
        })
        .collect::<Vec<_>>();
    let normal = texture(&f, width, height, &receivers);
    let response = texture(
        &f,
        width,
        height,
        &vec![[0.0; 4]; (width * height) as usize],
    );
    let indirect = texture(
        &f,
        width,
        height,
        &vec![[0.03, 0.03, 0.03, 1.0]; (width * height) as usize],
    );
    let hdr = f
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let depth = f
        .device
        .create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let mut targets = DynamicTargets {
        instances: vec![
            crate::render::avatars::idle_ray_target(0.0),
            drop_target(id("jg_cherry_log")),
        ],
    };
    ready(&f, &mut gpu, &targets);
    let fixed_seed = std::cell::Cell::new(None::<u32>);
    let submit = |gpu: &mut Gpu| {
        // Transport may submit inside resolve; fix RNG before encoding, retaining parity.
        gpu.sample_seed = fixed_seed.get();
        let mut encoder = f.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &hdr,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.2,
                            g: 0.1,
                            b: 0.05,
                            a: 1.0,
                        }),
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
        }
        let mut atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
        atmosphere.cloud = 0.0;
        atmosphere.fog = 0.0;
        gpu.resolve(
            &f.device,
            &f.queue,
            &mut encoder,
            &hdr,
            &depth,
            &normal,
            &response,
            &indirect,
            &f.materials,
            Mat4::IDENTITY,
            Vec3::ZERO,
            atmosphere,
            0.0,
            None,
        );
        f.queue.submit([encoder.finish()]);
    };
    let initial_joints = targets.instances[0].joints.clone();
    for frame in 0..12 {
        targets.instances[0].joints =
            crate::render::avatars::idle_ray_target(frame as f32 / 60.0).joints;
        ready(&f, &mut gpu, &targets);
        submit(&mut gpu);
    }
    assert_ne!(
        initial_joints, targets.instances[0].joints,
        "real breathing must change the current ray pose"
    );
    let metadata = read(&f, &gpu.history_geometry[(gpu.frame as usize - 1) % 2]);
    assert!(
        metadata.iter().filter(|g| g[3] >= 10.0).count() > metadata.len() / 2,
        "breathing reset static history: {metadata:?}"
    );
    // Compare actual movement with identical random numbers, so fresh sampling
    // alone cannot make the movement assertion pass. Earlier breathing frames
    // retain production's monotonically advancing RNG.
    fixed_seed.set(Some(444));
    gpu.history_valid = false; // Compare fresh paired/static samples at one RNG state.
    submit(&mut gpu);
    let static_reference = read(&f, &gpu.history[(gpu.frame as usize - 1) % 2]);
    let reflected = read(&f, &gpu.current_correction);
    assert!(
        reflected
            .iter()
            .any(|p| p[..3].iter().any(|v| v.abs() > 0.01)),
        "current behind-camera drop produced no paired reflection correction"
    );
    targets.instances[1].world = Mat4::from_translation(Vec3::new(30.0, 0.0, -2.5));
    ready(&f, &mut gpu, &targets);
    assert!(gpu.history_valid, "a moved drop must retain static history");
    submit(&mut gpu);
    let moved = read(&f, &gpu.current_correction);
    assert_ne!(
        reflected, moved,
        "current off-screen movement must alter reflection immediately"
    );
    // A primary actor is a current sample, not an admissible static receiver.
    targets.instances[1].world = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5));
    let foreground = receivers
        .iter()
        .map(|r| [r[0], r[1], r[2], r[3] * 0.5])
        .collect::<Vec<_>>();
    let write_receiver = |data: &[[f32; 4]]| {
        f.queue.write_texture(
            normal.texture().as_image_copy(),
            bytemuck::cast_slice(data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 16),
                rows_per_image: Some(height),
            },
            size,
        )
    };
    write_receiver(&foreground);
    ready(&f, &mut gpu, &targets);
    submit(&mut gpu);
    let actor = read(&f, &gpu.history_geometry[(gpu.frame as usize - 1) % 2]);
    assert!(
        actor.iter().all(|g| g[3] == -2.0),
        "primary actors entered static history: {actor:?}"
    );
    targets.instances[1].world = Mat4::from_translation(Vec3::new(30.0, 0.0, -2.5));
    write_receiver(&receivers);
    ready(&f, &mut gpu, &targets);
    submit(&mut gpu);
    let exposed = read(&f, &gpu.history_geometry[(gpu.frame as usize - 1) % 2]);
    assert!(
        exposed.iter().all(|g| g[3] == 1.0),
        "an exposed static receiver reused old actor history: {exposed:?}"
    );
    targets.instances.clear();
    ready(&f, &mut gpu, &targets);
    submit(&mut gpu);
    let removed = read(&f, &gpu.current_correction);
    let absent_reference = read(&f, &gpu.history[(gpu.frame as usize - 1) % 2]);
    assert_eq!(
        static_reference, absent_reference,
        "paired static replay must exactly match the actor/drop-free scene at the same RNG state"
    );
    assert!(
        removed.iter().all(|p| p[..3].iter().all(|v| *v == 0.0)),
        "removed actor/drop left a stale dynamic correction: {removed:?}"
    );
    let clear_samples = shadow_probe(&f, &gpu, &depth, &normal, &response, &indirect);
    let clear = clear_samples[0];
    assert!(
        clear_samples[1][1] > 0.0 && clear_samples[1][0] == 0.0 && clear_samples[1][2] == 0.0,
        "unoccluded primary medium must have an identical static sample: {clear_samples:?}"
    );
    assert!(
        clear[1] > 0.0 && clear[0] == 0.0 && clear[2] == 0.0,
        "clear sunlight must have no dynamic dependency: {clear:?}"
    );
    let sun = Atmosphere::at(crate::daylight::INITIAL_MS).sun;
    let mut blocker = drop_target(id("jg_cherry_log"));
    blocker.world = Mat4::from_rotation_translation(
        glam::Quat::from_rotation_arc(Vec3::Z, sun),
        Vec3::splat(128.0) + sun * 10.0,
    );
    targets.instances.push(blocker);
    ready(&f, &mut gpu, &targets);
    submit(&mut gpu);
    let shadow_samples = shadow_probe(&f, &gpu, &depth, &normal, &response, &indirect);
    let shadowed = shadow_samples[0];
    assert!(
        shadow_samples[1][0] < 0.0 && shadow_samples[1][1] > 0.0 && shadow_samples[1][2] == 1.0,
        "camera-medium shadow query lost dynamic dependency: {shadow_samples:?}"
    );
    assert!(
        shadowed[0] < 0.0 && shadowed[1] > 0.0 && shadowed[2] == 1.0 && shadowed[3] == 0.0,
        "current off-screen shadow failed paired dependency/correction: {shadowed:?}"
    );
    targets.instances.clear();
    ready(&f, &mut gpu, &targets);
    submit(&mut gpu);
    let clear_again = shadow_probe(&f, &gpu, &depth, &normal, &response, &indirect);
    assert_eq!(
        clear_samples, clear_again,
        "removed shadow caster left stale camera-medium or surface occlusion"
    );
}

// Uses production visibility, including the shared dynamic dependency flag.
fn shadow_probe(
    f: &Fixture,
    gpu: &Gpu,
    depth: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    response: &wgpu::TextureView,
    indirect: &wgpu::TextureView,
) -> Vec<[f32; 4]> {
    let source = format!(
        "{}\n{}",
        shaders::transport(),
        r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 if u32(pixel.x)==1u {
  var full=vec3f(0.0);var reference=vec3f(0.0);var touched=false;
  for(var i=0u;i<256u;i++) {
   ray_rng=911u+i*1973u;ray_dynamic_disabled=false;ray_dynamic_touched=false;
   full+=ray_primary_medium_sample(vec3f(128.0),-normalize(ray_frame.sun.xyz),30.0,vec3f(0.0),vec3f(0.0)).correction;
   touched=touched||ray_dynamic_touched;
   ray_rng=911u+i*1973u;ray_dynamic_disabled=true;
   reference+=ray_primary_medium_sample(vec3f(128.0),-normalize(ray_frame.sun.xyz),30.0,vec3f(0.0),vec3f(0.0)).correction;
  }
  return vec4f((full.r-reference.r)/256.0,reference.r/256.0,select(0.0,1.0,touched),full.r/256.0);
 }
 ray_rng=911u;ray_dynamic_disabled=false;ray_dynamic_touched=false;
 let full=sun_light(vec3f(128.0));let touched=ray_dynamic_touched;
 ray_dynamic_disabled=true;let reference=sun_light(vec3f(128.0));
 return vec4f(full.r-reference.r,reference.r,select(0.0,1.0,touched),full.r);
}
"#
    );
    draw_probe(f, gpu, [depth, normal, response, indirect], &source, 2)
}
fn draw_probe(
    f: &Fixture,
    gpu: &Gpu,
    views: [&wgpu::TextureView; 4],
    source: &str,
    width: u32,
) -> Vec<[f32; 4]> {
    let [depth, normal, response, indirect] = views;
    let bind = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    let mut entries = vec![
        bind(0, gpu.uniform.as_entire_binding()),
        bind(1, gpu.geometry.nodes.as_entire_binding()),
        bind(2, gpu.geometry.triangles.as_entire_binding()),
        bind(3, wgpu::BindingResource::TextureView(depth)),
        bind(4, wgpu::BindingResource::TextureView(normal)),
        bind(5, wgpu::BindingResource::TextureView(response)),
        bind(6, wgpu::BindingResource::TextureView(indirect)),
        bind(7, wgpu::BindingResource::TextureView(&gpu.history[0])),
        bind(
            8,
            wgpu::BindingResource::TextureView(&gpu.history_geometry[0]),
        ),
        bind(9, wgpu::BindingResource::TextureView(&gpu.baseline)),
        bind(10, gpu.geometry.coverage.as_entire_binding()),
        bind(
            15,
            wgpu::BindingResource::TextureView(&gpu.primary_transmission[0]),
        ),
    ];
    entries.extend(gpu.lod.entries());
    if let Some(reconstruction) = &gpu.water_reconstruction {
        entries.push(bind(
            16,
            wgpu::BindingResource::TextureView(&reconstruction.raw),
        ));
    }
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &gpu.layout,
        entries: &entries,
    });
    denoise::draw(
        &f.device,
        &f.queue,
        source,
        width,
        1,
        &[&group, &f.materials, &gpu.dynamic.group],
        &[
            Some(&gpu.layout),
            Some(&f.material_layout),
            Some(&gpu.dynamic.layout),
        ],
    )
}

#[path = "paired_tests/water_guidance.rs"]
mod water_guidance;
