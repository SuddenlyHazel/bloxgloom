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
}
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
    fn send_voice(&self, command: Command) -> bool {
        #[cfg(test)]
        {
            if self.blocked.get() {
                return false;
            }
            self.sent.borrow_mut().push(command);
            true
        }
        #[cfg(not(test))]
        {
            self.output
                .as_ref()
                .is_some_and(|output| output.try_send(command))
        }
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
                    if let Some(voice) = self.voices.active.remove(&key) {
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
                        if self.send_voice(Command::Update {
                            id: v.id,
                            position: Some(at),
                            gain,
                            pitch,
                        }) {
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
                        if let Some(v) = self.voices.active.remove(&key) {
                            self.voices.stopping.insert(v.id);
                        }
                    }
                    if self.voices.active.len() + self.voices.stopping.len() >= 32 {
                        continue;
                    }
                    let Some(next) = self.next_voice.checked_add(1) else {
                        continue;
                    };
                    let id = self.next_voice;
                    self.next_voice = next;
                    let clip = Arc::clone(&self.voices.clips[&clip]);
                    let expires = (!looping).then(|| {
                        now + Duration::from_secs_f32(clip.duration_seconds() / pitch + 0.1)
                    });
                    if self.send_voice(Command::Play {
                        id,
                        clip,
                        position: Some(position),
                        gain,
                        pitch,
                        looping,
                    }) {
                        self.voices.active.insert(
                            key,
                            Voice {
                                id,
                                entity,
                                position,
                                gain,
                                pitch,
                                expires,
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
            if self.send_voice(Command::Stop(id)) {
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
            {
                self.voices.stopping.insert(v.id);
            }
        }
        for (key, at, id, gain, pitch) in moves {
            if self.send_voice(Command::Update {
                id,
                position: Some(at),
                gain,
                pitch,
            }) {
                self.voices.active.get_mut(&key).unwrap().position = at;
            }
        }
        self.flush_stops();
    }
}
