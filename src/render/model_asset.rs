//! Native, bounded GLB assets. CPU parsing/PNG decoding is independent of wgpu.
//! Runtime actors can share this immutable data; poses and looks are separate.
mod animation;
mod controls;
mod load;
pub(crate) use controls::{Appearance, Controls, Look};
use glam::{Mat4, Quat, Vec3};

pub(crate) type Result<T> = std::result::Result<T, String>;
pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
impl Transform {
    fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}
pub(crate) struct Node {
    pub name: String,
    pub parent: Option<usize>,
    pub rest: Transform,
}
pub(crate) struct Binding {
    pub node: usize,
    pub inverse_bind: Mat4,
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}
pub(crate) struct Primitive {
    pub node: usize,
    pub material: usize,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
pub(crate) struct Material {
    pub name: String,
    pub texture: Option<usize>,
    /// glTF baseColorFactor is linear; customization inputs are sRGB.
    pub color: [f32; 4],
    pub alpha_cutoff: Option<f32>,
    pub double_sided: bool,
    pub wrap: [gltf::texture::WrappingMode; 2],
}
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub(crate) struct Model {
    pub nodes: Vec<Node>,
    pub bindings: Vec<Binding>,
    pub primitives: Vec<Primitive>,
    pub materials: Vec<Material>,
    pub images: Vec<Image>,
    pub clips: Vec<animation::Clip>,
    pub controls: Controls,
}
impl Model {
    pub(crate) fn from_glb(bytes: &[u8], controls: Controls) -> Result<Self> {
        load::load(bytes, controls)
    }
}
fn ensure(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn named(value: Option<&str>, prefix: &str, index: usize) -> Result<String> {
    let value = value.map_or_else(|| format!("{prefix}_{index}"), str::to_owned);
    ensure(
        !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control),
        "model names must be 1..128 bytes without control characters",
    )?;
    Ok(value)
}

#[cfg(test)]
mod tests;
