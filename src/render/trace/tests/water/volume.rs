//! Actual appended medium records, production lookup and boundary traversal.
#[path = "volume/indexed.rs"]
mod indexed;
use super::{device, draw};
use crate::{lod, render::trace::scene, world};
use std::sync::Arc;
use wgpu::util::DeviceExt;

fn records() -> scene::Scene {
    let key = world::ChunkKey { x: -1, y: 0, z: -1 };
    let mut voxels = world::Chunk::from_blocks(key, 1, vec![world::AIR; world::CHUNK_VOLUME]);
    voxels
        .blocks
        .set(world::Chunk::index([3, 7, 11]).unwrap(), world::WATER);
    let mixed = Arc::new(scene::Chunk {
        key: Some(key),
        water: scene::water::Occupancy::from_chunk(&voxels, crate::content::catalog()),
        ..Default::default()
    });
    let dry = Arc::new(scene::Chunk {
        key: Some(world::ChunkKey { x: -2, y: 0, z: -1 }),
        ..Default::default()
    });
    let mut columns = vec![lod::Column::default(); lod::TILE_COLUMNS];
    // Valid selected server summary, broad enough to overlap both loaded
    // chunks. Every column retains an actual vertical unknown interval.
    for column in &mut columns {
        column.coverage = vec![
            lod::Interval {
                bottom: -8,
                top: 20,
            },
            lod::Interval {
                bottom: 24,
                top: 32,
            },
        ];
        column.spans = vec![lod::Span {
            bottom: -8,
            top: 20,
            state: world::WATER,
            sky: 15,
            glow: 0,
        }];
    }
    let tile = lod::LodTile {
        key: lod::TileKey {
            level: 2,
            x: -1,
            z: -1,
        },
        revision: 1,
        columns,
        trees: vec![],
        geometric_error: 4,
    };
    let coarse = Arc::new(scene::Chunk {
        coarse_water: scene::water::CoarseTile::from_lod(&tile, crate::content::catalog()),
        ..Default::default()
    });
    assert!(coarse.coarse_water.is_some());
    let near = [
        mixed,
        dry,
        Arc::new(scene::Chunk {
            key: Some(world::ChunkKey { x: 32, y: 0, z: 32 }),
            ..Default::default()
        }),
        Arc::new(scene::Chunk {
            key: Some(world::ChunkKey { x: 31, y: 0, z: 32 }),
            ..Default::default()
        }),
    ];
    let mut coarse_tiles = vec![coarse];
    for x in [3, 4] {
        let columns = vec![
            lod::Column {
                coverage: vec![lod::Interval { bottom: 0, top: 16 }],
                spans: vec![]
            };
            lod::TILE_COLUMNS
        ];
        let tile = lod::LodTile {
            key: lod::TileKey { level: 2, x, z: 4 },
            revision: 1,
            columns,
            trees: vec![],
            geometric_error: 4,
        };
        coarse_tiles.push(Arc::new(scene::Chunk {
            coarse_water: scene::water::CoarseTile::from_lod(&tile, crate::content::catalog()),
            ..Default::default()
        }));
    }
    let mut result = scene::Scene::build(near.iter().cloned());
    scene::volume::append(&mut result, &near, &coarse_tiles);
    result
}
const PROBES: &str = r#"
struct Frame{water:vec4f};
@group(0) @binding(0) var<uniform> ray_frame:Frame;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(xy*2.0-1.0,0.0,1.0);
}
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {
 switch u32(p.x) {
 case 0u:{return vec4f(f32(ray_water_at(vec3f(-12.5,7.5,-4.5))),f32(ray_water_at(vec3f(-12.5,11.5,-8.5))),f32(ray_water_at(vec3f(-28.0,7.5,-4.5))),f32(ray_water_at(vec3f(1.0,7.5,-4.5))));}
 case 1u:{return vec4f(f32(ray_water_at(vec3f(-40.0,18.0,-40.0))),f32(ray_water_at(vec3f(-40.0,22.0,-40.0))),f32(ray_water_at(vec3f(-40.0,26.0,-40.0))),f32(ray_water_at(vec3f(-28.0,7.0,-4.0))));}
 case 2u:{return vec4f(ray_water_known_distance(vec3f(-40.0,18.0,-40.0),vec3f(0.0,1.0,0.0),512.0),ray_water_known_distance(vec3f(-40.0,26.0,-40.0),vec3f(0.0,-1.0,0.0),512.0),ray_water_known_distance(vec3f(-12.0,8.0,-4.0),vec3f(1.0,0.0,0.0),512.0),ray_water_known_distance(vec3f(-12.0,8.0,-4.0),vec3f(-1.0,0.0,0.0),512.0));}
 case 4u:{let d=normalize(vec3f(-0.1,0.0,0.995));return vec4f(ray_water_known_distance(vec3f(512.1,8.0,520.0),d,600.0),ray_water_known_distance(vec3f(520.0,8.0,520.0),d,600.0),f32(ray_water_at(vec3f(500.0,8.0,532.0))),ray_water_known_distance(vec3f(0.0,8.0,-4.0),vec3f(-1.0,0.0,0.0),512.0));}
 default:{let d=normalize(vec3f(-1.0,0.0,-1.0));return vec4f(ray_water_known_distance(vec3f(-8.0,8.0,-8.0),d,512.0),ray_water_known_distance(vec3f(-32.0,8.0,-8.0),vec3f(-1.0,0.0,0.0),512.0),f32(ray_water_at(vec3f(-40.0,-4.0,-40.0))),f32(ray_water_at(vec3f(-12.5,7.5,-4.5))));}
 }
}
"#;
fn source() -> String {
    format!(
        "{}\n{}\n{PROBES}",
        include_str!("../../coverage.wgsl"),
        include_str!("../../volume.wgsl")
    )
}
#[test]
fn production_water_volume_fixture_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn gpu_water_authoritative_negative_voxels_coarse_holes_override_and_exact_dda() {
    let scene = records();
    let (device, queue) = device();
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0.0f32, f32::from_bits(scene.water_offset), 0.0, 0.0]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let records = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&scene.coverage),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[0, 10].map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: if binding == 0 {
                    wgpu::BufferBindingType::Uniform
                } else {
                    wgpu::BufferBindingType::Storage { read_only: true }
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }),
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: records.as_entire_binding(),
            },
        ],
    });
    let rows = draw(
        &device,
        &queue,
        &source(),
        5,
        1,
        &[&group],
        &[Some(&layout)],
    );
    assert_eq!(
        rows[0],
        [1.0, 0.0, 0.0, -1.0],
        "actual local X/Z/Y mask and loaded dry precedence"
    );
    assert_eq!(
        rows[1],
        [1.0, -1.0, 0.0, 0.0],
        "coarse vertical holes and near override"
    );
    for (actual, expected) in rows[2].iter().zip([2.0, 2.0, 12.0, 116.0]) {
        assert!(
            (actual - expected).abs() < 0.0002,
            "exact known-prefix boundary: {rows:?}"
        );
    }
    // Negative corner enters covered coarse state, then exits its128m footprint.
    assert!(
        (rows[3][0] - 120.0 * 2.0f32.sqrt()).abs() < 0.001,
        "corner DDA: {rows:?}"
    );
    assert_eq!(rows[3][1], 96.0);
    let expected = 120.0 / glam::Vec3::new(-0.1, 0.0, 0.995).normalize().z;
    assert!(
        (rows[4][0] - expected).abs() < 0.005 && (rows[4][1] - expected).abs() < 0.005,
        "negative shallow boundary must advance despite sub-ULP0.0001*direction at X512: {rows:?}"
    );
    assert_eq!(rows[4][2], 0.0);
    assert_eq!(
        rows[4][3], 128.0,
        "exact zero-plane negative ray must enter known side without subnormal flush: {rows:?}"
    );
    assert_eq!(rows[3][2..], [1.0, 1.0]);
}
