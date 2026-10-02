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
pub(crate) fn render_material_preview(
    profile: &str,
    seconds: f32,
    path: &Path,
    seed: u32,
) -> io::Result<()> {
    use super::rain_scene::{RainMaterial, RainScene};
    let material = if profile == "custom" {
        RainMaterial::Metal
    } else if profile == "split" {
        RainMaterial::Wood
    } else {
        RainMaterial::parse(profile)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown rain material"))?
    };
    let mut scene = RainScene::patch(material);
    if profile == "custom" {
        for tile in &mut std::sync::Arc::make_mut(&mut scene).tiles {
            tile.impact = Some(bloxgloom_host_api::content::ImpactProfile {
                gain: 0.8,
                click: 0.55,
                frequency_hz: [450., 1100.],
                damping_per_s: [180., 350.],
                resonance: 0.65,
                lowpass_hz: 5000.,
            });
        }
    }
    if profile == "split" {
        for tile in &mut std::sync::Arc::make_mut(&mut scene).tiles {
            if tile.centre[2] > 0.0 {
                tile.material = RainMaterial::Metal;
            }
        }
    }
    render(
        Preset::Off,
        seconds,
        path,
        seed,
        Some((
            scene,
            super::WeatherSound {
                rain_mm_h: 30.0,
                exposure: 1.0,
                ..super::WeatherSound::default()
            },
        )),
        profile == "split",
    )
}
pub(crate) fn render_insect_preview(
    kind: &str,
    seconds: f32,
    path: &Path,
    seed: u32,
) -> io::Result<()> {
    use super::rain_scene::{RainMaterial, RainScene};
    let (material, daylight) = match kind {
        "crickets" => (RainMaterial::Dirt, 0.0),
        "cicadas" => (RainMaterial::Leaf, 1.0),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected crickets or cicadas",
            ));
        }
    };
    render(
        Preset::Off,
        seconds,
        path,
        seed,
        Some((
            RainScene::patch(material),
            super::WeatherSound {
                daylight,
                exposure: 1.0,
                ..super::WeatherSound::default()
            },
        )),
        true,
    )
}
pub(crate) fn render_preview(
    preset: Preset,
    seconds: f32,
    path: &Path,
    seed: u32,
) -> io::Result<()> {
    render(preset, seconds, path, seed, None, false)
}
fn render(
    preset: Preset,
    seconds: f32,
    path: &Path,
    seed: u32,
    scene: Option<(
        std::sync::Arc<super::rain_scene::RainScene>,
        super::WeatherSound,
    )>,
    turn: bool,
) -> io::Result<()> {
    validate_seconds(seconds)?;
    let mut mixer = Mixer::new(seed);
    if let Some((scene, weather)) = scene {
        mixer.command(Command::RainScene(scene));
        mixer.command(Command::Listener {
            position: [0.0, 1.6, 0.0],
            yaw: 0.0,
        });
        mixer.command(Command::Weather(Some(weather)));
    }
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
        if turn {
            mixer.command(Command::Listener {
                position: [0.0, 1.6, 0.0],
                yaw: std::f32::consts::TAU * frames as f32 / (seconds * SAMPLE_RATE as f32),
            });
        }
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
            pitch: 1.0,
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
