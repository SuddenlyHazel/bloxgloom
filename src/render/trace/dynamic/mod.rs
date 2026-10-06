//! Exact current presentation targets, independently of the immutable terrain BVH.
//! Assets are admitted once; frames contain poses and looks, never skinned meshes.
mod asset;
mod drops;
mod gpu;
pub(crate) use asset::{DynamicAsset, Image, Material, Vertex};
pub(crate) use drops::DropTargets;
use glam::{Mat4, Vec3};
pub(crate) use gpu::DynamicGpu;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Deformation {
    Primitive,
    Character,
    Authored,
    Rigid,
}
#[derive(Clone)]
pub(crate) struct DynamicInstance {
    pub asset: Arc<DynamicAsset>,
    pub world: Mat4,
    pub joints: Vec<Mat4>,
    /// Per-primitive linear RGB and multiply/replace selector, matching raster.
    pub parts: Vec<([f32; 4], bool)>,
    pub deformation: Deformation,
    pub pose: [f32; 4],
    pub orientation: [f32; 4],
    pub tint: Vec3,
    pub sky: f32,
    pub cosmetics: [u8; 4],
    pub recipe: crate::appearance::CharacterRecipe,
    /// Retain the owner's world body for secondary rays, outside camera framing.
    pub skip_primary: bool,
}
impl DynamicInstance {
    pub fn rigid(asset: Arc<DynamicAsset>, world: Mat4, sky: f32) -> Self {
        Self {
            asset,
            world,
            joints: Vec::new(),
            parts: Vec::new(),
            deformation: Deformation::Rigid,
            pose: [0.0; 4],
            orientation: [0.0, 0.0, 0.0, 1.0],
            tint: Vec3::ONE,
            sky,
            cosmetics: [0; 4],
            recipe: Default::default(),
            skip_primary: false,
        }
    }
}
#[derive(Clone, Default)]
pub(crate) struct DynamicTargets {
    pub instances: Vec<DynamicInstance>,
}
impl DynamicTargets {
    pub fn append(&mut self, other: &Self) {
        self.instances.extend(other.instances.iter().cloned());
    }
    pub fn clear(&mut self) {
        self.instances.clear();
    }
}

pub(crate) const DEFORMATION_SHADER: &str = include_str!("deformation.wgsl");
pub(crate) const MATERIAL_SHADER: &str = include_str!("material.wgsl");
pub(crate) fn enabled() -> bool {
    std::env::var("BLOXGLOOM_GI").as_deref() == Ok("1") && !crate::render::bsl_reference::enabled()
}
pub(crate) fn shader(catalog: &crate::content::Catalog) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        DEFORMATION_SHADER,
        crate::render::avatars::ray_palettes(catalog),
        MATERIAL_SHADER,
        include_str!("buffers.wgsl"),
        include_str!("material_channels.wgsl"),
        include_str!("intersection.wgsl")
    )
}

#[cfg(test)]
mod tests;
