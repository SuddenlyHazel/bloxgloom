//! Exact admitted-player roster for command discovery. Names never identify saves.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSummary {
    pub profile: u128,
    pub session: u64,
    pub name: String,
}
pub(super) fn write(out: &mut Vec<u8>, players: &[PlayerSummary]) -> io::Result<()> {
    if players.len() > 256 {
        return Err(invalid("player roster exceeds admission bound"));
    }
    out.extend((players.len() as u16).to_le_bytes());
    let mut last = 0;
    for p in players {
        if p.profile <= last || p.session == 0 {
            return Err(invalid("noncanonical player roster"));
        }
        out.extend(p.profile.to_le_bytes());
        out.extend(p.session.to_le_bytes());
        short_string(out, &p.name)?;
        last = p.profile;
    }
    Ok(())
}
pub(super) fn read(c: &mut Cursor<'_>) -> io::Result<Vec<PlayerSummary>> {
    let count = usize::from(c.u16()?);
    if count > 256 {
        return Err(invalid("player roster exceeds admission bound"));
    }
    let mut players = Vec::with_capacity(count);
    let mut last = 0;
    for _ in 0..count {
        let profile = c.u128()?;
        let session = c.u64()?;
        let name = c.string()?;
        if profile <= last || session == 0 {
            return Err(invalid("noncanonical player roster"));
        }
        last = profile;
        players.push(PlayerSummary {
            profile,
            session,
            name,
        });
    }
    Ok(players)
}

pub(super) fn validate_notice(profile: u128, session: u64, text: &str) -> io::Result<()> {
    if profile == 0
        || session == 0
        || text.is_empty()
        || text.len() > 255
        || text.chars().any(char::is_control)
    {
        return Err(invalid("invalid player notice"));
    }
    Ok(())
}
