//! Explicit headless diagnostics; never polls or maps buffers from the live renderer.
use std::{error::Error, sync::mpsc, time::Duration};

pub(crate) const STAGES: [&str; 4] = ["deformation", "transport", "filter", "composite"];
pub(crate) type Samples = [Vec<f64>; 4];
const QUERIES_PER_FRAME: u32 = 8;

pub(super) struct Profile {
    queries: wgpu::QuerySet,
    capacity: u32,
    samples: u32,
    active: bool,
}

pub(super) struct Frame {
    queries: wgpu::QuerySet,
    first: u32,
}

impl Frame {
    pub fn render(&self, stage: u32) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(self.first + stage * 2),
            end_of_pass_write_index: Some(self.first + stage * 2 + 1),
        }
    }

    pub fn compute(&self) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(self.first),
            end_of_pass_write_index: Some(self.first + 1),
        }
    }
}

impl Profile {
    pub fn new(device: &wgpu::Device, frames: usize) -> Result<Self, Box<dyn Error>> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return Err("trace profiling requires GPU timestamp queries".into());
        }
        let capacity = u32::try_from(frames)?;
        let count = capacity
            .checked_mul(QUERIES_PER_FRAME)
            .filter(|count| *count > 0 && *count <= wgpu::QUERY_SET_MAX_QUERIES)
            .ok_or("trace profile exceeds GPU timestamp query capacity")?;
        Ok(Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("headless trace stage timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count,
            }),
            capacity,
            samples: 0,
            active: false,
        })
    }

    pub fn activate(&mut self, active: bool) {
        self.active = active;
    }

    pub fn frame(&mut self) -> Option<Frame> {
        if !self.active || self.samples == self.capacity {
            return None;
        }
        let first = self.samples * QUERIES_PER_FRAME;
        self.samples += 1;
        Some(Frame {
            queries: self.queries.clone(),
            first,
        })
    }

    /// Called once by the headless harness after all measured submissions.
    pub fn read(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Samples, Box<dyn Error>> {
        if self.samples == 0 {
            return Ok(std::array::from_fn(|_| vec![]));
        }
        let count = self.samples * QUERIES_PER_FRAME;
        let size = u64::from(count) * 8;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless trace timestamp resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless trace timestamp readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.resolve_query_set(&self.queries, 0..count, &resolve, 0);
        encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, size);
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(120)),
        })?;
        receiver.recv()??;
        let bytes = readback.get_mapped_range(..)?;
        let timestamps = bytes
            .chunks_exact(8)
            .map(|value| u64::from_le_bytes(value.try_into().unwrap()))
            .collect::<Vec<_>>();
        if let Some(first) = timestamps.chunks_exact(8).next() {
            let origin = first[0];
            let milliseconds = first
                .iter()
                .map(|value| {
                    value.wrapping_sub(origin) as f64 * f64::from(queue.get_timestamp_period())
                        / 1_000_000.0
                })
                .collect::<Vec<_>>();
            println!(
                "trace first-frame timestamp offsets (milliseconds, begin/end per stage): {milliseconds:?}"
            );
        }
        let result = stages(&timestamps, queue.get_timestamp_period());
        drop(bytes);
        readback.unmap();
        Ok(result)
    }
}

fn stages(timestamps: &[u64], period: f32) -> Samples {
    let mut result = std::array::from_fn(|_| Vec::with_capacity(timestamps.len() / 8));
    if period > 0.0 {
        for frame in timestamps.chunks_exact(8) {
            for (stage, samples) in result.iter_mut().enumerate() {
                samples.push(
                    frame[stage * 2 + 1].wrapping_sub(frame[stage * 2]) as f64 * f64::from(period)
                        / 1_000_000.0,
                );
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::stages;

    #[test]
    fn stage_timings_keep_frame_pairs_separate_and_use_gpu_period() {
        let samples = stages(
            &[
                1, 101, 200, 1200, 1300, 2300, 2400, 6400, 9, 209, 500, 2500, 3000, 6000, 7000,
                9000,
            ],
            2.0,
        );
        assert_eq!(samples[0], [0.0002, 0.0004]);
        assert_eq!(samples[1], [0.002, 0.004]);
        assert_eq!(samples[2], [0.002, 0.006]);
        assert_eq!(samples[3], [0.008, 0.004]);
        assert!(
            stages(&[1, 2, 3, 4, 5, 6, 7, 8], 0.0)
                .iter()
                .all(Vec::is_empty)
        );
    }
}
