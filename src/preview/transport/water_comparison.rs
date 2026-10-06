//! Apply the unchanged production post stack to cached GI comparisons.
use crate::render::post::PostProcess;
use std::{error::Error, path::Path};
pub(in crate::preview) fn write(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    post: &mut PostProcess,
    hdr: [wgpu::TextureView; 2],
    directory: &Path,
) -> Result<(), Box<dyn Error>> {
    if post.temporal_enabled() {
        return Err("cached production water comparison requires BLOXGLOOM_TAA=0".into());
    }
    let size = post.scene.texture().size();
    let saved = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("saved immutable preview scene"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::render::post::HDR_FORMAT,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_texture(
        post.scene.texture().as_image_copy(),
        saved.as_image_copy(),
        size,
    );
    let outputs = hdr
        .iter()
        .map(|input| {
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("cached production post result"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: super::super::FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            encoder.copy_texture_to_texture(
                input.texture().as_image_copy(),
                post.scene.texture().as_image_copy(),
                size,
            );
            post.encode(
                device,
                queue,
                &mut encoder,
                &output.create_view(&Default::default()),
            );
            output
        })
        .collect::<Vec<_>>();
    encoder.copy_texture_to_texture(
        saved.as_image_copy(),
        post.scene.texture().as_image_copy(),
        size,
    );
    queue.submit([encoder.finish()]);
    for (name, output) in ["production-legacy", "production-optical"]
        .iter()
        .zip(&outputs)
    {
        let stride = (size.width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(stride) * u64::from(size.height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(size.height),
                },
            },
            size,
        );
        let submitted = queue.submit([encoder.finish()]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(deadline.saturating_duration_since(std::time::Instant::now())),
        })?;
        rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))??;
        let mapped = buffer.slice(..).get_mapped_range()?;
        let pixels = mapped
            .chunks_exact(stride as usize)
            .flat_map(|row| row[..size.width as usize * 4].iter().copied())
            .collect::<Vec<_>>();
        drop(mapped);
        buffer.unmap();
        super::super::write_png(
            &directory.join(format!("{name}.png")),
            size.width,
            size.height,
            &pixels,
        )?;
    }
    Ok(())
}
