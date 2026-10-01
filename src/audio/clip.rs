//! Bounded WAV decode/preparation; callers load on workers or before the event loop.
use std::{
    io,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
static DECODED_BYTES: AtomicUsize = AtomicUsize::new(0);
const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;
#[derive(Debug)]
struct Reservation(usize);
impl Reservation {
    fn new(bytes: usize) -> io::Result<Self> {
        DECODED_BYTES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|total| *total <= MAX_DECODED_BYTES)
            })
            .map_err(|_| io::Error::other("decoded audio exceeds64MiB process budget"))?;
        Ok(Self(bytes))
    }
    fn shrink(&mut self, bytes: usize) {
        DECODED_BYTES.fetch_sub(self.0 - bytes, Ordering::AcqRel);
        self.0 = bytes;
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        DECODED_BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}
pub(crate) const MAX_CLIP_FRAMES: usize = 1_323_000;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
#[derive(Debug)]
pub(crate) struct Clip {
    pub(super) frames: Vec<[f32; 2]>,
    pub(super) rate: u32,
    _reservation: Reservation,
}
impl Clip {
    pub(crate) fn duration_seconds(&self) -> f32 {
        self.frames.len() as f32 / self.rate as f32
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "audio file exceeds16MiB",
            ));
        }
        let reader = hound::WavReader::open(path).map_err(io::Error::other)?;
        Self::decode_reader(reader)
    }
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(io::Error::other("audio file exceeds 16 MiB"));
        }
        let reader = hound::WavReader::new(io::Cursor::new(bytes)).map_err(io::Error::other)?;
        Self::decode_reader(reader)
    }
    fn decode_reader<R: io::Read>(mut reader: hound::WavReader<R>) -> io::Result<Self> {
        let spec = reader.spec();
        if !(1..=2).contains(&spec.channels)
            || !(8_000..=192_000).contains(&spec.sample_rate)
            || reader.duration() == 0
            || reader.duration() as usize > MAX_CLIP_FRAMES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported WAV channels/rate/duration",
            ));
        }
        // Account for decoded frames and the temporary interleaved samples before allocation.
        let mut reservation = Reservation::new(reader.duration() as usize * 16)?;
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float if spec.bits_per_sample == 32 => reader
                .samples::<f32>()
                .map(|x| x.map_err(io::Error::other))
                .collect::<io::Result<_>>()?,
            hound::SampleFormat::Int if matches!(spec.bits_per_sample, 8 | 16 | 24 | 32) => {
                let scale = 2.0f32.powi(i32::from(spec.bits_per_sample) - 1);
                reader
                    .samples::<i32>()
                    .map(|x| x.map(|x| x as f32 / scale).map_err(io::Error::other))
                    .collect::<io::Result<_>>()?
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported WAV encoding",
                ));
            }
        };
        if samples.len() != reader.duration() as usize * usize::from(spec.channels)
            || samples.iter().any(|x| !x.is_finite() || x.abs() > 1.0)
            || !samples.len().is_multiple_of(usize::from(spec.channels))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid WAV samples",
            ));
        }
        let frames: Vec<[f32; 2]> = samples
            .chunks_exact(usize::from(spec.channels))
            .map(|s| [s[0], *s.get(1).unwrap_or(&s[0])])
            .collect();
        let frames_bytes = frames.len() * 8;
        drop(samples);
        reservation.shrink(frames_bytes);
        Ok(Self {
            frames,
            rate: spec.sample_rate,
            _reservation: reservation,
        })
    }
    pub fn click() -> Self {
        let frames = (0..2205)
            .map(|n| {
                let t = n as f32 / 44_100.0;
                let envelope = (-100.0 * t).exp() * (n as f32 / 64.0).min(1.0);
                let sample = 0.15 * envelope * (std::f32::consts::TAU * 700.0 * t).sin();
                [sample; 2]
            })
            .collect();
        Self {
            frames,
            rate: 44_100,
            _reservation: Reservation(0),
        }
    }
}
