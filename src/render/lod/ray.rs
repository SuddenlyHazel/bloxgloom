//! Ray targets are extracted from admitted distant geometry, never generated here.
//! Near-ready suppression is evaluated by the shared ray intersection path.
use super::{Mesh, vertex::Vertex};
use crate::render::trace::scene::{Chunk, Triangle, surface};
use std::sync::Arc;

pub(super) struct Appearance {
    pub packed: u32,
    pub layer: u32,
    pub color: [f32; 4],
    pub textured: bool,
    pub reconstructed: bool,
}

pub(super) fn requested() -> bool {
    std::env::var("BLOXGLOOM_GI").is_ok_and(|v| v == "1")
        && !crate::render::bsl_reference::enabled()
}

pub(super) fn extract_tile(
    mesh: &Mesh,
    tile: &crate::lod::LodTile,
    catalog: &crate::content::Catalog,
) -> Result<Arc<Chunk>, String> {
    if mesh.key != tile.key || mesh.revision != tile.revision {
        return Err("distant ray payload does not match admitted tile revision".into());
    }
    let mut ray = extract(mesh)?;
    let water = crate::render::trace::scene::water::CoarseTile::from_lod(tile, catalog)
        .ok_or("invalid distant medium coverage")?;
    // Newly extracted and not yet published: no payload clone or worker alias.
    Arc::get_mut(&mut ray)
        .expect("new distant ray payload is unique")
        .coarse_water = Some(water);
    Ok(ray)
}

pub(super) fn extract(mesh: &Mesh) -> Result<Arc<Chunk>, String> {
    // Like Gpu::new, this is a session setting. Read once per worker payload,
    // never per triangle or submitted frame.
    extract_with_textures(
        mesh,
        std::env::var("BLOXGLOOM_LOD_TEXTURES").as_deref() != Ok("0"),
    )
}

fn extract_with_textures(mesh: &Mesh, textures: bool) -> Result<Arc<Chunk>, String> {
    // At most 8 MiB of raster indices can encode 64 MiB of 96-byte
    // triangles. Vertex bytes make the actual maximum smaller. Keep this
    // independent of the adapter's aggregate/page limit and reject overflow
    // before allocating; the unchanged raster budget remains authoritative.
    let indices = mesh
        .indices
        .len()
        .checked_add(mesh.water_indices.len())
        .ok_or("distant ray index count overflow")?;
    if !mesh.indices.len().is_multiple_of(3) || !mesh.water_indices.len().is_multiple_of(3) {
        return Err("distant ray indices are not complete triangles".into());
    }
    let count = indices / 3;
    let bytes = count
        .checked_mul(std::mem::size_of::<Triangle>())
        .ok_or("distant ray triangle byte overflow")?;
    if bytes > 64 * 1024 * 1024 {
        return Err("distant ray payload exceeds raster-derived 64 MiB bound".into());
    }
    let [x, z, _, _] = mesh.key.bounds().ok_or("invalid distant ray tile bounds")?;
    let origin = [x as f32, 0.0, z as f32];
    let mut triangles = Vec::new();
    triangles
        .try_reserve_exact(count)
        .map_err(|_| "distant ray payload allocation failed")?;
    for indices in [&mesh.indices, &mesh.water_indices] {
        for face in indices.chunks_exact(3) {
            let vertex = |i: u32| {
                mesh.vertices
                    .get(i as usize)
                    .copied()
                    .ok_or("distant ray triangle references a missing vertex")
            };
            let vertices = [vertex(face[0])?, vertex(face[1])?, vertex(face[2])?];
            let first = vertices[0];
            let material = first.ray_surface();
            let axis = (material.packed & 7) as usize / 2;
            let mut normal = [0.0; 4];
            normal[axis] = if material.packed & 1 == 0 { -1.0 } else { 1.0 };
            // Current distant proxy vertex shader is static, even for leaves.
            normal[3] = -1.0;
            let positions =
                vertices.map(|v| std::array::from_fn::<_, 3, _>(|a| v.position[a] + origin[a]));
            let points = positions.map(glam::Vec3::from);
            let mut geometric = (points[1] - points[0])
                .cross(points[2] - points[0])
                .normalize_or_zero();
            if material.reconstructed && geometric != glam::Vec3::ZERO {
                if geometric.dot(glam::Vec3::from_array(normal[..3].try_into().unwrap())) < 0.0 {
                    geometric = -geometric;
                }
                normal[..3].copy_from_slice(&geometric.to_array());
            }
            let uv = vertices.map(|v| uv(v, axis));
            let mut flags = surface::LOD
                | surface::COARSE_COLOR
                | surface::NO_WIND
                | (((material.packed >> 7) & 15) << surface::GLOW_SHIFT);
            if material.packed & (1 << 11) != 0 {
                flags |= surface::WATER;
            }
            let has_layer =
                material.packed & (1 << 11) == 0 && (material.packed >> 13) & 0x3ffff != 0;
            let cutout = material.packed & (1 << 12) != 0;
            if has_layer && (cutout || material.textured && textures) {
                flags |= surface::TEXTURE;
            }
            let triangle = Triangle {
                a: [
                    positions[0][0],
                    positions[0][1],
                    positions[0][2],
                    material.layer as f32,
                ],
                b: [
                    positions[1][0],
                    positions[1][1],
                    positions[1][2],
                    ((material.packed >> 3) & 15) as f32 / 15.0,
                ],
                c: [
                    positions[2][0],
                    positions[2][1],
                    positions[2][2],
                    if material.packed & (1 << 12) != 0 {
                        1.0
                    } else {
                        0.0
                    },
                ],
                uv_ab: [uv[0][0], uv[0][1], uv[1][0], uv[1][1]],
                uv_c: uv[2],
                surface_color: 0,
                surface_flags: 0,
                normal,
            }
            .with_surface(material.color, flags);
            triangles.push(triangle);
        }
    }
    Ok(Arc::new(Chunk {
        water: None,
        coarse_water: None,

        key: None,
        triangles,
    }))
}

fn uv(vertex: Vertex, axis: usize) -> [f32; 2] {
    let p = vertex.position;
    match axis {
        0 => [p[2], -p[1]],
        1 => [p[2], p[0]],
        _ => [p[0], -p[1]],
    }
}

#[cfg(test)]
mod tests;
