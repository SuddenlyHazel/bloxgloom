//! CPAL setup and the allocation-free, lock-free device callback.
use super::*;
use cpal::traits::{DeviceTrait, HostTrait};
use rtrb::Consumer;

pub(super) fn open_device(
    consumer: Consumer<Frame>,
    shared: Arc<Shared>,
) -> Result<(cpal::Stream, u32), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no default output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config: cpal::StreamConfig = supported.config();
    if !(1..=32).contains(&config.channels) || !(8_000..=384_000).contains(&config.sample_rate) {
        return Err("unsupported audio device channel count or sample rate".into());
    }
    shared
        .sample_rate
        .store(config.sample_rate, Ordering::Relaxed);
    shared.channels.store(config.channels, Ordering::Relaxed);
    macro_rules! stream {
        ($sample:ty) => {
            build_stream::<$sample>(&device, config, consumer, shared).map_err(|e| e.to_string())?
        };
    }
    let rate = config.sample_rate;
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => stream!(f32),
        cpal::SampleFormat::F64 => stream!(f64),
        cpal::SampleFormat::I8 => stream!(i8),
        cpal::SampleFormat::I16 => stream!(i16),
        cpal::SampleFormat::I24 => stream!(cpal::I24),
        cpal::SampleFormat::I32 => stream!(i32),
        cpal::SampleFormat::I64 => stream!(i64),
        cpal::SampleFormat::U8 => stream!(u8),
        cpal::SampleFormat::U16 => stream!(u16),
        cpal::SampleFormat::U24 => stream!(cpal::U24),
        cpal::SampleFormat::U32 => stream!(u32),
        cpal::SampleFormat::U64 => stream!(u64),
        _ => return Err("unsupported audio device sample format".into()),
    };
    Ok((stream, rate))
}
fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut consumer: Consumer<Frame>,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, cpal::Error> {
    let channels = usize::from(config.channels);
    let errors = shared.clone();
    device.build_output_stream(
        config,
        move |data: &mut [T], _| write_output(data, channels, &mut consumer, &shared),
        move |_| {
            errors.errors.fetch_add(1, Ordering::Relaxed);
        },
        Some(Duration::from_secs(1)),
    )
}
pub(super) fn write_output<T: cpal::Sample + cpal::FromSample<f32>>(
    data: &mut [T],
    channels: usize,
    consumer: &mut Consumer<Frame>,
    shared: &Shared,
) {
    let epoch = shared.epoch.load(Ordering::Acquire);
    let stopping = shared.shutdown.load(Ordering::Acquire);
    let mut underruns = 0;
    // Across this callback there can be no more than RING_FRAMES stale pops.
    let mut stale_budget = RING_FRAMES;
    for output in data.chunks_mut(channels) {
        let mut samples = [0.0; 2];
        if !stopping {
            loop {
                match consumer.pop() {
                    Ok(frame) if frame.epoch == epoch => {
                        samples = frame.samples;
                        break;
                    }
                    Ok(_) if stale_budget > 0 => {
                        stale_budget -= 1;
                    }
                    _ => {
                        underruns += 1;
                        break;
                    }
                }
            }
        }
        for (channel, sample) in output.iter_mut().enumerate() {
            let value = if channels == 1 {
                (samples[0] + samples[1]) * 0.5
            } else {
                match channel {
                    0 => samples[0],
                    1 => samples[1],
                    _ => 0.0,
                }
            };
            *sample = T::from_sample(value.clamp(-1.0, 1.0));
        }
    }
    if underruns > 0 {
        shared.underruns.fetch_add(underruns, Ordering::Relaxed);
    }
}
