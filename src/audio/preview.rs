//! Device-independent render and real-device smoke tools share the game mixer.
use super::{Command, Controls, Mixer, Preset, SAMPLE_RATE, output::AudioOutput};
use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};
fn validate_seconds(seconds: f32) -> io::Result<()> {
    if !seconds.is_finite() || !(0.1..=120.0).contains(&seconds) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "duration must be0.1–120seconds",
        ));
    }
    Ok(())
}
pub(crate) fn render_preview(
    preset: Preset,
    seconds: f32,
    path: &Path,
    seed: u32,
) -> io::Result<()> {
    validate_seconds(seconds)?;
    let mut mixer = Mixer::new(seed);
    mixer.set_controls(Controls {
        preset,
        ..Controls::default()
    });
    if preset == Preset::Storm {
        mixer.command(Command::Thunder {
            distance: 1200.0,
            angle: 0.7,
        });
    }
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(io::Error::other)?;
    let start = Instant::now();
    let mut buffer = [[0.0; 2]; 256];
    let mut remaining = (seconds * SAMPLE_RATE as f32).round() as usize;
    let mut peak = 0.0f32;
    let mut energy = 0.0f64;
    let mut frames = 0usize;
    while remaining > 0 {
        let count = remaining.min(buffer.len());
        mixer.render(&mut buffer[..count]);
        for frame in &buffer[..count] {
            for sample in frame {
                peak = peak.max(sample.abs());
                energy += f64::from(*sample).powi(2);
                writer
                    .write_sample((sample * 32767.0).round() as i16)
                    .map_err(io::Error::other)?;
            }
        }
        remaining -= count;
        frames += count;
    }
    writer.finalize().map_err(io::Error::other)?;
    let (drops, dropped, active, thunder, thunder_rejected) = mixer.diagnostics();
    tracing::info!(drops,dropped,active,thunder,thunder_rejected,preset=?preset,seconds,render_ms=start.elapsed().as_secs_f64()*1000.0,peak,rms=(energy/(frames*2) as f64).sqrt(),rejected=mixer.rejected,path=%path.display(),"audio preview rendered");
    Ok(())
}
pub(crate) fn play_preview(preset: Preset, seconds: f32) -> io::Result<()> {
    validate_seconds(seconds)?;
    let output = AudioOutput::start(Controls {
        preset,
        ..Controls::default()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !output.stats().available
        && output.stats().device_errors == 0
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !output.stats().available {
        return Err(io::Error::other(
            "audio output unavailable; inspect log for device error",
        ));
    }
    if preset == Preset::Storm {
        output.try_send(Command::Thunder {
            distance: 1200.0,
            angle: 0.7,
        });
    }
    std::thread::sleep(Duration::from_secs_f32(seconds));
    let stats = output.stats();
    tracing::info!(
        available = stats.available,
        sample_rate = stats.sample_rate,
        channels = stats.channels,
        underruns = stats.underrun_frames,
        device_errors = stats.device_errors,
        rejected = stats.rejected_commands,
        "audio device probe finished"
    );
    if stats.device_errors > 0 {
        Err(io::Error::other("audio output failed during probe"))
    } else {
        Ok(())
    }
}

pub(crate) fn play_file(path: &Path, seconds: f32) -> io::Result<()> {
    validate_seconds(seconds)?;
    let clip = std::sync::Arc::new(super::Clip::load(path)?);
    let output = AudioOutput::start(Controls::default());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !output.stats().available
        && output.stats().device_errors == 0
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !output.stats().available
        || !output.try_send(Command::Play {
            clip,
            position: None,
            gain: 1.0,
            looping: false,
            id: 1,
        })
    {
        return Err(io::Error::other("audio output unavailable"));
    }
    std::thread::sleep(Duration::from_secs_f32(seconds));
    output.try_send(Command::Stop(1));
    std::thread::sleep(Duration::from_millis(80));
    if output.stats().device_errors > 0 {
        return Err(io::Error::other("audio device failed"));
    }
    Ok(())
}
