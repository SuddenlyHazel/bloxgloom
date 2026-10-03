use glam::{Mat4, Vec3, Vec4, camera::rh};

use crate::world::{CHUNK_SIZE, ChunkKey};

use super::{Camera, DEPTH_FORMAT};

pub(crate) fn view_projection(camera: Camera, width: u32, height: u32) -> Mat4 {
    let view = rh::view::look_to_mat4(camera.position, camera.direction(), Vec3::Y);
    let projection = rh::proj::directx::perspective(
        camera.fov_y_radians,
        width as f32 / height as f32,
        0.05,
        4096.0,
    );
    projection * view
}

pub(super) fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

pub(crate) fn chunk_visible(matrix: Mat4, key: ChunkKey) -> bool {
    chunk_visible_padded(matrix, key, 0.0)
}

pub(super) fn chunk_visible_padded(matrix: Mat4, key: ChunkKey, padding: f32) -> bool {
    let n = CHUNK_SIZE as f32;
    let min = Vec3::new(key.x as f32 * n, key.y as f32 * n, key.z as f32 * n);
    let max = min + Vec3::splat(n + padding);
    let min = min - Vec3::splat(padding);
    // Reject only when all corners lie outside one clip plane. This avoids any
    // dependence on matrix row/column extraction conventions.
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ]
    .map(|p| matrix * p.extend(1.0));
    for plane in 0..6 {
        if corners.iter().all(|v| outside_clip(*v, plane)) {
            return false;
        }
    }
    true
}

fn outside_clip(v: Vec4, plane: usize) -> bool {
    match plane {
        0 => v.x < -v.w,
        1 => v.x > v.w,
        2 => v.y < -v.w,
        3 => v.y > v.w,
        4 => v.z < 0.0,
        _ => v.z > v.w,
    }
}
