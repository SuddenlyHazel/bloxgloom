//! Independent coverage-first reference versus bounded first-interface queries.
use super::*;
#[path = "guide_query/timing.rs"]
mod timing;
use crate::render::trace::{
    dynamic::{DynamicAsset, DynamicInstance, DynamicTargets, Material, Vertex},
    scene::{self, surface},
};
use crate::{lod, world};

const PROBE: &str = r#"
struct GuideQuery {origin:vec4f,axis:vec4f};
fn query_at(index:u32)->GuideQuery {
 let at=ray_frame.counts.y+index*8u;
 return GuideQuery(bitcast<vec4f>(vec4u(ray_volume_word(at),ray_volume_word(at+1u),ray_volume_word(at+2u),ray_volume_word(at+3u))),
  bitcast<vec4f>(vec4u(ray_volume_word(at+4u),ray_volume_word(at+5u),ray_volume_word(at+6u),ray_volume_word(at+7u))));
}
fn oracle_hit(origin:vec3f,axis:vec3f)->RayHit {
 let prefix=oracle_known_distance(origin,axis,512.0);
 return ray_cast(origin,axis,prefix);
}
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let q=query_at(u32(pixel.x)+u32(pixel.y)*256u);
 ray_dynamic_disabled=q.origin.w>0.5;
 ray_rng=1799u;ray_dynamic_touched=false;
 let a=oracle_hit(q.origin.xyz,q.axis.xyz);let old_hit_touched=ray_dynamic_touched;
 let old_hit_rng=ray_rng;
 ray_rng=1799u;ray_dynamic_touched=false;
 let b=ray_water_guide_hit(q.origin.xyz,q.axis.xyz);let new_hit_touched=ray_dynamic_touched;
 let new_hit_rng=ray_rng;
 ray_rng=1799u;ray_dynamic_touched=false;
 let old=oracle_sun_guide(q.origin.xyz,q.axis.xyz);let old_touched=ray_dynamic_touched;
 let old_rng=ray_rng;
 ray_rng=1799u;ray_dynamic_touched=false;
 let proposed=ray_water_sun_guide(q.origin.xyz,q.axis.xyz);let new_touched=ray_dynamic_touched;
 let new_rng=ray_rng;
 let hits=a.distance==b.distance&&a.triangle==b.triangle&&all(a.uv==b.uv)&&all(a.normal==b.normal);
 let guides=all(old.axis==proposed.axis)&&old.cosine==proposed.cosine&&old.enabled==proposed.enabled;
 return vec4f(select(0.0,1.0,hits),select(0.0,1.0,guides),
  select(0.0,1.0,old_touched==new_touched&&old_hit_touched==new_hit_touched),
  select(-1.0,1.0+select(0.0,1.0,old.enabled)+select(0.0,2.0,old_hit_touched||old_touched)
    +select(0.0,4.0,ray_water_is(a)),old_rng==new_rng&&old_hit_rng==new_hit_rng&&new_rng==1799u));
}
"#;
const TIMING: &str = r#"
fn timing_query(pixel:vec4f,old:bool)->vec4f {
 let q=query_at(u32(pixel.x));ray_dynamic_disabled=q.origin.w>0.5;
 var guide=RayWaterGuide(q.axis.xyz,1.0,false);
 if old {guide=oracle_sun_guide(q.origin.xyz,q.axis.xyz);}
 else {guide=ray_water_sun_guide(q.origin.xyz,q.axis.xyz);}
 return vec4f(guide.axis,select(0.0,guide.cosine,guide.enabled));
}
@fragment fn fs_old(@builtin(position) pixel:vec4f)->@location(0) vec4f {return timing_query(pixel,true);}
@fragment fn fs_new(@builtin(position) pixel:vec4f)->@location(0) vec4f {return timing_query(pixel,false);}
"#;
fn shader() -> String {
    let old_volume = include_str!("../water/volume/reference.wgsl")
        .replace("RayKnownRegion", "OracleRegion")
        .replace("ray_volume_word", "oracle_volume_word")
        .replace("ray_water_known_distance", "oracle_known_distance")
        .replace("ray_volume_entered_point", "oracle_entered_point")
        .replace("ray_water_region", "oracle_volume_region")
        .replace("ray_water_at", "oracle_volume_at");
    source(12)
        .replace(
            "const RAY_KNOWN_CERTIFICATE:bool=false;",
            "const RAY_KNOWN_CERTIFICATE:bool=true;",
        )
        .replace(
            FIXTURE,
            &format!(
                "{old_volume}\n{}\n{PROBE}",
                include_str!("guide_query/reference.wgsl")
            ),
        )
}
fn group(f: &Fixture, scene: &Scene, time: f32, queries: &[[f32; 8]]) -> wgpu::BindGroup {
    let mut frame = [0.0f32; 76];
    frame[36..40].copy_from_slice(&[0.6, 0.8, 0.0, 1.0]);
    frame[40..43].fill(1.0);
    frame[68] = f32::from_bits(scene.nodes.len() as u32);
    frame[69] = f32::from_bits(scene.coverage.len() as u32);
    let mut coverage = scene.coverage.clone();
    coverage.extend_from_slice(bytemuck::cast_slice(queries));
    frame[72] = time;
    frame[73] = f32::from_bits(scene.water_offset);
    let buffer = |data: &[u8], usage| {
        f.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("guide first-interface fixture"),
                contents: if data.is_empty() { &[0u8; 96] } else { data },
                usage,
            })
    };
    let buffers = [
        buffer(bytemuck::cast_slice(&frame), wgpu::BufferUsages::UNIFORM),
        buffer(
            bytemuck::cast_slice(&scene.nodes),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(
            bytemuck::cast_slice(&scene.triangles),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(bytemuck::cast_slice(&coverage), wgpu::BufferUsages::STORAGE),
    ];
    let mut entries = buffers
        .iter()
        .enumerate()
        .map(|(i, b)| wgpu::BindGroupEntry {
            binding: if i == 3 { 10 } else { i as u32 },
            resource: b.as_entire_binding(),
        })
        .collect::<Vec<_>>();
    f.append_empty_pages(&mut entries);
    f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &f.layout,
        entries: &entries,
    })
}

fn pool(catalog: &Catalog, shift: i32, variant: usize) -> Scene {
    let key = world::ChunkKey {
        x: shift / 16,
        y: 0,
        z: shift / 16,
    };
    let mut voxels = world::Chunk::from_blocks(key, 1, vec![world::AIR; world::CHUNK_VOLUME]);
    let top = if variant == 2 { 16 } else { 8 };
    for z in 0..16 {
        for x in 0..16 {
            for y in 0..top {
                voxels
                    .blocks
                    .set(world::Chunk::index([x, y, z]).unwrap(), world::WATER);
            }
        }
    }
    let known = std::collections::HashMap::from([(key, Arc::new(voxels.clone()))]);
    let light = crate::lighting::LightField::build_with_catalog(key, &known, 1, catalog);
    let mesh = render::mesh::mesh_chunk_lit_with_neighbors(&voxels, &light, 1, catalog, &known);
    let mut published = Chunk::from_world_mesh(&mesh, catalog, &voxels);
    for t in &mut published.triangles {
        t.b[3] = 1.0;
    }
    if variant == 1 {
        published.triangles.extend(quad(
            [
                [shift as f32, 4.0, shift as f32],
                [shift as f32 + 16.0, 4.0, shift as f32],
                [shift as f32 + 16.0, 4.0, shift as f32 + 16.0],
                [shift as f32, 4.0, shift as f32 + 16.0],
            ],
            0,
            [0.5; 2],
            false,
        ));
    }
    let near = [Arc::new(published)];
    let mut scene = Scene::build(near.iter().cloned());
    scene::volume::append(&mut scene, &near, &[]);
    scene
}
fn coarse(_catalog: &Catalog, shift: i32, gap: bool) -> Scene {
    let s = shift as f32;
    let triangles = quad(
        [
            [s, 8.0, s],
            [s, 8.0, s + 64.0],
            [s + 64.0, 8.0, s + 64.0],
            [s + 64.0, 8.0, s],
        ],
        0,
        [0.5; 2],
        false,
    )
    .into_iter()
    .map(|mut t| {
        t.b[3] = 1.0;
        t.with_surface([1.0; 4], surface::WATER | surface::NO_WIND)
    })
    .collect();
    let near = [Arc::new(Chunk {
        triangles,
        ..Default::default()
    })];
    let tiles = (0..77)
        .map(|x| {
            Arc::new(Chunk {
                coarse_water: Some(scene::water::CoarseTile {
                    key: lod::TileKey {
                        level: 1,
                        x: shift / 64 + x,
                        z: shift / 64,
                    },
                    columns: vec![
                        scene::water::CoarseColumn {
                            coverage: vec![lod::Interval {
                                bottom: 0,
                                top: if gap { 8 } else { 512 }
                            }],
                            water: vec![lod::Interval { bottom: 0, top: 8 }]
                        };
                        lod::TILE_COLUMNS
                    ],
                }),
                ..Default::default()
            })
        })
        .collect::<Vec<_>>();
    let mut scene = Scene::build(near);
    scene::volume::append(&mut scene, &[], &tiles);
    scene
}
fn actor(shift: i32) -> DynamicInstance {
    let s = shift as f32;
    let vertices = [
        [s + 6.0, 4.0, s + 6.0],
        [s + 10.0, 4.0, s + 6.0],
        [s + 10.0, 4.0, s + 10.0],
        [s + 6.0, 4.0, s + 10.0],
    ]
    .into_iter()
    .map(|position| Vertex {
        position,
        normal: [0.0, 1.0, 0.0],
        uv: [0.5; 2],
        joints: [0; 4],
        weights: [0.0; 4],
        part: 0,
    })
    .collect();
    let mut material = Material::flat([1.0; 3]);
    material.double_sided = true;
    DynamicInstance::rigid(
        DynamicAsset::build(
            vertices,
            vec![([0, 1, 2], 0, 0), ([0, 2, 3], 0, 0)],
            vec![material],
            Vec::new(),
        ),
        glam::Mat4::IDENTITY,
        1.0,
    )
}
#[test]
fn first_interface_query_shader_validates() {
    let s = shader();
    let module = wgpu::naga::front::wgsl::parse_str(&s)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&s)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn gpu_first_interface_guide_preserves_hits_unknown_seams_dynamic_dependencies_and_rng() {
    let catalog = Catalog::builtins();
    let mut f = Fixture::new(&catalog);
    let mut total = 0;
    let mut enabled = 0;
    let mut touched = 0;
    let mut water_hits = 0;
    for shift in [0, -32000, 64000] {
        let targets = DynamicTargets {
            instances: vec![actor(shift)],
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !f.dynamic.set(&f.device, &f.queue, &targets) {
            assert!(
                std::time::Instant::now() < deadline,
                "fixture actor admission timed out"
            );
            std::thread::yield_now();
        }
        let mut encoder = f.device.create_command_encoder(&Default::default());
        f.dynamic.encode(&mut encoder);
        f.queue.submit([encoder.finish()]);
        let mut queries = Vec::new();
        for i in 0..1024 {
            let x = shift as f32 + 0.5 + (i % 32) as f32 * 0.5;
            let z = shift as f32 + 0.5 + ((i / 32) % 32) as f32 * 0.5;
            let direction = match i % 6 {
                0 => glam::Vec3::Y,
                1 => glam::Vec3::new(1.0, 0.01, 0.0).normalize(),
                2 => glam::Vec3::X,
                3 => -glam::Vec3::X,
                4 => glam::Vec3::new(-0.6, 0.8, 0.0),
                _ => -glam::Vec3::Y,
            };
            for disabled in [0.0, 1.0] {
                queries.push([
                    x,
                    1.5,
                    z,
                    disabled,
                    direction.x,
                    direction.y,
                    direction.z,
                    0.0,
                ]);
            }
        }
        // Exact/neighboring unknown-at-interface seams and negative crossings.
        for x in [shift as f32, shift as f32 + 16.0] {
            for y in [0.0, 7.9999995, 8.0, 8.000001, 15.999999, 16.0] {
                queries.push([x, y, shift as f32 + 8.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
            }
        }
        let count = queries.len();
        queries.resize(count.div_ceil(256) * 256, queries[0]);
        for (variant, scene) in [
            pool(&catalog, shift, 0),
            pool(&catalog, shift, 1),
            pool(&catalog, shift, 2),
            coarse(&catalog, shift, false),
            coarse(&catalog, shift, true),
        ]
        .iter()
        .enumerate()
        {
            for time in [0.0, 19.75] {
                let inputs = group(&f, scene, time, &queries);
                let pixels = super::super::denoise::draw(
                    &f.device,
                    &f.queue,
                    &shader(),
                    256,
                    (queries.len() / 256) as u32,
                    &[&inputs, &f.materials, &f.dynamic.group],
                    &[
                        Some(&f.layout),
                        Some(&f.material_layout),
                        Some(&f.dynamic.layout),
                    ],
                );
                for (i, row) in pixels[..count].iter().enumerate() {
                    assert_eq!(
                        &row[..3],
                        &[1.0; 3],
                        "shift={shift} variant={variant} time={time} query{i} {:?}: {row:?}",
                        queries[i]
                    );
                    assert!(
                        row[3] >= 1.0 && row[3] <= 8.0,
                        "guide queries must leave RNG unchanged: {row:?}"
                    );
                    let mask = row[3] as u32 - 1;
                    enabled += usize::from(mask & 1 != 0);
                    touched += usize::from(mask & 2 != 0);
                    water_hits += usize::from(mask & 4 != 0);
                    total += 1;
                }
            }
        }
    }
    assert!(
        enabled > 0 && touched > 0 && water_hits > 0,
        "fixture must exercise real guides, water hits, and current actors"
    );
    println!(
        "first-interface guide: {total} bitexact hit/guide/dependency/RNG queries, enabled={enabled} touched={touched} water_hits={water_hits}"
    );
}
