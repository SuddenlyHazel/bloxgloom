//! Shared diagnostic-preload and normal bounded-upload resource creation.
use super::{ChunkMesh, PerfGpuMesh, PerfGpuSubmesh};
use wgpu::util::DeviceExt;

pub(super) fn mesh(device: &wgpu::Device, mesh: &ChunkMesh) -> PerfGpuMesh {
    let upload = |vertices: &[f32], indices: &[u32]| {
        if indices.is_empty() {
            return None;
        }
        Some(PerfGpuSubmesh {
            vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("perf chunk vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("perf chunk indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            indices: indices.len() as u32,
        })
    };
    PerfGpuMesh {
        opaque: upload(&mesh.vertices, &mesh.indices),
        cutout: upload(&mesh.cutout_vertices, &mesh.cutout_indices),
        water: upload(&mesh.water_vertices, &mesh.water_indices),
    }
}
