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
 if ray_frame.counts.y==22u||ray_frame.counts.y==23u {
  return vec4f(f32(test_transport_count),result.transmission,result.radiance.r,result.geometry.w);
 }
 return vec4f(result.geometry.z,result.transmission,result.radiance.r,result.geometry.w);
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
        triangles,
        key: None,
    })]);
    let mut frame = vec![0.0f32; 72];
    frame[..16].copy_from_slice(&Mat4::IDENTITY.to_cols_array());
    frame[16..32].copy_from_slice(&Mat4::IDENTITY.to_cols_array());
    frame[44..48].copy_from_slice(&[0.1, 0.2, 0.3, 0.0]);
    frame[48..52].copy_from_slice(&[0.2, 0.3, 0.5, 1.0]);
    frame[60] = density;
    frame[62] = 2.0;
    frame[68] = f32::from_bits(scene.nodes.len() as u32);
    frame[69] = f32::from_bits(if forced_mode > 0 {
        forced_mode
    } else {
        u32::from(probe_alpha)
    });
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
    let entries = (0..11)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: if binding <= 2 || binding == 10 {
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
    let textures = [
        sample_texture(f, 4, 2, &receivers),
        sample_texture(f, 4, 2, &[[0.04, 0.04, 0.04, 1.0]; 8]),
        sample_texture(f, 4, 2, &[[0.03, 0.03, 0.03, marker]; 8]),
        sample_texture(f, 2, 1, &[[0.0; 4]; 2]),
        sample_texture(f, 2, 1, &[[0.0; 4]; 2]),
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
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
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
        ],
    });
    super::super::denoise::draw(
        &f.device,
        &f.queue,
        &source(2).replace(FIXTURE, PRIMARY),
        2,
        1,
        &[&group, &f.materials],
        &[Some(&layout), Some(&f.material_layout)],
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
            .all(|p| p[0] < 0.0 && p[1] == 1.0 && p[3] == 1.0)
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
            && p[2].abs() > 0.01
            && p.iter().all(|v| v.is_finite())),
        "unknown leaves still integrate camera media: {fog:?}"
    );
}
