//! Native GLB inspection before installing an authored actor into the world.
use super::*;
use crate::render::{
    model_asset::{Controls, Look, MAX_BYTES, Model},
    model_renderer::ModelRenderer,
};
use serde::Deserialize;
use std::io::Read;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Options {
    pub clip: Option<String>,
    #[serde(default)]
    pub seconds: f32,
    pub blend: Option<Blend>,
    #[serde(default)]
    pub look: Look,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Blend {
    pub clip: String,
    pub seconds: f32,
    pub weight: f32,
}
pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut data = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut data)?;
    if data.len() > limit {
        return Err(format!("file exceeds {} byte limit: {}", limit, path.display()).into());
    }
    Ok(data)
}
pub(crate) fn render(
    path: &Path,
    output: &Path,
    controls: Option<&Path>,
    options: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let controls = controls
        .map(|p| read_bounded(p, 65536).and_then(|b| Ok(serde_json::from_slice::<Controls>(&b)?)))
        .transpose()?
        .unwrap_or_default();
    let model = Model::from_glb(&read_bounded(path, MAX_BYTES)?, controls)?;
    let options = options
        .map(|p| read_bounded(p, 65536).and_then(|b| Ok(serde_json::from_slice::<Options>(&b)?)))
        .transpose()?
        .unwrap_or_default();
    let pose = match &options.blend {
        Some(blend) => model.sample_blended(
            options.clip.as_deref(),
            options.seconds,
            Some(&blend.clip),
            blend.seconds,
            blend.weight,
        )?,
        None => model.sample(options.clip.as_deref(), options.seconds)?,
    };
    let appearance = model.appearance(&options.look)?;
    println!(
        "GLB: {} nodes, {} primitives, {} textures; clips: {}",
        model.nodes.len(),
        model.primitives.len(),
        model.images.len(),
        model
            .clips
            .iter()
            .map(|c| format!("{} ({:.3}s)", c.name, c.duration))
            .collect::<Vec<_>>()
            .join(", ")
    );
    pollster::block_on(render_async(&model, &pose, appearance, output)).map(|_| ())
}
async fn render_async(
    model: &Model,
    pose: &[glam::Mat4],
    appearance: crate::render::model_asset::Appearance,
    path: &Path,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&Default::default()).await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await?;
    let rest = model.sample(None, 0.0)?;
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for primitive in &model.primitives {
        for v in &primitive.vertices {
            let p = Vec3::from_array(v.position);
            let p = (0..4).fold(Vec3::ZERO, |sum, i| {
                sum + rest[v.joints[i] as usize].transform_point3(p) * v.weights[i]
            });
            min = min.min(p);
            max = max.max(p);
        }
    }
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(0.01);
    let eye = center + Vec3::new(0.35, 0.2, -1.0).normalize() * radius * 3.0;
    let matrix = glam::camera::rh::proj::directx::perspective(
        45.0_f32.to_radians(),
        1.0,
        radius * 0.01,
        radius * 10.0,
    ) * glam::camera::rh::view::look_at_mat4(eye, center, Vec3::Y);
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("GLB preview camera"),
        contents: bytemuck::cast_slice(&matrix.to_cols_array()),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut renderer = ModelRenderer::new(&device, &queue, FORMAT, &camera, model);
    renderer.set(&queue, pose, appearance);
    println!("GPU material draw batches: {}", renderer.draw_calls());
    let size = wgpu::Extent3d {
        width: 768,
        height: 768,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("GLB preview color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("GLB preview depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("native GLB preview"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color.create_view(&Default::default()),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.045,
                        g: 0.06,
                        b: 0.07,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.create_view(&Default::default()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        renderer.draw(&mut pass);
    }
    queue.submit(Some(encoder.finish()));
    capture::save_and_read(&device, &queue, &color, 768, 768, path)
}

#[cfg(test)]
mod tests;
