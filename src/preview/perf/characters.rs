//! Character-only comparison through the production renderer. No terrain or UI.
//! Submission samples never explicitly wait for the GPU; readback happens once.
use crate::{
    appearance::{CharacterRecipe, HAIR},
    content::Catalog,
    render::{self, AvatarModel, AvatarRenderer, MAX_AVATARS, VisualAvatar},
};
use glam::{Mat4, Vec3};
use std::{
    error::Error,
    sync::mpsc,
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const WARMUP_FRAMES: usize = 30;
const MAX_MEASURED_FRAMES: usize = 2000;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BYTES_PER_ROW: u32 = WIDTH * 8;
const _: () = {
    assert!(MAX_MEASURED_FRAMES * 2 <= wgpu::QUERY_SET_MAX_QUERIES as usize);
    assert!(BYTES_PER_ROW.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));
};

/// None selects classic; Some(0..=13) selects an authored hairstyle (0 is bald).
pub(crate) fn run_character_benchmark(
    measured_frames: usize,
    actor_count: usize,
    hair: Option<u8>,
) -> Result<(), Box<dyn Error>> {
    validate(measured_frames, actor_count, hair)?;
    pollster::block_on(run(measured_frames, actor_count, hair))
}

fn validate(frames: usize, count: usize, hair: Option<u8>) -> Result<(), Box<dyn Error>> {
    if !(1..=MAX_MEASURED_FRAMES).contains(&frames) {
        return Err(
            format!("character benchmark frames must be in 1..={MAX_MEASURED_FRAMES}").into(),
        );
    }
    if !(1..=MAX_AVATARS).contains(&count) {
        return Err(format!("character benchmark actors must be in 1..={MAX_AVATARS}").into());
    }
    if hair.is_some_and(|id| usize::from(id) >= HAIR.len()) {
        return Err("character benchmark hair must be classic or a registered ID in 0..=13".into());
    }
    Ok(())
}

/// An XY grid avoids hiding rows behind one another. All actors face the camera;
/// count changes screen coverage, so compare hairstyles at the same actor count.
fn scene(count: usize, hair: Option<u8>) -> (Vec<VisualAvatar>, Mat4) {
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let columns = ((count as f32 * aspect * 2.5 / 1.4).sqrt().ceil() as usize).min(count);
    let rows = count.div_ceil(columns);
    let grid_width = columns as f32 * 1.4;
    let grid_height = rows as f32 * 2.5;
    let half_height = (grid_height * 0.5 + 0.3).max((grid_width * 0.5 + 0.3) / aspect);
    let half_width = half_height * aspect;
    let matrix =
        glam::camera::rh::proj::directx::orthographic(
            -half_width,
            half_width,
            -half_height,
            half_height,
            0.1,
            20.0,
        ) * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let actors = (0..count)
        .map(|index| VisualAvatar {
            animation: Default::default(),
            model: AvatarModel::Player,
            pose: [0.0; 4],
            motion: None,
            character_pose: [0.0; 3],
            character_crouch: 0.0,
            character_tool: None,
            character_recipe: hair.map(|hair| CharacterRecipe {
                hair,
                eyes: (index % 8) as u8,
                mouth: (index % 6) as u8,
                iris: index.is_multiple_of(2).then_some([80, 160, 220]),
            }),
            airborne: false,
            id: index as u64 + 1,
            position: Vec3::new(
                (index % columns) as f32 * 1.4 - (columns - 1) as f32 * 0.7,
                (index / columns) as f32 * 2.5 - grid_height * 0.5,
                0.0,
            ),
            cosmetics: [(index % 6) as u8, (index % 8) as u8, (index % 6) as u8, 0],
            light_levels: [15, 0, 0, 0],
            bounce: [0; 4],
            glow_bounce: [0; 4],
            tint: [1.0; 3],
        })
        .collect();
    (actors, matrix)
}

async fn run(frames: usize, count: usize, hair: Option<u8>) -> Result<(), Box<dyn Error>> {
    let setup = Instant::now();
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await?;
    let adapter_info = adapter.get_info();
    let timestamps = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_features: if timestamps {
                wgpu::Features::TIMESTAMP_QUERY
            } else {
                wgpu::Features::empty()
            },
            ..Default::default()
        })
        .await?;
    let query_count = u32::try_from(frames * 2)?;
    if timestamps && query_count > wgpu::QUERY_SET_MAX_QUERIES {
        return Err("character benchmark exceeds GPU timestamp query bound".into());
    }
    let queries = timestamps.then(|| {
        device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("character benchmark timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: query_count,
        })
    });
    let (mut actors, matrix) = scene(count, hair);
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("character benchmark camera"),
        contents: bytemuck::cast_slice(
            &render::daylight::Atmosphere::at(crate::daylight::INITIAL_MS).camera_data(matrix),
        ),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let catalog = Catalog::builtins();
    let mut renderer = AvatarRenderer::new(&device, &queue, FORMAT, &camera, &catalog);
    renderer.set_authored(hair.is_some());
    let size = wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("character benchmark HDR target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("character benchmark depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
    let mut update_ms = Vec::with_capacity(frames);
    let mut submission_ms = Vec::with_capacity(frames);
    let mut triangles = 0;
    for frame in 0..WARMUP_FRAMES + frames {
        let start = Instant::now();
        for (index, avatar) in actors.iter_mut().enumerate() {
            let phase = frame as f32 / 60.0 + index as f32 * 0.037;
            avatar.character_pose = [phase, phase, 1.0];
        }
        let update_start = Instant::now();
        renderer.set(&queue, &actors);
        let update = update_start.elapsed().as_secs_f64() * 1000.0;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("character benchmark frame"),
        });
        let measured = frame.checked_sub(WARMUP_FRAMES);
        let timestamp_writes = measured.and_then(|index| {
            queries
                .as_ref()
                .map(|query_set| wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(index as u32 * 2),
                    end_of_pass_write_index: Some(index as u32 * 2 + 1),
                })
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("character-only benchmark pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes,
                ..Default::default()
            });
            triangles = renderer.draw(&mut pass);
        }
        queue.submit(Some(encoder.finish()));
        if measured.is_some() {
            update_ms.push(update);
            submission_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    // One final submission resolves all measured queries and copies final pixels.
    // Mapping requests precede the sole explicit GPU wait; no frame is polled.
    let pixel_bytes = u64::from(BYTES_PER_ROW) * u64::from(HEIGHT);
    let pixels = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("character benchmark final pixels"),
        size: pixel_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let timestamp_buffers = queries.as_ref().map(|_| {
        let bytes = u64::from(query_count) * 8;
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage,
                mapped_at_creation: false,
            })
        };
        (
            buffer(
                "character timestamps resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            buffer(
                "character timestamps readback",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
        )
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("character benchmark final readback"),
    });
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &pixels,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(BYTES_PER_ROW),
                rows_per_image: Some(HEIGHT),
            },
        },
        size,
    );
    if let (Some(queries), Some((resolve, readback))) = (&queries, &timestamp_buffers) {
        encoder.resolve_query_set(queries, 0..query_count, resolve, 0);
        encoder.copy_buffer_to_buffer(resolve, 0, readback, 0, u64::from(query_count) * 8);
    }
    let submission = queue.submit(Some(encoder.finish()));
    let (pixel_sender, pixel_receiver) = mpsc::channel();
    pixels.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = pixel_sender.send(result);
    });
    let (time_sender, time_receiver) = mpsc::channel();
    if let Some((_, readback)) = &timestamp_buffers {
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = time_sender.send(result);
        });
    }
    device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(Duration::from_secs(120)),
    })?;
    pixel_receiver.recv_timeout(Duration::from_secs(30))??;
    let mapped = pixels.get_mapped_range(..)?;
    let checksum = mapped.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    drop(mapped);
    pixels.unmap();
    let period = queue.get_timestamp_period();
    let mut gpu_ms = Vec::with_capacity(frames);
    if let Some((_, readback)) = &timestamp_buffers {
        time_receiver.recv_timeout(Duration::from_secs(30))??;
        let mapped = readback.get_mapped_range(..)?;
        if period.is_finite() && period > 0.0 {
            for pair in mapped.chunks_exact(16) {
                let start = u64::from_le_bytes(pair[..8].try_into()?);
                let end = u64::from_le_bytes(pair[8..].try_into()?);
                gpu_ms.push(end.wrapping_sub(start) as f64 * f64::from(period) / 1_000_000.0);
            }
        }
        drop(mapped);
        readback.unmap();
    }
    let mode = hair.map_or_else(
        || "classic".to_owned(),
        |id| format!("authored hair={id} ({})", HAIR[usize::from(id)]),
    );
    eprintln!(
        "character benchmark: {mode}, actors={count}, {WIDTH}x{HEIGHT} RGBA16Float, warmup={WARMUP_FRAMES}, measured={frames}, triangles/frame={triangles}, triangles/actor={}",
        triangles / count
    );
    eprintln!(
        "adapter: {} ({:?}, {:?}), driver={} {}",
        adapter_info.name,
        adapter_info.backend,
        adapter_info.device_type,
        adapter_info.driver,
        adapter_info.driver_info
    );
    super::print_percentiles("CPU joint/instance update + queue writes", &update_ms);
    super::print_percentiles(
        "CPU total frame submission (includes update)",
        &submission_ms,
    );
    if gpu_ms.is_empty() {
        eprintln!("GPU character pass: unavailable (timestamps unsupported or invalid period)");
    } else {
        super::print_percentiles("GPU character pass including target/depth clear", &gpu_ms);
    }
    eprintln!(
        "setup={setup_ms:.3} ms (excluded); final HDR pixel checksum={checksum:016x}; timestamps period={period:.3} ns/tick"
    );
    eprintln!(
        "measurement: deterministic nonoverlapping XY grid, full walk blend at synthetic 60 Hz; no terrain, post, HUD, network or present; CPU update includes production joint sampling/instance packing/queue writes, skinning itself runs on GPU; total is submit-side CPU and may include driver backpressure, not GPU completion latency; no per-frame wait, final readback only; compare styles at identical count/resolution/frame count"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
