//! Production primary transport against real imported leaf alpha/mip levels.
use super::*;
use glam::Mat4;

const PRIMARY: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 if ray_frame.counts.y==1u {
  let t=ray_triangles[0];let layer=i32(ray_materials[u32(t.a.w)].layer);
  return vec4f(textureSampleLevel(ray_albedo,ray_sampler,t.uv_ab.xy,layer,0.0).a,
   textureSampleLevel(ray_albedo,ray_sampler,t.uv_ab.xy,layer,f32(textureNumLevels(ray_albedo)-1u)).a,0.0,1.0);
 }
 let result=ray_primary_transport(pixel);
 if ray_frame.counts.y==44u {return vec4f(f32(test_primary_air_count),f32(test_transport_count),result.radiance.r,result.geometry.w);}
 if ray_frame.counts.y==22u||ray_frame.counts.y==23u {
  return vec4f(f32(test_transport_count),ray_primary_t(result.transmission),result.radiance.r,result.geometry.w);
 }
 return vec4f(result.geometry.z,ray_primary_t(result.transmission),result.radiance.r,result.geometry.w);
}
"#;

fn sample_texture(f: &Fixture, width: u32, height: u32, data: &[[f32; 4]]) -> wgpu::Texture {
    let t = f.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("primary raster receiver fixture"),
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
    });
    f.queue.write_texture(
        t.as_image_copy(),
        bytemuck::cast_slice(data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        t.size(),
    );
    t
}
fn run(
    f: &Fixture,
    triangles: Vec<Triangle>,
    marker: f32,
    density: f32,
    probe_alpha: bool,
) -> Vec<[f32; 4]> {
    run_mode(f, triangles, marker, density, probe_alpha, 0)
}

pub(super) fn run_mode(
    f: &Fixture,
    triangles: Vec<Triangle>,
    marker: f32,
    density: f32,
    probe_alpha: bool,
    forced_mode: u32,
) -> Vec<[f32; 4]> {
    let scene = Scene::build([Arc::new(Chunk {
        water: None,
        coarse_water: None,

        triangles,
        key: None,
    })]);
    run_scene(
        f,
        &scene,
        marker,
        density,
        if forced_mode > 0 {
            forced_mode
        } else {
            u32::from(probe_alpha)
        },
        Mat4::IDENTITY,
        false,
    )
}

pub(super) struct PrimaryCamera {
    pub inverse: Mat4,
    pub eye: glam::Vec3,
}
impl From<Mat4> for PrimaryCamera {
    fn from(inverse: Mat4) -> Self {
        Self {
            eye: inverse.w_axis.truncate(),
            inverse,
        }
    }
}
pub(super) fn run_scene(
    f: &Fixture,
    scene: &Scene,
    marker: f32,
    density: f32,
    mode: u32,
    camera: impl Into<PrimaryCamera>,
    eye_water: bool,
) -> Vec<[f32; 4]> {
    let camera = camera.into();
    let inverse = camera.inverse;
    let eye = camera.eye;
    let mut frame = vec![0.0f32; 76];
    // Callers provide their world inverse; production traces camera-relative
    // vectors and adds the eye only when reconstructing a world position.
    let relative_inverse = (inverse.inverse() * glam::Mat4::from_translation(eye)).inverse();
    frame[..16].copy_from_slice(&relative_inverse.to_cols_array());
    frame[32..35].copy_from_slice(&eye.to_array());
    frame[73] = f32::from_bits(scene.water_offset);
    frame[74] = f32::from(eye_water);
    frame[75] = 1.0; // Production plane-aware complete-water history.
    frame[16..32].copy_from_slice(&inverse.inverse().to_cols_array());
    frame[64..67].copy_from_slice(&eye.to_array());
    frame[36..40].copy_from_slice(&[0.0, 1.0, 0.0, 1.0]);
    if (40..=43).contains(&mode) {
        frame[71] = f32::from_bits(1);
    }
    frame[44..48].copy_from_slice(&[0.1, 0.2, 0.3, 0.0]);
    frame[48..52].copy_from_slice(&[0.2, 0.3, 0.5, 1.0]);
    frame[60] = density;
    frame[62] = 2.0;
    frame[68] = f32::from_bits(scene.nodes.len() as u32);
    frame[69] = f32::from_bits(mode);
    let buffer = |data: &[u8], usage| {
        f.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: data,
                usage,
            })
    };
    let uniform = buffer(bytemuck::cast_slice(&frame), wgpu::BufferUsages::UNIFORM);
    let nodes = buffer(
        bytemuck::cast_slice(&scene.nodes),
        wgpu::BufferUsages::STORAGE,
    );
    let triangles = buffer(
        bytemuck::cast_slice(&scene.triangles),
        wgpu::BufferUsages::STORAGE,
    );
    let coverage = buffer(
        bytemuck::cast_slice(&scene.coverage),
        wgpu::BufferUsages::STORAGE,
    );
    let entries = (0..16)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: if binding <= 2 || (10..=14).contains(&binding) {
                wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                }
            } else {
                wgpu::BindingType::Texture {
                    sample_type: if binding == 3 {
                        wgpu::TextureSampleType::Depth
                    } else {
                        wgpu::TextureSampleType::Float { filterable: false }
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }
            },
            count: None,
        })
        .collect::<Vec<_>>();
    let layout = f
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
    let receivers = (0..2)
        .flat_map(|y| {
            (0..4).map(move |x| {
                let nx = (x as f32 + 0.5) / 4.0 * 2.0 - 1.0;
                let ny = 1.0 - (y as f32 + 0.5);
                [0.0, 0.0, 0.65, (nx * nx + ny * ny + 1.0).sqrt()]
            })
        })
        .collect::<Vec<_>>();
    let history = (0..2)
        .map(|x| {
            if !(40..=43).contains(&mode) {
                return [0.0; 4];
            }
            let nx = ((x * 2 + 1) as f32 + 0.5) / 4.0 * 2.0 - 1.0;
            let far = inverse * glam::Vec4::new(nx, -0.5, 1.0, 1.0);
            let direction = (far.truncate() / far.w - eye).normalize();
            [
                4.0,
                2.0,
                1.0,
                2.0 / (-direction.y) + if mode == 41 { 1.0 } else { 0.0 },
            ]
        })
        .collect::<Vec<_>>();
    let history_geometry = if (40..=43).contains(&mode) {
        [
            0.0,
            if mode == 43 { -1.0 } else { 1.0 },
            if mode == 42 { -0.12 } else { -3.12 },
            8.0,
        ]
    } else {
        [0.0; 4]
    };
    let textures = [
        sample_texture(f, 4, 2, &receivers),
        sample_texture(f, 4, 2, &[[0.04, 0.04, 0.04, 1.0]; 8]),
        sample_texture(f, 4, 2, &[[0.03, 0.03, 0.03, marker]; 8]),
        sample_texture(f, 2, 1, &history),
        sample_texture(f, 2, 1, &[history_geometry; 2]),
        sample_texture(f, 4, 2, &[[1.0, 1.0, 1.0, 1.0]; 8]),
    ];
    let views = textures
        .iter()
        .map(|t| t.create_view(&Default::default()))
        .collect::<Vec<_>>();
    let depth = f.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 4,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = depth.create_view(&Default::default());
    let mut encoder = f.device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
    f.queue.submit([encoder.finish()]);
    let mut bindings = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: nodes.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: triangles.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(&depth),
        },
        wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::TextureView(&views[0]),
        },
        wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::TextureView(&views[1]),
        },
        wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::TextureView(&views[2]),
        },
        wgpu::BindGroupEntry {
            binding: 7,
            resource: wgpu::BindingResource::TextureView(&views[3]),
        },
        wgpu::BindGroupEntry {
            binding: 8,
            resource: wgpu::BindingResource::TextureView(&views[4]),
        },
        wgpu::BindGroupEntry {
            binding: 9,
            resource: wgpu::BindingResource::TextureView(&views[5]),
        },
        wgpu::BindGroupEntry {
            binding: 10,
            resource: coverage.as_entire_binding(),
        },
    ];
    bindings.push(wgpu::BindGroupEntry {
        binding: 15,
        resource: wgpu::BindingResource::TextureView(&views[3]),
    });
    f.append_empty_pages(&mut bindings);
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &bindings,
    });
    super::super::denoise::draw(
        &f.device,
        &f.queue,
        &source(12).replace(FIXTURE, PRIMARY).replace("fn ray_primary_medium_sample(origin:vec3f,direction:vec3f,distance:f32,color:vec3f,correction:vec3f)->RayPrimaryMedium {", "var<private> test_primary_air_count:u32;\nfn ray_primary_medium_sample(origin:vec3f,direction:vec3f,distance:f32,color:vec3f,correction:vec3f)->RayPrimaryMedium {test_primary_air_count++;"),
        2,
        1,
        &[&group, &f.materials, &f.dynamic.group],
        &[
            Some(&layout),
            Some(&f.material_layout),
            Some(&f.dynamic.layout),
        ],
    )
}

#[test]
fn gpu_primary_ray_alpha_mismatch_preserves_leaf_fallback_and_distinguishes_water() {
    let catalog = crate::content::catalog();
    let state = catalog
        .state(catalog.state_by_key("bloxgloom:cherry_leaves").unwrap())
        .unwrap();
    let layer = state.face_texture(1, 1).unwrap().get();
    let texture = catalog
        .texture(crate::content::TextureId::new(layer))
        .unwrap();
    let [hole, solid] = alpha_samples(&texture.png);
    let f = Fixture::new(catalog);
    let alpha = run(
        &f,
        plane(-2.0, 2.0, 1.0, layer, hole, true),
        -1.0,
        0.0,
        true,
    );
    assert!(
        alpha[0][0] < 0.5 && alpha[0][1] >= 0.5,
        "real filtered leaf alpha can be raster-visible while mip0 ray alpha rejects it: {:?}",
        alpha[0]
    );
    for marker in [-1.0, -0.5, 1.0] {
        let leaves = run(
            &f,
            plane(-2.0, 2.0, 1.0, layer, hole, true),
            marker,
            0.0,
            false,
        );
        for pixel in leaves {
            assert!(
                (pixel[0] - 0.65).abs() < 0.0001,
                "reactive leaves remain opaque class"
            );
            assert_eq!(pixel[1], -1.0, "retain existing specular fallback");
            assert_eq!(
                pixel[2], 0.0,
                "no false water reflection or ambient replacement"
            );
            assert_eq!(pixel[3], -1.0, "unmatched surfaces must reject history");
        }
    }
    let water = run(
        &f,
        plane(-2.0, 2.0, 1.0, layer, hole, true),
        -2.0,
        0.0,
        false,
    );
    assert!(
        water
            .iter()
            .all(|p| p[0] < 0.0 && p[1] == -1.0 && p[2] == 0.0 && p[3] == -1.0),
        "a water raster marker without an admitted real interface retains its fallback"
    );
    let matched = run(
        &f,
        plane(-2.0, 2.0, 1.0, layer, solid, true),
        -1.0,
        0.0,
        false,
    );
    assert!(
        matched
            .iter()
            .all(|p| p[0] > 0.0 && p[1] == 1.0 && p[3] == 1.0 && p.iter().all(|v| v.is_finite()))
    );
    let depth_mismatch = run(
        &f,
        plane(-2.0, 2.0, 0.5, layer, solid, true),
        -1.0,
        0.0,
        false,
    );
    assert!(
        depth_mismatch
            .iter()
            .all(|p| p[0] > 0.0 && p[1] == -1.0 && p[2] == 0.0 && p[3] == -1.0)
    );
    let fog = run(
        &f,
        plane(-2.0, 2.0, 1.0, layer, hole, true),
        -1.0,
        0.05,
        false,
    );
    assert!(
        fog.iter().all(|p| p[1] < 0.0
            && p[1] > -1.0
            // Static history excludes current raster HDR extinction. A sealed,
            // nonemissive air path can have zero incident radiance; signed T
            // must still attenuate its current HDR at exact-center composition.
            && p[2] >= 0.0
            && p.iter().all(|v| v.is_finite())),
        "unknown leaves still integrate camera media: {fog:?}"
    );
}
