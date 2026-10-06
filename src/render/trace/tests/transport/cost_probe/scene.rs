//! Same generated near bounds, light meshes and coverage-selected LOD as coast.
use super::*;
use crate::{lod, render::trace::scene, world};
use glam::Vec3;
use std::collections::HashMap;

pub(super) const SEED: u64 = 0xB10C_6100;
pub(super) const RAYS: usize = 32;
pub(super) const SEEDS: usize = 8;
pub(super) struct Coast {
    pub group: wgpu::BindGroup,
    pub near_triangles: usize,
    pub near_nodes: usize,
    pub tiles: usize,
    pub pages: usize,
    pub lod_triangles: usize,
}
fn touches(a: lod::TileKey, b: lod::TileKey) -> bool {
    let [ax, az, bx, bz] = a.bounds().unwrap();
    let [cx, cz, dx, dz] = b.bounds().unwrap();
    ((bx == cx || dx == ax) && az < dz && cz < bz) || ((bz == cz || dz == az) && ax < dx && cx < bx)
}
pub(super) fn build(f: &Fixture, catalog: &Catalog) -> Coast {
    let started = std::time::Instant::now();
    let mut loaded = HashMap::new();
    for z in -134..=-122 {
        for x in -47..=-35 {
            for y in -1..=world::MAX_GENERATED_HEIGHT.div_euclid(16) {
                let key = world::ChunkKey { x, y, z };
                loaded.insert(key, Arc::new(world::generate_chunk(key, SEED)));
            }
        }
    }
    let mut keys: Vec<_> = loaded.keys().copied().collect();
    keys.sort_by_key(|key| (key.z, key.x, key.y));
    let near: Vec<_> = keys
        .iter()
        .map(|key| {
            let light = crate::lighting::LightField::build_with_bounce(*key, &loaded, SEED, false);
            let mesh = render::mesh::mesh_chunk_lit_with_neighbors(
                &loaded[key],
                &light,
                0,
                catalog,
                &loaded,
            );
            // from_world_mesh uses the actual authoritative occupancy, including
            // empty loaded chunks, with no invented procedural neighbor snapshots.
            Arc::new(scene::Chunk::from_world_mesh(&mesh, catalog, &loaded[key]))
        })
        .collect();
    let origin = Vec3::new(-617.5, 37.0, -2015.5);
    let forward = (Vec3::new(-655.5, 19.0, -2047.5) - origin).normalize();
    let camera = render::Camera {
        position: origin,
        yaw: forward.z.atan2(forward.x),
        pitch: forward.y.asin(),
        fov_y_radians: 70.0f32.to_radians(),
    };
    let atmosphere = render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS);
    let mut resident = render::lod::Gpu::new(
        &f.device,
        render::post::HDR_FORMAT,
        &f.pipeline,
        &f.materials,
    );
    resident.set_horizon(512);
    let tiles: Vec<_> = render::lod::desired_tiles(origin, 512, 1, 4)
        .into_iter()
        .map(|key| world::lod::builtin_lod_tile(key, 1, SEED, catalog).unwrap())
        .collect();
    let colors = render::lod::FaceColors::new(catalog);
    for tile in &tiles {
        let neighbors: Vec<_> = tiles
            .iter()
            .filter(|other| other.key != tile.key && touches(tile.key, other.key))
            .collect();
        let mesh = render::lod::mesh(tile, &neighbors, catalog, &colors).unwrap();
        assert!(
            mesh.ray.is_some(),
            "GI=1 must publish actual mesh-derived LOD targets"
        );
        resident.enqueue(mesh).unwrap();
        assert_eq!(resident.upload(&f.device), 1);
    }
    resident.prepare_at(
        &f.queue,
        camera,
        1280,
        800,
        atmosphere,
        keys.into_iter(),
        0.0,
    );
    let distant: Vec<_> = resident
        .ray_targets()
        .into_iter()
        .map(|(_, _, target)| target)
        .collect();
    assert!(!distant.is_empty());
    let pages = scene::pages::build(
        distant.iter().cloned(),
        f.device.limits().max_storage_buffer_binding_size,
    )
    .unwrap();
    let mut geometry = Scene::build(near.iter().cloned());
    scene::volume::append(&mut geometry, &near, &distant);
    assert!(geometry.fits(f.device.limits().max_storage_buffer_binding_size));
    // Sample actual coast pixel directions, including shore, near/far ocean,
    // land/crowns and sky. They are finite secondary queries from the same eye;
    // this fixture intentionally does not fabricate raster MRT/depth inputs.
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward).normalize();
    let mut queries = Vec::<[f32; 8]>::new();
    for y in [130.5, 230.5, 380.5, 600.5] {
        for x in [80.5, 240.5, 400.5, 560.5, 720.5, 880.5, 1040.5, 1200.5] {
            let tan = (camera.fov_y_radians * 0.5).tan();
            let direction = (forward
                + right * ((x / 1280.0 * 2.0 - 1.0) * 1.6 * tan)
                + up * ((1.0 - y / 800.0 * 2.0) * tan))
                .normalize();
            queries.push([
                origin.x,
                origin.y,
                origin.z,
                1.0,
                direction.x,
                direction.y,
                direction.z,
                0.0,
            ]);
        }
    }
    assert_eq!(queries.len(), RAYS);
    let query_offset = geometry.coverage.len();
    assert!(
        query_offset > 25,
        "query offset must not enable controlled fixture modes"
    );
    geometry
        .coverage
        .extend_from_slice(bytemuck::cast_slice(&queries));
    let mut frame = [0.0f32; 76];
    frame[32..36].copy_from_slice(&[origin.x, origin.y, origin.z, atmosphere.wind_seconds]);
    frame[36..40].copy_from_slice(&[
        atmosphere.sun.x,
        atmosphere.sun.y,
        atmosphere.sun.z,
        (atmosphere.sun.y / render::SUN_DIRECTION.normalize().y).clamp(0.0, 1.0),
    ]);
    frame[40..43].copy_from_slice(&atmosphere.sun_radiance().to_array());
    frame[44..47].copy_from_slice(&atmosphere.horizon.to_array());
    frame[48..51].copy_from_slice(&atmosphere.zenith.to_array());
    frame[51] = 1.0;
    frame[52..56].copy_from_slice(&[
        atmosphere.cloud,
        atmosphere.drift[0],
        atmosphere.drift[1],
        1.0,
    ]);
    frame[56..60].copy_from_slice(&[
        atmosphere.rain_strength,
        atmosphere.moon_multiplier(),
        atmosphere.moon_phase as f32,
        atmosphere.presentation_seconds,
    ]);
    frame[60..64].copy_from_slice(&[
        0.0004 + atmosphere.fog * 0.003,
        1.0,
        4.0,
        f32::from(std::env::var("BLOXGLOOM_GI_OPAQUE_PRECHECK").as_deref() == Ok("1")),
    ]);
    frame[68] = f32::from_bits(geometry.nodes.len() as u32);
    frame[69] = f32::from_bits(query_offset as u32);
    frame[73] = f32::from_bits(geometry.water_offset);
    let buffer = |data: &[u8], usage| {
        f.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("real coast fixed-ray attribution"),
                contents: if data.is_empty() { &[0; 96] } else { data },
                usage,
            })
    };
    let buffers = [
        buffer(bytemuck::cast_slice(&frame), wgpu::BufferUsages::UNIFORM),
        buffer(
            bytemuck::cast_slice(&geometry.nodes),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(
            bytemuck::cast_slice(&geometry.triangles),
            wgpu::BufferUsages::STORAGE,
        ),
        buffer(
            bytemuck::cast_slice(&geometry.coverage),
            wgpu::BufferUsages::STORAGE,
        ),
    ];
    // Match the live predeformed near geometry rather than charging repeated
    // per-triangle wind calculations that production has already moved out.
    let original = buffer(
        bytemuck::cast_slice(&geometry.triangles),
        wgpu::BufferUsages::STORAGE,
    );
    let deformation = render::trace::deformation::Deformation::new(&f.device, &f.material_layout);
    let mut encoder = f.device.create_command_encoder(&Default::default());
    deformation.encode(
        &f.device,
        &mut encoder,
        &buffers[0],
        &original,
        &buffers[2],
        &f.materials,
        None,
    );
    f.queue.submit([encoder.finish()]);
    let distant_buffers: [wgpu::Buffer; scene::pages::MAX_PAGES] = std::array::from_fn(|index| {
        let words = pages
            .get(index)
            .map_or_else(|| vec![0; 4], scene::pages::packed_words);
        buffer(bytemuck::cast_slice(&words), wgpu::BufferUsages::STORAGE)
    });
    let mut entries: Vec<_> = [0, 1, 2, 10]
        .into_iter()
        .zip(&buffers)
        .map(|(binding, buffer)| wgpu::BindGroupEntry {
            binding,
            resource: buffer.as_entire_binding(),
        })
        .collect();
    entries.extend(distant_buffers.iter().enumerate().map(|(index, buffer)| {
        wgpu::BindGroupEntry {
            binding: 11 + index as u32,
            resource: buffer.as_entire_binding(),
        }
    }));
    let group = f.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("actual selected coast ray scene"),
        layout: &f.layout,
        entries: &entries,
    });
    println!(
        "coast secondary probe setup={:.3}s; original admitted tiles={} selected={}; frozen clock0, no saved edits/dynamic actors",
        started.elapsed().as_secs_f64(),
        tiles.len(),
        distant.len()
    );
    Coast {
        group,
        near_triangles: geometry.triangles.len(),
        near_nodes: geometry.nodes.len(),
        tiles: distant.len(),
        pages: pages.len(),
        lod_triangles: pages.iter().map(|page| page.triangles.len()).sum(),
    }
}
