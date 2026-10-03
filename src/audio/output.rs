//! Device discovery and mixing stay on a worker. The device callback only
//! consumes a bounded SPSC ring, converts samples, and updates atomic counters.
use super::{Command, Controls, Mixer, Preset, SAMPLE_RATE};
use cpal::traits::StreamTrait;
use rtrb::{Producer, RingBuffer};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[path = "output/device.rs"]
mod device;
#[path = "output/resampler.rs"]
mod resampler;
use device::open_device;
#[cfg(test)]
use device::write_output;
use resampler::{Resampler, Source};

const COMMAND_CAPACITY: usize = 64;
const RING_FRAMES: usize = 4096;
const SOURCE_FRAMES: usize = 256;
const OUTPUT_BATCH: usize = 512;
const SHUTDOWN_WAIT: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OutputStats {
    pub available: bool,
    pub sample_rate: u32,
    pub channels: u16,
    pub underrun_frames: u64,
    pub device_errors: u64,
    pub rejected_commands: u64,
}
struct Shared {
    mix_config: Mutex<super::mix_tuning::MixConfig>,
    mix_revision: AtomicU64,
    rain_config: Mutex<crate::audio::rain_tuning::RainConfig>,
    rain_revision: AtomicU64,
    controls: AtomicU64,
    controls_revision: AtomicU64,
    epoch: AtomicU64,
    shutdown: AtomicBool,
    available: AtomicBool,
    sample_rate: AtomicU32,
    channels: AtomicU16,
    underruns: AtomicU64,
    errors: AtomicU64,
    rejected: AtomicU64,
}
impl Shared {
    fn new(controls: Controls) -> Self {
        Self {
            mix_config: Mutex::new(Default::default()),
            mix_revision: AtomicU64::new(0),
            rain_config: Mutex::new(Default::default()),
            rain_revision: AtomicU64::new(0),
            controls: AtomicU64::new(pack_controls(controls)),
            controls_revision: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            available: AtomicBool::new(false),
            sample_rate: AtomicU32::new(0),
            channels: AtomicU16::new(0),
            underruns: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
        }
    }
}
struct Queued {
    epoch: u64,
    command: Command,
}
#[derive(Clone, Copy)]
struct Frame {
    epoch: u64,
    samples: [f32; 2],
}

pub(crate) struct AudioOutput {
    commands: mpsc::SyncSender<Queued>,
    shared: Arc<Shared>,
    done: mpsc::Receiver<()>,
    worker: Option<JoinHandle<()>>,
}
impl AudioOutput {
    /// Returns immediately, including when there is no device or device setup hangs.
    pub(crate) fn start(controls: Controls) -> Self {
        let shared = Arc::new(Shared::new(controls));
        let (commands, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (complete, done) = mpsc::channel();
        let state = shared.clone();
        let worker = thread::Builder::new()
            .name("audio-output".into())
            .spawn(move || {
                run(receiver, &state);
                state.available.store(false, Ordering::Release);
                let _ = complete.send(());
            });
        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                shared.errors.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(%error, "audio worker unavailable; continuing silently");
                None
            }
        };
        Self {
            commands,
            shared,
            done,
            worker,
        }
    }
    pub(crate) fn try_send(&self, command: Command) -> bool {
        if matches!(command, Command::Reset) {
            self.reset();
            return true;
        }
        let message = Queued {
            epoch: self.shared.epoch.load(Ordering::Acquire),
            command,
        };
        if self.commands.try_send(message).is_ok() {
            true
        } else {
            self.shared.rejected.fetch_add(1, Ordering::Relaxed);
            false
        }
    }
    /// Latest controls replace previous controls without filling the event queue.
    pub(crate) fn set_controls(&self, controls: Controls) {
        self.shared
            .controls
            .store(pack_controls(controls), Ordering::Release);
        self.shared
            .controls_revision
            .fetch_add(1, Ordering::Release);
    }
    pub(crate) fn set_mix_config(&self, config: super::mix_tuning::MixConfig) {
        let config = config.sanitized();
        let mut stored = self.shared.mix_config.lock().expect("mix settings lock");
        if *stored != config {
            *stored = config;
            self.shared.mix_revision.fetch_add(1, Ordering::Release);
        }
    }
    /// Latest tuning is independent of event queue pressure. Only the worker
    /// reads this short-lived lock; the device callback never touches it.
    pub(crate) fn set_rain_config(&self, config: crate::audio::rain_tuning::RainConfig) {
        let config = config.sanitized();
        let mut stored = self.shared.rain_config.lock().expect("rain settings lock");
        if *stored != config {
            *stored = config;
            self.shared.rain_revision.fetch_add(1, Ordering::Release);
        }
    }
    /// Reset is independent of queue capacity. Old commands and buffered PCM
    /// retain their old epoch and cannot cross a reconnect/session boundary.
    pub(crate) fn reset(&self) {
        self.shared
            .controls
            .fetch_and((1u64 << 48) - 1, Ordering::AcqRel);
        self.shared
            .controls_revision
            .fetch_add(1, Ordering::Release);
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
    }
    pub(crate) fn stats(&self) -> OutputStats {
        OutputStats {
            available: self.shared.available.load(Ordering::Acquire),
            sample_rate: self.shared.sample_rate.load(Ordering::Relaxed),
            channels: self.shared.channels.load(Ordering::Relaxed),
            underrun_frames: self.shared.underruns.load(Ordering::Relaxed),
            device_errors: self.shared.errors.load(Ordering::Relaxed),
            rejected_commands: self.shared.rejected.load(Ordering::Relaxed),
        }
    }
}
impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        if self.done.recv_timeout(SHUTDOWN_WAIT).is_ok()
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
        // Dropping an unfinished handle detaches it. A blocked host driver must
        // never block window destruction; the worker still owns all its resources.
    }
}
fn volume(value: f32) -> u64 {
    let value = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (value * f32::from(u16::MAX)).round() as u64
}
fn pack_controls(c: Controls) -> u64 {
    volume(c.master) | volume(c.ambient) << 16 | volume(c.effects) << 32 | (c.preset as u64) << 48
}
fn unpack_controls(bits: u64) -> Controls {
    let gain = |shift| ((bits >> shift) & u64::from(u16::MAX)) as f32 / f32::from(u16::MAX);
    Controls {
        master: gain(0),
        ambient: gain(16),
        effects: gain(32),
        preset: match (bits >> 48) as u8 {
            1 => Preset::Rain,
            2 => Preset::Storm,
            3 => Preset::Wind,
            _ => Preset::Off,
        },
    }
}

fn run(commands: mpsc::Receiver<Queued>, shared: &Arc<Shared>) {
    let (mut producer, consumer) = RingBuffer::new(RING_FRAMES);
    let (stream, rate) = match open_device(consumer, shared.clone()) {
        Ok(output) => output,
        Err(error) => {
            shared.errors.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(%error, "audio output unavailable; continuing silently");
            return;
        }
    };
    let mut source = Source::new();
    let mut resampler = Resampler::new(rate);
    let mut epoch = shared.epoch.load(Ordering::Acquire);
    let mut controls = shared.controls.load(Ordering::Acquire);
    let mut controls_revision = shared.controls_revision.load(Ordering::Acquire);
    source.mixer.set_controls(unpack_controls(controls));
    let mut rain_revision = apply_latest_rain(&mut source, shared);
    let mut mix_revision = apply_latest_mix(&mut source, shared);
    // Prime before starting callbacks so device startup is not itself an underrun.
    fill(
        &mut producer,
        &mut source,
        &mut resampler,
        epoch,
        OUTPUT_BATCH,
    );
    if let Err(error) = stream.play() {
        shared.errors.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(%error, "audio output failed to start; continuing silently");
        return;
    }
    shared.available.store(true, Ordering::Release);
    while !shared.shutdown.load(Ordering::Acquire) && shared.errors.load(Ordering::Relaxed) == 0 {
        let current_epoch = shared.epoch.load(Ordering::Acquire);
        if current_epoch != epoch {
            source.reset();
            resampler.reset();
            epoch = current_epoch;
            (controls, controls_revision) = apply_latest_controls(&mut source, shared);
        }
        let latest_mix = shared.mix_revision.load(Ordering::Acquire);
        if latest_mix != mix_revision {
            mix_revision = apply_latest_mix(&mut source, shared);
        }
        let latest_rain = shared.rain_revision.load(Ordering::Acquire);
        if latest_rain != rain_revision {
            rain_revision = apply_latest_rain(&mut source, shared);
        }
        let latest = shared.controls.load(Ordering::Acquire);
        let latest_revision = shared.controls_revision.load(Ordering::Acquire);
        if latest != controls || latest_revision != controls_revision {
            source.mixer.set_controls(unpack_controls(latest));
            controls = latest;
            controls_revision = latest_revision;
        }
        for _ in 0..COMMAND_CAPACITY {
            let Ok(message) = commands.try_recv() else {
                break;
            };
            // A reset can race this queue drain. Adopt its epoch before
            // deciding whether a newly enqueued command is stale.
            let current_epoch = shared.epoch.load(Ordering::Acquire);
            if current_epoch != epoch {
                source.reset();
                resampler.reset();
                epoch = current_epoch;
                (controls, controls_revision) = apply_latest_controls(&mut source, shared);
            }
            if message.epoch == epoch && !source.mixer.command(message.command) {
                shared.rejected.fetch_add(1, Ordering::Relaxed);
            }
        }
        if producer.is_abandoned() {
            break;
        }
        let queued = RING_FRAMES - producer.slots();
        if queued < 2 * OUTPUT_BATCH {
            fill(
                &mut producer,
                &mut source,
                &mut resampler,
                epoch,
                OUTPUT_BATCH,
            );
        } else {
            thread::sleep(Duration::from_millis(2));
        }
    }
    // Callback errors are atomics only; teardown and diagnostics stay here.
    if shared.errors.load(Ordering::Relaxed) != 0 {
        tracing::warn!("audio device failed; continuing silently");
    }
    drop(stream);
}
/// Used after every reset, even if an identical setter was applied earlier in
/// the queue drain. Reset clears Mixer state; its controls must be restored too.
fn apply_latest_controls(source: &mut Source, shared: &Shared) -> (u64, u64) {
    let controls = shared.controls.load(Ordering::Acquire);
    let revision = shared.controls_revision.load(Ordering::Acquire);
    source.mixer.set_controls(unpack_controls(controls));
    apply_latest_rain(source, shared);
    apply_latest_mix(source, shared);
    (controls, revision)
}
fn apply_latest_mix(source: &mut Source, shared: &Shared) -> u64 {
    let revision = shared.mix_revision.load(Ordering::Acquire);
    let config = *shared.mix_config.lock().expect("mix settings lock");
    source.mixer.set_mix_config(config);
    revision
}
fn apply_latest_rain(source: &mut Source, shared: &Shared) -> u64 {
    let revision = shared.rain_revision.load(Ordering::Acquire);
    let config = *shared.rain_config.lock().expect("rain settings lock");
    source.mixer.set_rain_config(config);
    revision
}
fn fill(
    producer: &mut Producer<Frame>,
    source: &mut Source,
    resampler: &mut Resampler,
    epoch: u64,
    count: usize,
) {
    for _ in 0..count.min(producer.slots()) {
        let _ = producer.push(Frame {
            epoch,
            samples: resampler.next(source),
        });
    }
}
#[cfg(test)]
#[path = "output/tests.rs"]
mod tests;
