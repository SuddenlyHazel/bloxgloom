//! Generated coast interface geometry versus the actual admitted medium directory.
use super::{device, draw};
use crate::{content, lod, render::trace::scene, world};
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[test]
fn gpu_generated_coast_water_interface_offsets_match_authoritative_medium() {
    const SEED: u64 = 0xB10C6100;
    let catalog = content::catalog();
    let mut near = Vec::new();
    let mut points = Vec::<[f32; 4]>::new();
    let mut known = std::collections::HashMap::new();
    for z in -129..=-126 {
        for x in -42..=-38 {
            for y in 0..=2 {
                let key = world::ChunkKey { x, y, z };
                known.insert(key, Arc::new(world::generate_chunk(key, SEED)));
            }
        }
    }
    for z in -129..=-126 {
        for x in -42..=-38 {
            for y in 0..=2 {
                let key = world::ChunkKey { x, y, z };
                let voxels = &known[&key];
                let chunk = if y == 1 {
                    let light =
                        crate::lighting::LightField::build_with_catalog(key, &known, 1, catalog);
                    let mesh = crate::render::mesh::mesh_chunk_lit_with_neighbors(
                        voxels, &light, 1, catalog, &known,
                    );
                    scene::Chunk::from_world_mesh(&mesh, catalog, voxels)
                } else {
                    scene::Chunk {
                        key: Some(key),
                        water: scene::water::Occupancy::from_chunk(voxels, catalog),
                        ..Default::default()
                    }
                };
                for triangle in &chunk.triangles {
                    if triangle.surface_flags & scene::surface::WATER == 0
                        || triangle.normal[1] < 0.99
                    {
                        continue;
                    }
                    let p = [
                        (triangle.a[0] + triangle.b[0] + triangle.c[0]) / 3.0,
                        triangle.a[1],
                        (triangle.a[2] + triangle.b[2] + triangle.c[2]) / 3.0,
                        0.0,
                    ];
                    assert_eq!(p[1], 17.0, "generated ocean mesh is at the exact voxel top");
                    points.push(p);
                }
                near.push(Arc::new(chunk));
            }
        }
    }
    let near_count = points.len();
    assert!(
        near_count > 8,
        "probe actual generated coastline, not a synthetic pool"
    );
    let key = lod::TileKey {
        level: 2,
        x: -5,
        z: -16,
    };
    let tile = world::lod::builtin_lod_tile(key, 1, SEED, catalog).unwrap();
    let coarse = scene::water::CoarseTile::from_lod(&tile, catalog).unwrap();
    let [minx, minz, _, _] = key.bounds().unwrap();
    let width = key.sample_width().unwrap();
    for (index, column) in coarse.columns.iter().enumerate() {
        for span in &column.water {
            assert_eq!(
                span.top, 17,
                "generated coarse ocean keeps the exact fine ocean top"
            );
            let px = minx + (index % 32) as i32 * width + width / 2;
            let pz = minz + (index / 32) as i32 * width + width / 2;
            if known.contains_key(&world::world_to_chunk(px, span.top, pz).0) {
                continue;
            }
            points.push([
                minx as f32 + (index % 32) as f32 * width as f32 + width as f32 * 0.5,
                span.top as f32,
                minz as f32 + (index / 32) as f32 * width as f32 + width as f32 * 0.5,
                1.0,
            ]);
        }
    }
    let coarse_count = points.len() - near_count;
    assert!(coarse_count > 8);
    let coarse = [Arc::new(scene::Chunk {
        coarse_water: Some(coarse),
        ..Default::default()
    })];
    let mut scene = scene::Scene::build(near.iter().cloned());
    scene::volume::append(&mut scene, &near, &coarse);
    let (device, queue) = device();
    let buffers = [
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0.0f32, f32::from_bits(scene.water_offset), 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM,
        }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&points),
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
    let source = format!(
        "{}\n{}\n{}",
        include_str!("../../coverage.wgsl"),
        include_str!("../../volume.wgsl"),
        PROBE
    );
    let rows = draw(
        &device,
        &queue,
        &source,
        points.len() as u32,
        1,
        &[&group],
        &[Some(&layout)],
    );
    for (point, row) in points.iter().zip(&rows) {
        assert_eq!(
            *row,
            [0.0, 1.0, 0.0, 1.0],
            "actual near/coarse surface reflected origin is dry, transmitted origin is wet, no hidden geometry offset: point={point:?}"
        );
    }
    println!(
        "actual coast: {near_count} near mesh triangle centers, {coarse_count} authoritative coarse wet spans; top17; reflection dry/transmission wet"
    );
}
const PROBE: &str = r#"
struct Frame {water:vec4f};
@group(0) @binding(0) var<uniform> ray_frame:Frame;
@group(0) @binding(1) var<storage,read> points:array<vec4f>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(xy*2.0-1.0,0.0,1.0);
}
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {
 let point=points[u32(p.x)].xyz;
 return vec4f(f32(ray_water_at(point+vec3f(0.0,0.006,0.0))),f32(ray_water_at(point-vec3f(0.0,0.006,0.0))),
  f32(ray_water_at(point+vec3f(0.0,0.0001,0.0))),f32(ray_water_at(point-vec3f(0.0,0.0001,0.0))));
}
"#;
