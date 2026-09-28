//! Small textured block meshes for nearby dropped items.
use super::{VERTEX_FLOATS, material::face_uv};
use crate::content::{self, Catalog};
use crate::items::ItemId;
use crate::lighting::LightSample;
use glam::Vec3;

pub(super) const MAX_ITEMS: usize = 512;
pub(super) const MAX_VERTEX_BYTES: u64 = (MAX_ITEMS * 24 * VERTEX_FLOATS * 4) as u64;
pub(super) const MAX_INDEX_BYTES: u64 = (MAX_ITEMS * 36 * 4) as u64;
pub(super) const MAX_CUTOUT_VERTEX_BYTES: u64 = (MAX_ITEMS * 8 * VERTEX_FLOATS * 4) as u64;
pub(super) const MAX_CUTOUT_INDEX_BYTES: u64 = (MAX_ITEMS * 12 * 4) as u64;

pub(crate) struct DropMeshes {
    pub opaque_vertices: Vec<f32>,
    pub opaque_indices: Vec<u32>,
    pub cutout_vertices: Vec<f32>,
    pub cutout_indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualDrop {
    pub item: ItemId,
    pub center: Vec3,
    pub angle: f32,
    pub scale: f32,
    pub light: LightSample,
}

impl VisualDrop {
    pub(crate) fn presentation_scale(&self, catalog: &Catalog) -> f32 {
        self.scale * catalog.drop_size(self.item).multiplier()
    }

    fn light_attributes(&self) -> [f32; 3] {
        let bounce = u32::from(self.light.bounce[0])
            | (u32::from(self.light.bounce[1]) << 8)
            | (u32::from(self.light.bounce[2]) << 16);
        [
            f32::from(self.light.sky) / 15.0,
            f32::from(self.light.glow) / 15.0,
            bounce as f32,
        ]
    }
}

fn is_sprite_item(item: ItemId, catalog: &Catalog) -> bool {
    catalog
        .item(item)
        .is_some_and(|definition| definition.sprite)
}

pub(crate) fn mesh(items: &[VisualDrop]) -> DropMeshes {
    mesh_with_catalog(items, content::catalog())
}

pub(crate) fn mesh_with_catalog(items: &[VisualDrop], catalog: &Catalog) -> DropMeshes {
    let (cutout_count, opaque_count) =
        items
            .iter()
            .take(MAX_ITEMS)
            .fold((0, 0), |(cutout, opaque), item| {
                if is_sprite_item(item.item, catalog) {
                    (cutout + 1, opaque)
                } else {
                    (cutout, opaque + 1)
                }
            });
    let mut vertices = Vec::with_capacity(opaque_count * 24 * VERTEX_FLOATS);
    let mut indices = Vec::with_capacity(opaque_count * 36);
    let mut cutout_vertices = Vec::with_capacity(cutout_count * 8 * VERTEX_FLOATS);
    let mut cutout_indices = Vec::with_capacity(cutout_count * 12);
    for item in items.iter().take(MAX_ITEMS) {
        if is_sprite_item(item.item, catalog) {
            emit_cutout_drop(item, &mut cutout_vertices, &mut cutout_indices, catalog);
            continue;
        }
        let (sin, cos) = item.angle.sin_cos();
        let scale = item.presentation_scale(catalog);
        for axis in 0..3 {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            for side in [-1i32, 1] {
                let base = (vertices.len() / VERTEX_FLOATS) as u32;
                let mut normal = [0.0; 3];
                normal[axis] = side as f32;
                for (du, dv) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    let mut local = [0.0; 3];
                    local[axis] += 0.23 * side as f32;
                    local[u] += (du - 0.5) * 0.46;
                    local[v] += (dv - 0.5) * 0.46;
                    let local = Vec3::from_array(local) * scale;
                    let position = item.center
                        + Vec3::new(
                            local.x * cos + local.z * sin,
                            local.y,
                            -local.x * sin + local.z * cos,
                        );
                    let normal = Vec3::from_array(normal);
                    let normal = Vec3::new(
                        normal.x * cos + normal.z * sin,
                        normal.y,
                        -normal.x * sin + normal.z * cos,
                    );
                    vertices.extend(position.to_array());
                    vertices.extend(normal.to_array());
                    let (texture_u, texture_v) = face_uv(axis, du, dv, 1.0, 1.0);
                    vertices.extend([
                        texture_u,
                        texture_v,
                        super::material::item_material_layer_for(catalog, item.item, axis, side)
                            as f32,
                    ]);
                    vertices.extend(item.light_attributes());
                }
                if side > 0 {
                    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                } else {
                    indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
                }
            }
        }
    }
    DropMeshes {
        opaque_vertices: vertices,
        opaque_indices: indices,
        cutout_vertices,
        cutout_indices,
    }
}

fn emit_cutout_drop(
    item: &VisualDrop,
    vertices: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    catalog: &Catalog,
) {
    let (sin, cos) = item.angle.sin_cos();
    let scale = item.presentation_scale(catalog);
    let layer = super::material::item_material_layer_for(catalog, item.item, 1, 1) as f32;
    let half_size = if catalog
        .item(item.item)
        .and_then(|definition| definition.placeable)
        .is_some_and(|state| catalog.block_flags(state) & content::PLANT != 0)
    {
        0.32
    } else {
        0.23
    };
    for (start, end) in [
        ([-half_size, -half_size], [half_size, half_size]),
        ([-half_size, half_size], [half_size, -half_size]),
    ] {
        let base = (vertices.len() / VERTEX_FLOATS) as u32;
        for (t, height, u, v) in [
            (0.0, -half_size, 0.0, 1.0),
            (1.0, -half_size, 1.0, 1.0),
            (1.0, half_size, 1.0, 0.0),
            (0.0, half_size, 0.0, 0.0),
        ] {
            let x = (start[0] + (end[0] - start[0]) * t) * scale;
            let z = (start[1] + (end[1] - start[1]) * t) * scale;
            let position =
                item.center + Vec3::new(x * cos + z * sin, height * scale, -x * sin + z * cos);
            vertices.extend_from_slice(&[
                position.x, position.y, position.z, 0.0, 1.0, 0.0, u, v, layer,
            ]);
            vertices.extend(item.light_attributes());
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::SEEDS;
    use crate::world::GRASS;

    #[test]
    fn block_and_sprite_drops_carry_sky_glow_and_bounce_without_fixed_lighting() {
        for item in [ItemId::new(GRASS.get()), SEEDS] {
            for light in [
                LightSample::default(),
                LightSample {
                    sky: 3,
                    glow: 12,
                    bounce: [17, 29, 43],
                },
            ] {
                let mesh = mesh(&[VisualDrop {
                    item,
                    center: Vec3::ZERO,
                    angle: 0.4,
                    scale: 1.0,
                    light,
                }]);
                let vertices = if item == SEEDS {
                    mesh.cutout_vertices
                } else {
                    mesh.opaque_vertices
                };
                assert!(!vertices.is_empty());
                for vertex in vertices.chunks_exact(VERTEX_FLOATS) {
                    assert_eq!(vertex[9], f32::from(light.sky) / 15.0);
                    assert_eq!(vertex[10], f32::from(light.glow) / 15.0);
                    let bounce = vertex[11] as u32;
                    assert_eq!(
                        [bounce as u8, (bounce >> 8) as u8, (bounce >> 16) as u8],
                        light.bounce
                    );
                }
            }
        }
    }

    #[test]
    fn rotated_drop_stays_bounded_and_uses_all_six_faces() {
        let drop = VisualDrop {
            item: ItemId::new(2),
            center: Vec3::new(10.0, 5.0, -2.0),
            angle: 0.7,
            scale: 1.0,
            light: LightSample::default(),
        };
        let mesh = mesh(&[drop]);
        assert_eq!(mesh.opaque_vertices.len(), 24 * VERTEX_FLOATS);
        assert_eq!(mesh.opaque_indices.len(), 36);
        for vertex in mesh.opaque_vertices.chunks_exact(VERTEX_FLOATS) {
            let position = Vec3::new(vertex[0], vertex[1], vertex[2]);
            assert!(position.distance(drop.center) < 0.41);
        }
    }

    #[test]
    fn grass_side_band_is_at_the_top_on_both_side_axes() {
        let vertices = mesh(&[VisualDrop {
            item: ItemId::new(GRASS.get()),
            center: Vec3::ZERO,
            angle: 0.0,
            scale: 1.0,
            light: LightSample::default(),
        }])
        .opaque_vertices;
        for face in [0, 1, 4, 5] {
            for vertex in vertices[face * 4 * VERTEX_FLOATS..(face + 1) * 4 * VERTEX_FLOATS]
                .chunks_exact(VERTEX_FLOATS)
            {
                assert_eq!(vertex[8], 1.0, "side must use grass-side texture");
                assert_eq!(vertex[7], if vertex[1] > 0.0 { 0.0 } else { 1.0 });
            }
        }
    }

    #[test]
    fn flower_pickup_uses_cutout_crosses_instead_of_cube_faces() {
        let mesh = mesh(&[VisualDrop {
            item: ItemId::new(crate::world::RED_FLOWER.get()),
            center: Vec3::new(4.0, 2.0, -1.0),
            angle: 0.35,
            scale: 1.0,
            light: LightSample::default(),
        }]);
        assert!(mesh.opaque_indices.is_empty());
        assert_eq!(mesh.cutout_vertices.len(), 8 * VERTEX_FLOATS);
        assert_eq!(mesh.cutout_indices.len(), 12);
        assert!(
            mesh.cutout_vertices
                .chunks_exact(VERTEX_FLOATS)
                .all(|vertex| vertex[8] == 12.0)
        );
    }

    #[test]
    fn seed_pickup_uses_item_artwork() {
        let mesh = mesh(&[VisualDrop {
            item: SEEDS,
            center: Vec3::ZERO,
            angle: 0.0,
            scale: 1.0,
            light: LightSample::default(),
        }]);
        assert!(mesh.opaque_indices.is_empty());
        assert_eq!(mesh.cutout_indices.len(), 12);
        assert!(
            mesh.cutout_vertices
                .chunks_exact(VERTEX_FLOATS)
                .all(|vertex| vertex[8] == 17.0)
        );
    }

    #[test]
    fn registered_non_placeable_item_can_choose_cube_or_sprite_drop_mesh() {
        use bloxgloom_host_api::content::{Components, DropSize, Item};
        let mut catalog = Catalog::builtins();
        let mut declarations = crate::content::declarations::Declarations::default();
        for sprite in [false, true] {
            for (label, size) in [
                ("small", DropSize::Small),
                ("normal", DropSize::Normal),
                ("large", DropSize::Large),
            ] {
                declarations
                    .item(Item {
                        key: format!("test:{}_{label}", if sprite { "sprite" } else { "cube" }),
                        name: "Token".into(),
                        swatch: [1.0; 4],
                        texture: "bloxgloom:stone".into(),
                        placeable: None,
                        sprite,
                        drop_size: size,
                        drop_animation: Default::default(),
                        drop_policy: Default::default(),
                        components: Components::None,
                    })
                    .unwrap();
            }
        }
        declarations.install_items_and_tags(&mut catalog).unwrap();
        for sprite in [false, true] {
            let mut widths = Vec::new();
            for label in ["small", "normal", "large"] {
                let key = format!("test:{}_{label}", if sprite { "sprite" } else { "cube" });
                let item = catalog.item_by_key(&key).unwrap();
                let mesh = mesh_with_catalog(
                    &[VisualDrop {
                        item,
                        center: Vec3::ZERO,
                        angle: 0.0,
                        scale: 1.0,
                        light: LightSample::default(),
                    }],
                    &catalog,
                );
                assert_eq!(mesh.cutout_indices.len(), if sprite { 12 } else { 0 });
                assert_eq!(mesh.opaque_indices.len(), if sprite { 0 } else { 36 });
                let vertices = if sprite {
                    &mesh.cutout_vertices
                } else {
                    &mesh.opaque_vertices
                };
                let width = vertices
                    .chunks_exact(VERTEX_FLOATS)
                    .map(|vertex| vertex[1].abs())
                    .fold(0.0_f32, f32::max);
                widths.push(width);
            }
            assert!((widths[0] / widths[1] - 0.75).abs() < 0.0001);
            assert!((widths[2] / widths[1] - 1.25).abs() < 0.0001);
        }
    }
}
