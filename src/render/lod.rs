//! Distant terrain appearance, CPU surfaces, ready coverage, and GPU resources.
mod colors;
mod coverage;
mod forest;
mod gpu;
mod mesh;
mod near_coverage;
mod pipelines;
mod ray;
mod selection;
mod surface;
mod vertex;
pub(crate) use colors::FaceColors;
pub(crate) use gpu::{Gpu, UploadError};
pub(crate) use mesh::{Mesh, mesh};
pub(crate) use selection::desired_tiles;
#[cfg(test)]
pub(crate) mod qa;
#[cfg(test)]
mod tests;
