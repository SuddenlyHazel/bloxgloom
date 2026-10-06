//! Natural captures include the same distant terrain and near-coverage masks as play.
use crate::render::{self, Camera};
use std::error::Error;

pub(super) const DISTANCE: u16 = 512;

pub(in crate::preview) fn build(
    device: &wgpu::Device,
    camera: Camera,
    pipeline: &wgpu::RenderPipeline,
    materials: &wgpu::BindGroup,
    shadows: &wgpu::BindGroup,
) -> Result<render::lod::Gpu, Box<dyn Error>> {
    let mut gpu = render::lod::Gpu::new(device, render::post::HDR_FORMAT, pipeline, materials);
    gpu.set_horizon(DISTANCE);
    gpu.set_sun_shadows(shadows.clone());
    let (meshes, _) = crate::preview::lod::terrain_meshes(camera, DISTANCE)?;
    for mesh in meshes {
        gpu.enqueue(mesh)
            .map_err(|_| "landscape distant terrain exceeds residency budget")?;
        if gpu.upload(device) == 0 {
            return Err("landscape distant terrain upload stalled".into());
        }
    }
    Ok(gpu)
}

pub(in crate::preview) fn configure_camera(data: &mut [f32; 56]) {
    if !render::bsl_reference::enabled() {
        data[28] = f32::from(DISTANCE) * 0.65;
        data[29] = f32::from(DISTANCE);
    }
}
