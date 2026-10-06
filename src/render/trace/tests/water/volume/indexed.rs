//! Independent pre-optimization shader versus packed candidates and known-only DDA.
use super::*;
#[path = "indexed/timing.rs"]
mod timing;

const PROBE: &str = r#"
struct Frame {water:vec4f};
@group(0) @binding(0) var<uniform> ray_frame:Frame;
struct Query {point:vec4f,direction:vec4f};
@group(0) @binding(1) var<storage,read> queries:array<Query>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(xy*2.0-1.0,0.0,1.0);
}
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let q=queries[u32(pixel.x)+u32(pixel.y)*256u];
 let a=ray_water_region(q.point.xyz);let b=oracle_region(q.point.xyz);
 let x=ray_water_known_distance(q.point.xyz,q.direction.xyz,q.point.w);
 let y=oracle_known_distance(q.point.xyz,q.direction.xyz,q.point.w);
 return vec4f(select(0.0,1.0,a.state==b.state),select(0.0,1.0,all(a.low==b.low)),
   select(0.0,1.0,all(a.high==b.high)),select(0.0,1.0,bitcast<u32>(x)==bitcast<u32>(y)));
}
@fragment fn fs_old(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let q=queries[u32(pixel.x)+u32(pixel.y)*256u];return vec4f(oracle_known_distance(q.point.xyz,q.direction.xyz,q.point.w),0.0,0.0,1.0);
}
@fragment fn fs_new(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let q=queries[u32(pixel.x)+u32(pixel.y)*256u];return vec4f(ray_water_known_distance(q.point.xyz,q.direction.xyz,q.point.w),0.0,0.0,1.0);
}
"#;
fn source() -> String {
    let old = include_str!("reference.wgsl")
        .replace("RayKnownRegion", "OracleRegion")
        .replace("ray_volume_word", "oracle_word")
        .replace("ray_water_known_distance", "oracle_known_distance")
        .replace("ray_volume_entered_point", "oracle_entered_point")
        .replace("ray_water_region", "oracle_region")
        .replace("ray_water_at", "oracle_at");
    format!(
        "{}\n{}\n{old}\n{PROBE}",
        include_str!("../../../coverage.wgsl"),
        include_str!("../../../volume.wgsl").replace(
            "const RAY_KNOWN_CERTIFICATE:bool=false;",
            "const RAY_KNOWN_CERTIFICATE:bool=true;"
        )
    )
}
fn bindings(
    device: &wgpu::Device,
    scene: &scene::Scene,
    queries: &[[f32; 8]],
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let buffers = [
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0.0f32, f32::from_bits(scene.water_offset), 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(queries),
            usage: wgpu::BufferUsages::STORAGE,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&scene.coverage),
            usage: wgpu::BufferUsages::STORAGE,
        }),
    ];
    let bindings = [0, 1, 10];
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &bindings.map(|binding| wgpu::BindGroupLayoutEntry {
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
    let entries = bindings
        .into_iter()
        .zip(&buffers)
        .map(|(binding, buffer)| wgpu::BindGroupEntry {
            binding,
            resource: buffer.as_entire_binding(),
        })
        .collect::<Vec<_>>();
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &entries,
    });
    (layout, group)
}
fn compare(scene: &scene::Scene, mut queries: Vec<[f32; 8]>) {
    let count = queries.len();
    queries.resize(count.div_ceil(256) * 256, queries[0]);
    let (device, queue) = device();
    let (layout, group) = bindings(&device, scene, &queries);
    let rows = draw(
        &device,
        &queue,
        &source(),
        256,
        (queries.len() / 256) as u32,
        &[&group],
        &[Some(&layout)],
    );
    for (index, row) in rows[..count].iter().enumerate() {
        assert_eq!(
            *row, [1.0; 4],
            "state/bounds/bitexact known prefix query{index}: {:?}",
            queries[index]
        );
    }
    println!(
        "indexed medium: {count} independent old/new state, low/high and bitexact boundary queries passed"
    );
}
fn fixture(shift: [i32; 2]) -> scene::Scene {
    let catalog = crate::content::catalog();
    let key = world::ChunkKey {
        x: -1 + shift[0] / 16,
        y: 0,
        z: -1 + shift[1] / 16,
    };
    let mut voxels = world::Chunk::from_blocks(key, 1, vec![world::AIR; world::CHUNK_VOLUME]);
    voxels
        .blocks
        .set(world::Chunk::index([3, 7, 11]).unwrap(), world::WATER);
    let near = [
        Arc::new(scene::Chunk {
            key: Some(key),
            water: scene::water::Occupancy::from_chunk(&voxels, catalog),
            ..Default::default()
        }),
        Arc::new(scene::Chunk {
            key: Some(world::ChunkKey {
                x: key.x - 1,
                ..key
            }),
            ..Default::default()
        }),
        // A known fine interval continues above the entire coarse directory;
        // enlarged certificates must not stop before or skip its real end.
        Arc::new(scene::Chunk {
            key: Some(world::ChunkKey { y: 2, ..key }),
            ..Default::default()
        }),
    ];
    let mut coarse = Vec::new();
    for variant in 0..3 {
        let (coverage, water) = match variant {
            0 => (
                vec![
                    lod::Interval {
                        bottom: -8,
                        top: 20,
                    },
                    lod::Interval {
                        bottom: 24,
                        top: 32,
                    },
                ],
                vec![lod::Interval {
                    bottom: -8,
                    top: 17,
                }],
            ),
            1 => (
                vec![lod::Interval {
                    bottom: 20,
                    top: 24,
                }],
                vec![lod::Interval {
                    bottom: 20,
                    top: 22,
                }],
            ),
            _ => (
                vec![lod::Interval { bottom: 0, top: 30 }],
                vec![lod::Interval { bottom: 0, top: 30 }],
            ),
        };
        coarse.push(Arc::new(scene::Chunk {
            coarse_water: Some(scene::water::CoarseTile {
                key: lod::TileKey {
                    level: 2,
                    x: -1 + shift[0] / 128,
                    z: -1 + shift[1] / 128,
                },
                columns: vec![scene::water::CoarseColumn { coverage, water }; lod::TILE_COLUMNS],
            }),
            ..Default::default()
        }));
    }
    let mut scene = scene::Scene::build(near.iter().cloned());
    scene::volume::append(&mut scene, &near, &coarse);
    scene
}
#[test]
fn gpu_indexed_medium_matches_original_fine_coarse_overlap_gaps_and_distant_boundaries() {
    for shift in [[0, 0], [-32768, 65536], [32768, -65536]] {
        let scene = fixture(shift);
        let mut queries = Vec::new();
        let add = |queries: &mut Vec<_>, p: [f32; 3], d: [f32; 3], limit: f32| {
            let direction = glam::Vec3::from_array(d).normalize();
            queries.push([
                p[0] + shift[0] as f32,
                p[1],
                p[2] + shift[1] as f32,
                limit,
                direction.x,
                direction.y,
                direction.z,
                0.0,
            ]);
        };
        // Every fine voxel, including the one wet bit and loaded dry override.
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    add(
                        &mut queries,
                        [x as f32 - 15.5, y as f32 + 0.5, z as f32 - 15.5],
                        [-0.1, 0.0, 0.995],
                        64.0,
                    );
                }
            }
        }
        // Every selected column and both sides/exact values of interval boundaries.
        for z in 0..32 {
            for x in 0..32 {
                for y in [
                    -8.001, -8.0, -7.999, 16.999, 17.0, 19.999, 20.0, 21.999, 22.0, 23.999, 24.0,
                    31.999, 32.0, 32.001, 40.0, 47.999, 48.0,
                ] {
                    let d = if (x + z) % 2 == 0 {
                        [0.0, 1.0, 0.0]
                    } else {
                        [-1.0, -0.01, 1.0]
                    };
                    add(
                        &mut queries,
                        [x as f32 * 4.0 - 126.0, y, z as f32 * 4.0 - 126.0],
                        d,
                        if (x + z) % 3 == 0 { 1024.0 } else { 512.0 },
                    );
                }
            }
        }
        for x in [
            -128.001, -128.0, -127.999, -32.001, -32.0, -31.999, -16.0, 0.0, 0.001,
        ] {
            for z in [-128.0, -16.0, 0.0] {
                for d in [
                    [-1.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [-1.0, 0.0, -1.0],
                    [0.0, -1.0, 0.0],
                ] {
                    add(&mut queries, [x, 8.0, z], d, 512.0);
                }
            }
        }
        compare(&scene, queries);
    }
    // Preserve unindexed legacy records and the exact zero/shallow DDA controls.
    let mut legacy = records();
    let directory = legacy.coverage[legacy.water_offset as usize + 3] as usize;
    legacy.coverage[directory + 7] = 0;
    compare(
        &legacy,
        vec![
            [512.1, 8.0, 520.0, 600.0, -0.1, 0.0, 0.995, 0.0],
            [0.0, 8.0, -4.0, 512.0, -1.0, 0.0, 0.0, 0.0],
        ],
    );
}

#[test]
fn gpu_indexed_medium_matches_original_generated_coast_and_selected_tile_seams() {
    let catalog = crate::content::catalog();
    let seed = 0xB10C6100;
    let mut near = Vec::new();
    for z in -129..=-126 {
        for x in -42..=-38 {
            for y in 0..=2 {
                let key = world::ChunkKey { x, y, z };
                let voxels = world::generate_chunk(key, seed);
                near.push(Arc::new(scene::Chunk {
                    key: Some(key),
                    water: scene::water::Occupancy::from_chunk(&voxels, catalog),
                    ..Default::default()
                }));
            }
        }
    }
    let mut coarse = Vec::new();
    let mut queries = Vec::new();
    for z in -17..=-16 {
        for x in -6..=-4 {
            let key = lod::TileKey { level: 2, x, z };
            let tile = world::lod::builtin_lod_tile(key, 1, seed, catalog).unwrap();
            let [minx, minz, maxx, maxz] = key.bounds().unwrap();
            let width = key.sample_width().unwrap() as f32;
            for index in 0..1024 {
                for y in [0.0, 16.0, 16.999, 17.0, 17.001, 24.0, 32.0] {
                    let px = minx as f32 + (index % 32) as f32 * width + width * 0.5;
                    let pz = minz as f32 + (index / 32) as f32 * width + width * 0.5;
                    queries.push([px, y, pz, 128.0, 0.0, -1.0, 0.0, 0.0]);
                }
            }
            for px in [
                minx as f32 - 0.001,
                minx as f32,
                minx as f32 + 0.001,
                maxx as f32 - 0.001,
                maxx as f32,
                maxx as f32 + 0.001,
            ] {
                for pz in [minz as f32, maxz as f32, (minz + 16) as f32] {
                    for d in [-1.0, 1.0] {
                        for limit in [16.0, 64.0, 512.0, 1024.0] {
                            queries.push([px, 16.999, pz, limit, d, 0.0, 0.0, 0.0]);
                            let direction = glam::Vec3::new(d, 0.0001, -d * 0.995).normalize();
                            queries.push([
                                px,
                                16.999,
                                pz,
                                limit,
                                direction.x,
                                direction.y,
                                direction.z,
                                0.0,
                            ]);
                        }
                    }
                }
            }
            coarse.push(Arc::new(scene::Chunk {
                coarse_water: scene::water::CoarseTile::from_lod(&tile, catalog),
                ..Default::default()
            }));
        }
    }
    let mut scene = scene::Scene::build(near.iter().cloned());
    scene::volume::append(&mut scene, &near, &coarse);
    compare(&scene, queries);
}
