//! Explicit public projections for one exact local profile/session.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerState {
    pub key: String,
    /// Zero denotes an uninitialized profile cell.
    pub revision: u64,
    pub public: Vec<u8>,
}

fn valid_key(key: &str) -> bool {
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
    };
    key.len() <= 128
        && key
            .split_once(':')
            .is_some_and(|(namespace, local)| valid(namespace) && valid(local))
}

pub(super) fn write(out: &mut Vec<u8>, states: &[PlayerState]) -> io::Result<()> {
    if states.len() > 128 {
        return Err(invalid("too many public player states"));
    }
    out.extend((states.len() as u16).to_le_bytes());
    let mut previous = "";
    for state in states {
        if state.key.as_str() <= previous || !valid_key(&state.key) || state.public.len() > 1024 {
            return Err(invalid("invalid public player state"));
        }
        out.push(state.key.len() as u8);
        out.extend(state.key.as_bytes());
        out.extend(state.revision.to_le_bytes());
        out.extend((state.public.len() as u16).to_le_bytes());
        out.extend(&state.public);
        previous = &state.key;
    }
    Ok(())
}
pub(super) fn read(c: &mut Cursor<'_>) -> io::Result<Vec<PlayerState>> {
    let count = usize::from(c.u16()?);
    if count > 128 {
        return Err(invalid("too many public player states"));
    }
    let mut states = Vec::with_capacity(count);
    let mut previous = String::new();
    for _ in 0..count {
        let key_len = usize::from(c.u8()?);
        if key_len > 128 {
            return Err(invalid("player service key too long"));
        }
        let key = String::from_utf8(c.take(key_len)?.to_vec())
            .map_err(|_| invalid("invalid player service UTF-8"))?;
        let revision = c.u64()?;
        let len = usize::from(c.u16()?);
        if key <= previous || !valid_key(&key) || len > 1024 {
            return Err(invalid("invalid public player state"));
        }
        let public = c.take(len)?.to_vec();
        previous = key.clone();
        states.push(PlayerState {
            key,
            revision,
            public,
        });
    }
    Ok(states)
}
