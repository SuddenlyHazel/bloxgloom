//! Distant terrain appearance, CPU surfaces, ready coverage, and GPU resources.
mod colors;
mod gpu;
mod mesh;
mod selection;
pub(crate) use colors::FaceColors;
pub(crate) use gpu::Gpu;
pub(crate) use mesh::{Mesh, mesh};
pub(crate) use selection::desired_tiles;
#[cfg(test)]
mod tests;
