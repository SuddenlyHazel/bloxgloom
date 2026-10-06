//! Bound experimental transport work per Metal command buffer without changing rays.
use super::Gpu;
use crate::render::trace::profiling;
use std::time::{Duration, Instant};

const TILE_EDGE: u32 = 64;

pub(super) struct Checkpoint {
    tiles: u32,
    timeout: Duration,
}

pub(super) fn configured_edge() -> u32 {
    // The unbounded control is for small equivalence fixtures, not production captures.
    if std::env::var("BLOXGLOOM_GI_TILE").as_deref() == Ok("0") {
        0
    } else {
        TILE_EDGE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tile {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn tiles(width: u32, height: u32, edge: u32) -> Vec<Tile> {
    let mut result = Vec::new();
    let step = if edge == 0 {
        width.max(height).max(1)
    } else {
        edge
    };
    for y in (0..height).step_by(step as usize) {
        for x in (0..width).step_by(step as usize) {
            result.push(Tile {
                x,
                y,
                width: step.min(width - x),
                height: step.min(height - y),
            });
        }
    }
    result
}

impl Gpu {
    /// Explicit headless opt-in. Live rendering never installs this policy.
    pub fn set_headless_transport_scheduling(
        &mut self,
        edge: Option<u32>,
        inflight: u32,
    ) -> Result<(), String> {
        if edge.is_some_and(|edge| ![32, 64, 128].contains(&edge)) {
            return Err("headless GI tile edge must be 32, 64 or 128".into());
        }
        if inflight > 70 {
            return Err("headless GI tile inflight must be 0 through 70".into());
        }
        let edge = edge.unwrap_or(self.transport_tile_edge);
        if inflight > 0 && edge == 0 {
            return Err("headless GI tile checkpoints require tiled transport".into());
        }
        self.transport_tile_edge = edge;
        self.transport_checkpoint = (inflight > 0).then_some(Checkpoint {
            tiles: inflight,
            timeout: Duration::from_secs(30),
        });
        Ok(())
    }

    pub fn take_scheduling_error(&mut self) -> Option<String> {
        self.scheduling_error.take()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn encode_transport(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        group: &wgpu::BindGroup,
        materials: &wgpu::BindGroup,
        current: usize,
        profile: Option<&profiling::Frame>,
    ) -> Result<(), String> {
        let extent = self.history[current].texture().size();
        let work = tiles(extent.width, extent.height, self.transport_tile_edge);
        let bounded = self.transport_tile_edge != 0;
        let mut group_start = Instant::now();
        let mut first_pending = 0usize;
        let mut encode_submit = Duration::ZERO;
        if bounded {
            // Raster inputs, baseline copy and current geometry must precede every tile.
            // No render pass is alive here; leave the caller a fresh continuation.
            let prelude = std::mem::replace(
                encoder,
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("post-transport continuation"),
                }),
            );
            let start = Instant::now();
            let prelude_submission = queue.submit([prelude.finish()]);
            encode_submit += start.elapsed();
            if let Some(policy) = &self.transport_checkpoint {
                let completion = Instant::now();
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: Some(prelude_submission),
                        timeout: Some(policy.timeout),
                    })
                    .map_err(|error| {
                        format!("GI frame{} prelude completion failed: {error}", self.frame)
                    })?;
                println!(
                    "GI tile prelude: frame={} completion={:.3}ms",
                    self.frame,
                    completion.elapsed().as_secs_f64() * 1000.0
                );
                group_start = Instant::now();
                encode_submit = Duration::ZERO;
            }
        }
        for (index, tile) in work.iter().enumerate() {
            if bounded {
                let start = Instant::now();
                let mut tile_encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("bounded ray transport tile"),
                    });
                self.encode_tile(
                    &mut tile_encoder,
                    group,
                    materials,
                    current,
                    *tile,
                    index == 0,
                    index + 1 == work.len(),
                    profile,
                );
                let submission = queue.submit([tile_encoder.finish()]);
                encode_submit += start.elapsed();
                if let Some(policy) = &self.transport_checkpoint
                    && ((index + 1 - first_pending) as u32 >= policy.tiles
                        || index + 1 == work.len())
                {
                    let completion = Instant::now();
                    device.poll(wgpu::PollType::Wait {submission_index:Some(submission),timeout:Some(policy.timeout)}).map_err(|error|format!("GI frame{} tile completion failed: tiles{}..{} last=({},{},{}x{}): {error}",self.frame,first_pending,index+1,tile.x,tile.y,tile.width,tile.height))?;
                    println!(
                        "GI tile checkpoint: frame={} tiles={}..{}/{} last=({},{},{}x{}) encode-submit={:.3}ms completion={:.3}ms group={:.3}ms",
                        self.frame,
                        first_pending,
                        index + 1,
                        work.len(),
                        tile.x,
                        tile.y,
                        tile.width,
                        tile.height,
                        encode_submit.as_secs_f64() * 1000.0,
                        completion.elapsed().as_secs_f64() * 1000.0,
                        group_start.elapsed().as_secs_f64() * 1000.0
                    );
                    first_pending = index + 1;
                    group_start = Instant::now();
                    encode_submit = Duration::ZERO;
                }
            } else {
                self.encode_tile(
                    encoder, group, materials, current, *tile, true, true, profile,
                );
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_tile(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        group: &wgpu::BindGroup,
        materials: &wgpu::BindGroup,
        current: usize,
        tile: Tile,
        first: bool,
        last: bool,
        profile: Option<&profiling::Frame>,
    ) {
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: if first {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let timestamps = profile.filter(|_| first || last).map(|frame| {
            let mut writes = frame.render(1);
            if !first {
                writes.beginning_of_pass_write_index = None;
            }
            if !last {
                writes.end_of_pass_write_index = None;
            }
            writes
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("multi-bounce ray transport tile"),
            timestamp_writes: timestamps,
            color_attachments: &[
                attachment(&self.history[current]),
                attachment(&self.history_geometry[current]),
                attachment(&self.primary_transmission[current]),
                attachment(&self.current_correction),
            ],
            ..Default::default()
        });
        // Keep the original full-target viewport, fragment coordinates and derivative quads.
        pass.set_scissor_rect(tile.x, tile.y, tile.width, tile.height);
        pass.set_pipeline(&self.trace);
        pass.set_bind_group(0, group, &[]);
        pass.set_bind_group(1, materials, &[]);
        pass.set_bind_group(2, &self.dynamic.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests;
