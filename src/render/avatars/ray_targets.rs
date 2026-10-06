//! Ray assets reuse admitted raster geometry, native pixels and current world poses.
use super::*;
use crate::render::trace::dynamic::{
    Deformation, DynamicAsset, DynamicInstance, Image, Material, Vertex,
};
use std::sync::Arc;
pub(super) fn instance(
    asset: Arc<DynamicAsset>,
    avatar: &VisualAvatar,
    world: glam::Mat4,
    deformation: Deformation,
    joints: Vec<glam::Mat4>,
    parts: Vec<([f32; 4], bool)>,
) -> DynamicInstance {
    DynamicInstance {
        asset,
        world,
        joints,
        parts,
        deformation,
        pose: avatar.pose,
        orientation: avatar
            .motion
            .map_or([0.0, 0.0, 0.0, 1.0], |m| m.orientation),
        tint: Vec3::from_array(avatar.tint),
        sky: f32::from(avatar.light_levels[0]) / 15.0,
        cosmetics: avatar.cosmetics,
        recipe: avatar.character_recipe.unwrap_or_default(),
        skip_primary: false,
    }
}
pub(super) fn primitive(mesh: &mesh::AvatarMesh, range: std::ops::Range<u32>) -> Arc<DynamicAsset> {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut materials = Vec::new();
    for indices in mesh.indices[range.start as usize..range.end as usize].chunks_exact(3) {
        let base = vertices.len() as u32;
        let first = &mesh.vertices[indices[0] as usize];
        let material = materials.len() as u32;
        materials.push(Material::flat(first.color));
        for index in indices {
            let v = &mesh.vertices[*index as usize];
            vertices.push(Vertex {
                position: v.position,
                normal: v.normal,
                uv: [0.0; 2],
                joints: [0; 4],
                weights: [0.0; 4],
                part: v.part,
            });
        }
        triangles.push(([base, base + 1, base + 2], material, first.part));
    }
    DynamicAsset::build(vertices, triangles, materials, Vec::new())
}
pub(super) fn character(
    asset: &character_asset::CharacterAsset,
    body: u32,
    hair: u32,
) -> Arc<DynamicAsset> {
    let vertices = asset
        .vertices
        .iter()
        .map(|v| Vertex {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
            joints: [v.joint as u32, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
            part: 0,
        })
        .collect();
    let mut materials = Vec::new();
    let mut keys = std::collections::HashMap::new();
    let triangles = asset
        .indices
        .chunks_exact(3)
        .filter(|i| {
            let group = asset.vertices[i[0] as usize].material;
            group == body || (hair > 0 && group == hair)
        })
        .map(|i| {
            let v = &asset.vertices[i[0] as usize];
            let material = *keys
                .entry((v.material, v.surface, v.texture))
                .or_insert_with(|| {
                    let index = materials.len() as u32;
                    materials.push(Material {
                        kind: 2,
                        base: [1.0; 4],
                        image: Some(v.texture as usize),
                        wrap: [0; 2],
                        alpha_cutoff: 0.05,
                        double_sided: true,
                        surface: v.surface,
                        group: v.material,
                        catalog_layer: 0,
                    });
                    index
                });
            ([i[0], i[1], i[2]], material, 0)
        })
        .collect();
    static IMAGES: std::sync::OnceLock<Vec<Image>> = std::sync::OnceLock::new();
    let images = IMAGES
        .get_or_init(|| {
            asset
                .images
                .iter()
                .map(|i| Image {
                    width: i.width,
                    height: i.height,
                    rgba: i.rgba.clone().into(),
                })
                .collect()
        })
        .clone();
    DynamicAsset::build(vertices, triangles, materials, images)
}
pub(super) fn authored(model: &crate::render::model_asset::Model) -> Arc<DynamicAsset> {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for (part, p) in model.primitives.iter().enumerate() {
        let base = vertices.len() as u32;
        vertices.extend(p.vertices.iter().map(|v| Vertex {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
            joints: v.joints,
            weights: v.weights,
            part: part as u32,
        }));
        triangles.extend(p.indices.chunks_exact(3).map(|i| {
            (
                [base + i[0], base + i[1], base + i[2]],
                p.material as u32,
                part as u32,
            )
        }));
    }
    let wrap = |w| match w {
        gltf::texture::WrappingMode::ClampToEdge => 0,
        gltf::texture::WrappingMode::Repeat => 1,
        gltf::texture::WrappingMode::MirroredRepeat => 2,
    };
    let materials = model
        .materials
        .iter()
        .map(|m| Material {
            kind: 1,
            base: m.color,
            image: m.texture,
            wrap: m.wrap.map(wrap),
            alpha_cutoff: m.alpha_cutoff.unwrap_or(-1.0),
            double_sided: m.double_sided,
            surface: 0,
            group: 0,
            catalog_layer: 0,
        })
        .collect();
    let images = model
        .images
        .iter()
        .map(|i| Image {
            width: i.width,
            height: i.height,
            rgba: i.rgba.clone().into(),
        })
        .collect();
    DynamicAsset::build(vertices, triangles, materials, images)
}

#[cfg(test)]
pub(in crate::render) fn idle_ray_target(seconds: f32) -> DynamicInstance {
    static ASSET: std::sync::OnceLock<Arc<DynamicAsset>> = std::sync::OnceLock::new();
    let character = character_asset::CharacterAsset::builtin();
    let asset = ASSET
        .get_or_init(|| self::character(character, 0, 0))
        .clone();
    let mut target = DynamicInstance::rigid(
        asset,
        glam::Mat4::from_translation(Vec3::new(0.0, -0.7, -1.5)),
        1.0,
    );
    target.deformation = Deformation::Character;
    target.joints = character
        .sample_gameplay_look(seconds, 0.0, 0.0, 0.0, 0.0, None, [0.0; 2])
        .to_vec();
    target.skip_primary = true;
    target
}
