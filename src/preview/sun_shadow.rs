//! Shared preview/benchmark quality override; never overrides saved gameplay settings.
use crate::{config::SunShadowQuality, render};

pub(super) fn quality() -> Result<SunShadowQuality, Box<dyn std::error::Error>> {
    match std::env::var("BLOXGLOOM_SUN_SHADOWS") {
        Ok(value) => SunShadowQuality::parse(&value)
            .ok_or_else(|| "BLOXGLOOM_SUN_SHADOWS must be off, low, medium or high".into()),
        Err(std::env::VarError::NotPresent) => Ok(SunShadowQuality::default()),
        Err(error) => Err(error.into()),
    }
}

type Submesh = Option<(wgpu::Buffer, wgpu::Buffer, u32)>;
type Mesh = (Submesh, Submesh);

#[allow(clippy::too_many_arguments)] // Explicit borrowed GPU resources; no per-frame allocation.
pub(super) fn draw(
    encoder: &mut wgpu::CommandEncoder,
    shadows: &render::sun_shadow::SunShadows,
    pipelines: &(wgpu::RenderPipeline, wgpu::RenderPipeline),
    textures: &wgpu::BindGroup,
    material: Option<&render::custom::Gpu>,
    meshes: &[Mesh],
    drops: Option<&Mesh>,
    avatars: &render::AvatarRenderer,
) {
    let Some(mut pass) = shadows.begin(encoder) else {
        return;
    };
    pass.set_bind_group(0, &shadows.caster_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    if let Some(gpu) = material {
        pass.set_bind_group(2, &gpu.group, &[]);
    }
    for cutout in [false, true] {
        pass.set_pipeline(if cutout { &pipelines.1 } else { &pipelines.0 });
        for mesh in meshes.iter().chain(drops) {
            let mesh = if cutout { &mesh.1 } else { &mesh.0 };
            if let Some((vertices, indices, count)) = mesh {
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*count, 0, 0..1);
            }
        }
    }
    avatars.draw_shadow(&mut pass, &shadows.caster_group);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_local(
    encoder: &mut wgpu::CommandEncoder,
    shadows: &render::local_shadow::LocalShadows,
    pipelines: &(wgpu::RenderPipeline, wgpu::RenderPipeline),
    textures: &wgpu::BindGroup,
    material: Option<&render::custom::Gpu>,
    meshes: &[Mesh],
    drops: Option<&Mesh>,
    avatars: &render::AvatarRenderer,
) {
    for index in shadows.faces_to_update() {
        let face = shadows.face(index);
        let mut pass = face.begin(encoder);
        pass.set_bind_group(0, &face.caster_group, &[]);
        pass.set_bind_group(1, textures, &[]);
        if let Some(gpu) = material {
            pass.set_bind_group(2, &gpu.group, &[]);
        }
        for cutout in [false, true] {
            pass.set_pipeline(if cutout { &pipelines.1 } else { &pipelines.0 });
            for mesh in meshes.iter().chain(drops) {
                let mesh = if cutout { &mesh.1 } else { &mesh.0 };
                if let Some((vertices, indices, count)) = mesh {
                    pass.set_vertex_buffer(0, vertices.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
            }
        }
        avatars.draw_shadow(&mut pass, &face.caster_group);
    }
}
