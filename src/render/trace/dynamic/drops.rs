//! Native pickup assets, animated only by the existing presentation transforms.
use super::*;
use crate::{
    content::Catalog,
    items::ItemId,
    render::drops::{self, VisualDrop},
};
use std::collections::HashMap;
pub(crate) struct DropTargets {
    assets: HashMap<ItemId, Arc<DynamicAsset>>,
    targets: DynamicTargets,
}
impl DropTargets {
    pub fn new(catalog: &Catalog) -> Self {
        let mut result = Self {
            assets: HashMap::new(),
            targets: Default::default(),
        };
        if !super::enabled() {
            return result;
        }
        for definition in catalog.items() {
            let id = definition.id;
            let item = VisualDrop {
                item: id,
                center: Vec3::ZERO,
                angle: 0.0,
                scale: 1.0,
                light: crate::lighting::LightSample::default(),
            };
            let mesh = drops::mesh_with_catalog(&[item], catalog);
            let mut vertices = Vec::new();
            let mut triangles = Vec::new();
            let mut materials = Vec::new();
            for (input, indices, cutout) in [
                (&mesh.opaque_vertices, &mesh.opaque_indices, false),
                (&mesh.cutout_vertices, &mesh.cutout_indices, true),
            ] {
                let base = vertices.len() as u32;
                vertices.extend(
                    input
                        .chunks_exact(crate::render::VERTEX_FLOATS)
                        .map(|v| Vertex {
                            position: [v[0], v[1], v[2]],
                            normal: [v[3], v[4], v[5]],
                            uv: [v[6], v[7]],
                            joints: [0; 4],
                            weights: [0.0; 4],
                            part: 0,
                        }),
                );
                for i in indices.chunks_exact(3) {
                    let layer = input[i[0] as usize * crate::render::VERTEX_FLOATS + 8] as u32;
                    let material = materials.len() as u32;
                    materials.push(Material {
                        kind: 3,
                        base: [1.0; 4],
                        image: None,
                        wrap: [1; 2],
                        alpha_cutoff: if cutout { 0.5 } else { -1.0 },
                        double_sided: cutout,
                        surface: 0,
                        group: 0,
                        catalog_layer: layer,
                    });
                    triangles.push(([base + i[0], base + i[1], base + i[2]], material, 0));
                }
            }
            result.assets.insert(
                id,
                DynamicAsset::build(vertices, triangles, materials, Vec::new()),
            );
        }
        result
    }
    pub fn set(&mut self, items: &[VisualDrop]) {
        self.targets.clear();
        for item in items.iter().take(drops::MAX_ITEMS) {
            if let Some(asset) = self.assets.get(&item.item) {
                let world = Mat4::from_scale_rotation_translation(
                    Vec3::splat(item.scale),
                    glam::Quat::from_rotation_y(item.angle),
                    item.center,
                );
                self.targets.instances.push(DynamicInstance::rigid(
                    asset.clone(),
                    world,
                    f32::from(item.light.sky) / 15.0,
                ));
            }
        }
    }
    pub fn targets(&self) -> &DynamicTargets {
        &self.targets
    }
}
