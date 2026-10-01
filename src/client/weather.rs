//! Interpolation and bounded local presentation of authoritative weather.
use crate::weather::{Lightning, WeatherSnapshot, WeatherValues};
use glam::Vec3;
use std::time::{Duration, Instant};

const COVER_SIDE: usize = 16;
const COVER_PERIOD: Duration = Duration::from_millis(200);

pub(super) struct State {
    snapshot: Option<WeatherSnapshot>,
    received: Instant,
    last_lightning: u64,
    flash: Option<Lightning>,
    pending_thunder: Vec<Lightning>,
    next_audio: Instant,
    next_cover: Instant,
    origin: [i32; 2],
    cover: [f32; COVER_SIDE * COVER_SIDE],
    exposure: f32,
    target_exposure: f32,
    updated: Instant,
}

impl Default for State {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            snapshot: None,
            received: now,
            last_lightning: 0,
            flash: None,
            pending_thunder: Vec::with_capacity(8),
            next_audio: now,
            next_cover: now,
            origin: [0; 2],
            cover: [f32::INFINITY; COVER_SIDE * COVER_SIDE],
            exposure: 0.0,
            target_exposure: 0.0,
            updated: now,
        }
    }
}

impl State {
    pub(super) fn synchronize(&mut self, snapshot: WeatherSnapshot, now: Instant) {
        if !snapshot.valid()
            || self.snapshot.is_some_and(|old| {
                snapshot.revision < old.revision || snapshot.elapsed_ms < old.elapsed_ms
            })
        {
            return;
        }
        if self.snapshot.is_none() {
            // Joining a storm must not replay a strike that already happened.
            self.last_lightning = snapshot
                .lightning_near(snapshot.elapsed_ms, [0.0; 3])
                .map_or(snapshot.elapsed_ms / 15_000, |strike| strike.id);
        }
        self.snapshot = Some(snapshot);
        self.received = now;
    }

    fn elapsed(&self, now: Instant) -> u64 {
        self.snapshot.map_or(0, |snapshot| {
            snapshot.elapsed_ms.saturating_add(
                now.saturating_duration_since(self.received)
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            )
        })
    }

    fn track_strike(&mut self, elapsed: u64, position: Vec3) {
        if let Some(strike) = self
            .snapshot
            .and_then(|snapshot| snapshot.lightning_near(elapsed, position.to_array()))
            && strike.id > self.last_lightning
        {
            self.last_lightning = strike.id;
            // Only live events are presented; gaps/stalls do not replay old storms.
            if elapsed.saturating_sub(strike.elapsed_ms) < 1_000 {
                self.flash = Some(strike);
                if self.pending_thunder.len() < 8 {
                    self.pending_thunder.push(strike);
                }
            }
        }
    }

    fn sample(&self, now: Instant) -> WeatherValues {
        self.snapshot.map_or_else(
            || crate::weather::WeatherKind::Clear.values(),
            |snapshot| snapshot.sample_at(self.elapsed(now)),
        )
    }
}

/// A short, repeated lightning pulse; elapsed weather time is shared by clients.
fn flash_at(age_ms: u64) -> f32 {
    match age_ms {
        0..=79 => 1.0 - age_ms as f32 / 80.0,
        120..=179 => 0.65 * (1.0 - (age_ms - 120) as f32 / 60.0),
        220..=259 => 0.35 * (1.0 - (age_ms - 220) as f32 / 40.0),
        _ => 0.0,
    }
}

/// Scan only resident server chunks: a missing column is unknown, never clear.
/// The nearby 80-block column is the presentation horizon, not an acoustic ray.
fn column_cover(
    x: i32,
    eye_y: i32,
    z: i32,
    mut block: impl FnMut(i32, i32, i32) -> Option<bool>,
) -> f32 {
    for y in (eye_y.saturating_sub(16)..=eye_y.saturating_add(64)).rev() {
        match block(x, y, z) {
            Some(true) => return y as f32 + 1.0,
            Some(false) => {}
            None => return f32::INFINITY,
        }
    }
    f32::NEG_INFINITY
}

impl super::ClientApp {
    pub(super) fn present_weather(&mut self, camera: crate::render::Camera, now: Instant) {
        if now >= self.weather.next_cover {
            self.weather.next_cover = now + COVER_PERIOD;
            let origin = [
                camera.position.x.floor() as i32 - 8,
                camera.position.z.floor() as i32 - 8,
            ];
            let eye_y = camera.position.y.floor() as i32;
            let mut cover = [f32::INFINITY; COVER_SIDE * COVER_SIDE];
            for z in 0..COVER_SIDE {
                for x in 0..COVER_SIDE {
                    cover[z * COVER_SIDE + x] = column_cover(
                        origin[0] + x as i32,
                        eye_y,
                        origin[1] + z as i32,
                        |x, y, z| {
                            self.block_at(x, y, z)
                                .map(|id| self.catalog.block_flags(id) & crate::content::SOLID != 0)
                        },
                    );
                }
            }
            self.weather.origin = origin;
            self.weather.cover = cover;
            // Listener shelter uses the player's eye, including third-person mode.
            let eye = self.camera().position;
            let roof = column_cover(
                eye.x.floor() as i32,
                eye.y.floor() as i32,
                eye.z.floor() as i32,
                |x, y, z| {
                    self.block_at(x, y, z)
                        .map(|id| self.catalog.block_flags(id) & crate::content::SOLID != 0)
                },
            );
            self.weather.target_exposure = if roof <= eye.y { 1.0 } else { 0.0 };
        }
        let dt = now
            .saturating_duration_since(self.weather.updated)
            .as_secs_f32()
            .min(0.1);
        self.weather.updated = now;
        self.weather.exposure +=
            (self.weather.target_exposure - self.weather.exposure) * (1.0 - (-dt / 0.5).exp());
        let elapsed = self.weather.elapsed(now);
        let sample = self.weather.sample(now);
        self.weather.track_strike(elapsed, self.camera().position);
        let eye = self.camera().position;
        let mut index = 0;
        while index < self.weather.pending_thunder.len() {
            let strike = self.weather.pending_thunder[index];
            let delta = Vec3::from_array(strike.position) - eye;
            let distance = delta.length();
            let arrival = strike
                .elapsed_ms
                .saturating_add((distance / 343.0 * 1_000.0) as u64);
            if elapsed >= arrival {
                if distance <= 15_000.0 && elapsed.saturating_sub(arrival) < 1_000 {
                    self.audio.thunder(
                        distance.max(200.0),
                        delta.z.atan2(delta.x) - self.yaw,
                        self.weather.exposure,
                    );
                }
                self.weather.pending_thunder.swap_remove(index);
            } else {
                index += 1;
            }
        }
        if now >= self.weather.next_audio {
            self.weather.next_audio = now + Duration::from_millis(50);
            self.audio.update_weather(
                sample.rain * 30.0,
                sample.wind,
                0.35 - self.yaw,
                self.weather.exposure,
            );
        }
        let flash = self.weather.flash.map_or(0.0, |strike| {
            let distance = Vec3::from_array(strike.position).distance(eye);
            flash_at(elapsed.saturating_sub(strike.elapsed_ms)) / (1.0 + distance / 2_000.0)
        });
        if let Some(renderer) = &mut self.renderer {
            renderer.set_weather(
                sample.cloud,
                sample.rain,
                [sample.wind * 0.35_f32.cos(), sample.wind * 0.35_f32.sin()],
                self.weather.exposure,
                (elapsed % 3_600_000) as f32 / 1_000.0,
                flash,
            );
            renderer.set_weather_rain_cover(self.weather.origin, self.weather.cover);
        }
    }
}

#[cfg(test)]
#[path = "weather/tests.rs"]
mod tests;
