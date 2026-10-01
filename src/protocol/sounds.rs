//! Small validated sound batch, separate from authoritative world snapshots.
use super::{Cursor, invalid};
use bloxgloom_host_api::sound::{Event, Kind};
use std::io;
pub(super) fn len(events: &[Event]) -> usize {
    9 + events
        .iter()
        .map(|e| {
            3 + e.owner.len()
                + e.voice.len()
                + match &e.kind {
                    Kind::Stop => 0,
                    Kind::Update { position, .. } => 9 + position.map_or(0, |_| 12),
                    Kind::Play { clip, entity, .. } => 23 + clip.len() + entity.map_or(0, |_| 8),
                }
        })
        .sum::<usize>()
}
fn string(out: &mut Vec<u8>, text: &str) {
    out.push(text.len() as u8);
    out.extend(text.as_bytes());
}
fn read_string(c: &mut Cursor<'_>, max: usize) -> io::Result<String> {
    let len = usize::from(c.u8()?);
    if len > max {
        return Err(invalid("sound string too long"));
    }
    String::from_utf8(c.take(len)?.to_vec()).map_err(|_| invalid("invalid sound UTF-8"))
}
fn vector(out: &mut Vec<u8>, p: &[f32; 3]) {
    for v in p {
        out.extend(v.to_le_bytes());
    }
}
fn read_vector(c: &mut Cursor<'_>) -> io::Result<[f32; 3]> {
    Ok([c.f32()?, c.f32()?, c.f32()?])
}
fn flag(c: &mut Cursor<'_>) -> io::Result<bool> {
    match c.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid sound flag")),
    }
}
pub(super) fn write(out: &mut Vec<u8>, id: u64, events: &[Event]) -> io::Result<()> {
    if id == 0 || events.is_empty() || events.len() > 32 || events.iter().any(|e| !e.validate()) {
        return Err(invalid("invalid sound batch"));
    }
    out.extend(id.to_le_bytes());
    out.push(events.len() as u8);
    for e in events {
        string(out, &e.owner);
        string(out, &e.voice);
        match &e.kind {
            Kind::Stop => out.push(2),
            Kind::Update {
                position,
                gain,
                pitch,
            } => {
                out.push(1);
                out.push(u8::from(position.is_some()));
                if let Some(p) = position {
                    vector(out, p);
                }
                out.extend(gain.to_le_bytes());
                out.extend(pitch.to_le_bytes());
            }
            Kind::Play {
                clip,
                position,
                entity,
                gain,
                pitch,
                looping,
            } => {
                out.push(0);
                string(out, clip);
                vector(out, position);
                out.push(u8::from(entity.is_some()));
                if let Some(id) = entity {
                    out.extend(id.to_le_bytes());
                }
                out.extend(gain.to_le_bytes());
                out.extend(pitch.to_le_bytes());
                out.push(u8::from(*looping));
            }
        }
    }
    Ok(())
}
pub(super) fn read(c: &mut Cursor<'_>) -> io::Result<(u64, Vec<Event>)> {
    let id = c.u64()?;
    let count = usize::from(c.u8()?);
    if id == 0 || count == 0 || count > 32 {
        return Err(invalid("invalid sound batch"));
    }
    let mut events = Vec::with_capacity(count);
    for _ in 0..count {
        let owner = read_string(c, 64)?;
        let voice = read_string(c, 64)?;
        let kind = match c.u8()? {
            2 => Kind::Stop,
            1 => {
                let position = if flag(c)? {
                    Some(read_vector(c)?)
                } else {
                    None
                };
                Kind::Update {
                    position,
                    gain: c.f32()?,
                    pitch: c.f32()?,
                }
            }
            0 => {
                let clip = read_string(c, 129)?;
                let position = read_vector(c)?;
                let entity = if flag(c)? { Some(c.u64()?) } else { None };
                Kind::Play {
                    clip,
                    position,
                    entity,
                    gain: c.f32()?,
                    pitch: c.f32()?,
                    looping: flag(c)?,
                }
            }
            _ => return Err(invalid("invalid sound operation")),
        };
        let event = Event { owner, voice, kind };
        if !event.validate() {
            return Err(invalid("invalid sound event"));
        }
        events.push(event);
    }
    Ok((id, events))
}
