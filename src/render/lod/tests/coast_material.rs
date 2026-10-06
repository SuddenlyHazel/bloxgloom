//! Actual selected coast geometry diagnostic; no GPU or shading approximation.
use super::*;
use glam::Vec3;

#[derive(Clone, Copy)]
struct Hit {
    distance: f32,
    key: TileKey,
    vertex: super::super::vertex::Vertex,
    point: Vec3,
    normal: Vec3,
}

fn intersect(origin: Vec3, direction: Vec3, p: [Vec3; 3]) -> Option<f32> {
    let e1 = p[1] - p[0];
    let e2 = p[2] - p[0];
    let cross = direction.cross(e2);
    let determinant = e1.dot(cross);
    if determinant.abs() < 1e-7 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let relative = origin - p[0];
    let u = relative.dot(cross) * inverse;
    let q = relative.cross(e1);
    let v = direction.dot(q) * inverse;
    let t = e2.dot(q) * inverse;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
}

fn describe(hit: Hit, tiles: &std::collections::HashMap<TileKey, LodTile>) -> String {
    let tile = &tiles[&hit.key];
    let [ox, oz, _, _] = hit.key.bounds().unwrap();
    let width = hit.key.sample_width().unwrap();
    let inside = hit.point - hit.normal * 0.002;
    let x = ((inside.x.floor() as i32 - ox) / width).clamp(0, 31);
    let z = ((inside.z.floor() as i32 - oz) / width).clamp(0, 31);
    let span = tile.columns[(x + z * 32) as usize]
        .spans
        .iter()
        .find(|s| (s.bottom as f32..s.top as f32).contains(&inside.y));
    let catalog = crate::content::catalog();
    let state = span
        .and_then(|s| catalog.state(s.state))
        .map(|s| s.key.as_str());
    let appearance = hit.vertex.ray_surface();
    let source = crate::world::coast_column_diagnostic(
        i64::from(ox + x * width + width / 2),
        i64::from(oz + z * width + width / 2),
        0xB10C_6100,
    );
    format!(
        "distance={:.3} point={:?} normal={:?} tile={:?} width={width} state={state:?} color={:?} layer={} textured={} spans={:?}; {source}",
        hit.distance,
        hit.point,
        hit.normal,
        hit.key,
        appearance.color,
        appearance.layer,
        appearance.textured,
        tile.columns[(x + z * 32) as usize]
            .spans
            .iter()
            .filter(|s| s.top >= inside.y as i32 - 3 && s.bottom <= inside.y as i32 + 3)
            .collect::<Vec<_>>(),
    )
}

#[test]
#[ignore = "CPU-only actual coast material/geometry attribution; run --ignored --nocapture"]
fn actual_coast_white_bands_selected_material_probe() {
    let catalog = crate::content::catalog();
    let colors = FaceColors::new(catalog);
    // Actual natural coast capture raster camera (1280x800, FOV70).
    let origin = Vec3::new(-617.5, 37.0, -2015.5);
    let forward = (Vec3::new(-655.5, 19.0, -2047.5) - origin).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward).normalize();
    let mut tiles = std::collections::HashMap::new();
    for key in desired_tiles(origin, 512, 1, 4) {
        tiles.insert(
            key,
            crate::world::lod::builtin_lod_tile(key, 1, 0xB10C_6100, catalog).unwrap(),
        );
    }
    let coverage: std::collections::HashMap<_, _> = tiles
        .iter()
        .map(|(key, tile)| (*key, super::super::coverage::Coverage::from_tile(tile)))
        .collect();
    let selected =
        super::super::gpu::select_ready(tiles.keys().copied().collect(), |parent, children| {
            super::super::coverage::can_refine(
                &coverage[&parent],
                children.map(|key| &coverage[&key]),
            )
        });
    eprintln!(
        "actual coast probe generator={} requested={} selected={} (production can_refine/retirement)",
        crate::world::TERRAIN_GENERATOR_VERSION,
        tiles.len(),
        selected.len()
    );
    let default_pixels = [
        (400, 225),
        (400, 228),
        (440, 223),
        (440, 227),
        (480, 220),
        (480, 225),
        (520, 223),
        (520, 228),
        (550, 228),
        (550, 235),
        (650, 224),
        (320, 218),
        (360, 222),
        (420, 220),
        (420, 231),
        (460, 222),
        (460, 235),
        (500, 223),
        (500, 230),
        (540, 223),
    ];
    let requested_pixels = std::env::var("BLOXGLOOM_COAST_PROBE_PIXELS").ok();
    let custom_pixels = requested_pixels.is_some();
    let pixels = requested_pixels.map_or_else(
        || default_pixels.to_vec(),
        |list| {
            list.split(';')
                .map(|point| {
                    let (x, y) = point
                        .split_once(',')
                        .expect("coast probe pixel must be x,y");
                    (
                        x.parse::<i32>().expect("coast probe x"),
                        y.parse::<i32>().expect("coast probe y"),
                    )
                })
                .collect()
        },
    );
    let directions = pixels
        .iter()
        .map(|&(x, y)| {
            let ndc_x = 2.0 * (x as f32 + 0.5) / 1280.0 - 1.0;
            let ndc_y = 1.0 - 2.0 * (y as f32 + 0.5) / 800.0;
            let tangent = 35.0_f32.to_radians().tan();
            (forward + right * (ndc_x * 1.6 * tangent) + up * (ndc_y * tangent)).normalize()
        })
        .collect::<Vec<_>>();
    let mut hits = vec![[None::<Hit>; 2]; pixels.len()];
    for key in selected {
        let tile = &tiles[&key];
        let [ox, oz, _, _] = key.bounds().unwrap();
        let offset = Vec3::new(ox as f32, 0.0, oz as f32);
        let neighbors = tiles
            .values()
            .filter(|other| {
                let a = key.bounds().unwrap();
                let b = other.key.bounds().unwrap();
                ((a[2] == b[0] || b[2] == a[0]) && a[1] < b[3] && b[1] < a[3])
                    || ((a[3] == b[1] || b[3] == a[1]) && a[0] < b[2] && b[0] < a[2])
            })
            .collect::<Vec<_>>();
        let mesh = mesh(tile, &neighbors, catalog, &colors).unwrap();
        for (class, indices) in [mesh.indices.as_slice(), mesh.water_indices.as_slice()]
            .into_iter()
            .enumerate()
        {
            for indices in indices.chunks_exact(3) {
                let vertices =
                    [indices[0], indices[1], indices[2]].map(|i| mesh.vertices[i as usize]);
                let vertex = vertices[0];
                let decoded = vertex.unpack();
                let normal = Vec3::new(decoded[3], decoded[4], decoded[5]);
                let positions = vertices.map(|v| Vec3::from(v.position) + offset);
                for (ray, direction) in directions.iter().enumerate() {
                    if class == 0 && normal.dot(*direction) >= 0.0 {
                        continue;
                    }
                    let Some(distance) = intersect(origin, *direction, positions) else {
                        continue;
                    };
                    if hits[ray][class].is_some_and(|h| h.distance <= distance) {
                        continue;
                    }
                    let point = origin + *direction * distance;
                    // Same near-ready fragment predicate as bg_lod_coverage;
                    // includes all loaded empty and nonempty landscape chunks.
                    let inside = (point - normal * 0.002).floor().as_ivec3();
                    let k: glam::IVec3 = inside >> 4_u32;
                    if (-47..=-35).contains(&k.x)
                        && (-134..=-122).contains(&k.z)
                        && (-1..=7).contains(&k.y)
                    {
                        continue;
                    }
                    hits[ray][class] = Some(Hit {
                        distance,
                        key,
                        vertex,
                        point,
                        normal,
                    });
                }
            }
        }
    }
    let mut source_chunks = std::collections::HashMap::new();
    for ((pixel, hit), direction) in pixels.into_iter().zip(hits.iter()).zip(directions) {
        eprintln!(
            "PIXEL {pixel:?} exact source: {}",
            exact_bed(origin, direction, catalog, &mut source_chunks)
        );
        eprintln!(
            "PIXEL {pixel:?} opaque: {}",
            hit[0].map_or_else(|| "none".into(), |h| describe(h, &tiles))
        );
        eprintln!(
            "PIXEL {pixel:?} water: {}",
            hit[1].map_or_else(|| "none".into(), |h| describe(h, &tiles))
        );
    }
    assert!(hits.iter().any(|h| h[0].is_some()));
    if !custom_pixels {
        assert!(hits.iter().any(|h| h[1].is_some()));
    }
    // This is material/geometry evidence, not shader-color proof. The probes
    // target far terrain outside near-ready coverage; nearer loaded geometry
    // is not enumerated and may occlude a particular raster sample.
}

fn exact_bed(
    origin: Vec3,
    direction: Vec3,
    catalog: &crate::content::Catalog,
    chunks: &mut std::collections::HashMap<crate::world::ChunkKey, crate::world::Chunk>,
) -> String {
    let mut voxel = origin.floor().as_ivec3();
    let step = direction.signum().as_ivec3();
    let mut next = Vec3::ZERO;
    for axis in 0..3 {
        next[axis] =
            ((voxel[axis] + i32::from(step[axis] > 0)) as f32 - origin[axis]) / direction[axis];
    }
    let delta = direction.recip().abs();
    let mut travel = 0.0;
    let mut normal = Vec3::Y;
    let mut first_water = None;
    for _ in 0..16384 {
        let (key, local) = crate::world::world_to_chunk(voxel.x, voxel.y, voxel.z);
        let id = chunks
            .entry(key)
            .or_insert_with(|| crate::world::generate_chunk(key, 0xB10C_6100))
            .block(local)
            .unwrap();
        let flags = catalog.block_flags(id);
        if flags & crate::content::FLUID != 0 && first_water.is_none() {
            first_water = Some((travel, voxel));
        }
        if flags & crate::content::OPAQUE != 0 {
            let near = (-47..=-35).contains(&key.x)
                && (-134..=-122).contains(&key.z)
                && (-1..=7).contains(&key.y);
            let appearance = catalog.state(id).map(|s| s.key.as_str());
            let mut vertical_sky = 15u8;
            for y in ((voxel.y + 1)..=crate::world::MAX_GENERATED_HEIGHT + 1).rev() {
                let (sky_key, sky_local) = crate::world::world_to_chunk(voxel.x, y, voxel.z);
                let sky_id = chunks
                    .entry(sky_key)
                    .or_insert_with(|| crate::world::generate_chunk(sky_key, 0xB10C_6100))
                    .block(sky_local)
                    .unwrap();
                if catalog.block_flags(sky_id) & crate::content::OPAQUE != 0 {
                    vertical_sky = 0;
                    break;
                }
                vertical_sky = vertical_sky.saturating_sub(catalog.sky_attenuation(sky_id));
            }
            return format!(
                "distance={travel:.3} voxel={voxel:?} normal={normal:?} near={near} material={appearance:?} vertical_sky={vertical_sky} first_water={first_water:?}; {}",
                crate::world::coast_column_diagnostic(
                    i64::from(voxel.x),
                    i64::from(voxel.z),
                    0xB10C_6100
                )
            );
        }
        let axis = if next.x < next.y && next.x < next.z {
            0
        } else if next.y < next.z {
            1
        } else {
            2
        };
        travel = next[axis];
        next[axis] += delta[axis];
        voxel[axis] += step[axis];
        normal = Vec3::ZERO;
        normal[axis] = -step[axis] as f32;
        if travel > 4096.0 {
            break;
        }
    }
    "no opaque source hit".into()
}
