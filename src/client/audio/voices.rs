//! Session-scoped voice handles. Stops retry if the native command queue is full.
use super::*;
use bloxgloom_host_api::sound::{Event, Kind};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
struct Voice {
    id: u64,
    entity: Option<u64>,
    position: [f32; 3],
    gain: f32,
    pitch: f32,
    expires: Option<Instant>,
    pending: Option<Arc<crate::audio::Clip>>,
    looping: bool,
    bus: bloxgloom_host_api::sound::Bus,
    deadline: Instant,
}

#[cfg(test)]
mod tests;
#[derive(Default)]
pub(super) struct Voices {
    clips: Arc<crate::audio::sounds::Clips>,
    active: BTreeMap<(bool, String, String), Voice>,
    stopping: BTreeSet<u64>,
    last_server_batch: u64,
    last_follow: Option<Instant>,
}
impl State {
    pub(in crate::client) fn install_sounds(&mut self, clips: Arc<crate::audio::sounds::Clips>) {
        self.voices.clips = clips;
    }
    pub(in crate::client) fn sounds(
        &mut self,
        server: bool,
        batch: Option<u64>,
        events: Vec<Event>,
        now: Instant,
    ) {
        // Reliable publication is receipt-ordered. A high-water mark suppresses
        // every earlier batch for the entire session, including gaps from culling.
        if batch.is_some_and(|id| id <= self.voices.last_server_batch) {
            return;
        }
        // Validate the complete batch before sending any native command.
        if events.len() > 32 || events.iter().any(|e| {
            !e.validate()
                || matches!(&e.kind,Kind::Play{clip,..} if !self.voices.clips.contains_key(clip))
        }) {
            return;
        }
        if let Some(id) = batch {
            self.voices.last_server_batch = id;
        }
        for event in events {
            let key = (server, event.owner, event.voice);
            match event.kind {
                Kind::Stop => {
                    if let Some(voice) = self.voices.active.remove(&key)
                        && voice.pending.is_none()
                    {
                        self.voices.stopping.insert(voice.id);
                    }
                }
                Kind::Update {
                    position,
                    gain,
                    pitch,
                } => {
                    if let Some(v) = self.voices.active.get(&key) {
                        let at = position.unwrap_or(v.position);
                        if v.pending.is_some()
                            || self.send_command(Command::Update {
                                id: v.id,
                                position: Some(at),
                                gain,
                                pitch,
                            })
                        {
                            let v = self.voices.active.get_mut(&key).unwrap();
                            if let Some(expiry) = v.expires {
                                v.expires = Some(
                                    now + Duration::from_secs_f32(
                                        expiry.saturating_duration_since(now).as_secs_f32()
                                            * v.pitch
                                            / pitch
                                            + 0.02,
                                    ),
                                );
                            }
                            v.position = at;
                            v.gain = gain;
                            v.pitch = pitch;
                        }
                    }
                }
                Kind::Play {
                    bus,
                    clip,
                    position,
                    entity,
                    gain,
                    pitch,
                    looping,
                } => {
                    if self.voices.active.contains_key(&key) {
                        // Repeated replica loop assertions are idempotent; replacement requires stop.
                        if looping {
                            continue;
                        }
                        if let Some(v) = self.voices.active.remove(&key)
                            && v.pending.is_none()
                        {
                            self.voices.stopping.insert(v.id);
                        }
                    }
                    if self.voices.active.len() + self.voices.stopping.len()
                        >= crate::audio::MAX_CLIP_VOICES
                    {
                        continue;
                    }
                    let Some(next) = self.next_voice.checked_add(1) else {
                        continue;
                    };
                    let id = self.next_voice;
                    self.next_voice = next;
                    let clip = Arc::clone(&self.voices.clips[&clip]);
                    let waiting = self.obstruction.enabled();
                    let expires = (!waiting && !looping).then(|| {
                        now + Duration::from_secs_f32(clip.duration_seconds() / pitch + 0.1)
                    });
                    if waiting
                        || self.send_command(Command::Play {
                            bus,
                            id,
                            clip: clip.clone(),
                            position: Some(position),
                            gain,
                            pitch,
                            looping,
                        })
                    {
                        self.voices.active.insert(
                            key,
                            Voice {
                                id,
                                entity,
                                position,
                                gain,
                                pitch,
                                expires,
                                pending: waiting.then_some(clip),
                                looping,
                                bus,
                                deadline: now + Duration::from_millis(100),
                            },
                        );
                    }
                }
            }
        }
        self.flush_stops();
    }
    fn flush_stops(&mut self) {
        let mut done = Vec::new();
        for &id in &self.voices.stopping {
            if self.send_command(Command::Stop(id)) {
                done.push(id);
            }
        }
        for id in done {
            self.voices.stopping.remove(&id);
        }
    }
    pub(in crate::client) fn follow_sounds(
        &mut self,
        now: Instant,
        mut position: impl FnMut(u64) -> Option<[f32; 3]>,
    ) {
        if self
            .voices
            .last_follow
            .is_some_and(|last| now.saturating_duration_since(last) < Duration::from_millis(50))
        {
            return;
        }
        self.voices.last_follow = Some(now);
        let mut remove = Vec::new();
        let mut moves = Vec::new();
        for (key, v) in &self.voices.active {
            if v.expires.is_some_and(|expiry| now >= expiry) {
                remove.push((key.clone(), false));
                continue;
            }
            if let Some(id) = v.entity {
                match position(id) {
                    None => remove.push((key.clone(), true)),
                    Some(at) if at != v.position => {
                        moves.push((key.clone(), at, v.id, v.gain, v.pitch))
                    }
                    _ => {}
                }
            }
        }
        for (key, stop) in remove {
            if let Some(v) = self.voices.active.remove(&key)
                && stop
                && v.pending.is_none()
            {
                self.voices.stopping.insert(v.id);
            }
        }
        for (key, at, id, gain, pitch) in moves {
            if self.voices.active[&key].pending.is_some()
                || self.send_command(Command::Update {
                    id,
                    position: Some(at),
                    gain,
                    pitch,
                })
            {
                self.voices.active.get_mut(&key).unwrap().position = at;
            }
        }
        self.flush_stops();
    }

    pub(super) fn obstruction_sources(&self) -> Vec<super::obstruction::Source> {
        self.voices
            .active
            .values()
            .map(|voice| super::obstruction::Source {
                id: voice.id,
                position: voice.position,
            })
            .collect()
    }

    pub(super) fn apply_obstruction(
        &mut self,
        id: u64,
        position: [f32; 3],
        gain: f32,
        lowpass_hz: f32,
        now: Instant,
    ) -> bool {
        let Some((key, voice)) = self.voices.active.iter().find(|(_, voice)| voice.id == id) else {
            return false;
        };
        if !super::obstruction_state::position_matches(voice.position, position) {
            return false;
        }
        let key = key.clone();
        if let Some(clip) = &voice.pending {
            let expires = (!voice.looping).then(|| {
                now + Duration::from_secs_f32(clip.duration_seconds() / voice.pitch + 0.1)
            });
            if !self.send_command(Command::PlayObstructed {
                bus: voice.bus,
                clip: clip.clone(),
                position: voice.position,
                gain: voice.gain,
                pitch: voice.pitch,
                looping: voice.looping,
                id,
                transmission: gain,
                lowpass_hz,
            }) {
                return false;
            }
            let voice = self.voices.active.get_mut(&key).unwrap();
            voice.pending = None;
            voice.expires = expires;
            true
        } else {
            self.send_command(Command::Obstruction {
                id,
                gain,
                lowpass_hz,
            })
        }
    }

    pub(super) fn start_overdue_obstruction(&mut self, now: Instant) {
        let overdue: Vec<_> = self
            .voices
            .active
            .values()
            .filter(|voice| voice.pending.is_some() && now >= voice.deadline)
            .map(|voice| (voice.id, voice.position))
            .collect();
        for (id, position) in overdue {
            // Missing/slow geometry never produces an initially unfiltered
            // one-shot. A subsequent worker result can smoothly reopen it.
            self.apply_obstruction(id, position, 0.35, 2400.0, now);
        }
    }
}
