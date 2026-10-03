//! Bounded owner/session health snapshots. Alive is derived rather than trusted.
use super::*;
use bloxgloom_host_api::player_health::{State, View};
pub(super) fn write(
    out: &mut Vec<u8>,
    profile: u128,
    session: u64,
    health: View,
) -> io::Result<()> {
    if profile == 0
        || session == 0
        || (State {
            current: health.current,
            max: health.max,
            life: health.life,
            respawn: None,
        })
        .validate()
        .is_err()
        || health.alive != (health.current > 0)
    {
        return Err(invalid("invalid player health snapshot"));
    }
    out.extend(profile.to_le_bytes());
    out.extend(session.to_le_bytes());
    out.extend(health.current.to_le_bytes());
    out.extend(health.max.to_le_bytes());
    out.extend(health.revision.to_le_bytes());
    out.extend(health.life.to_le_bytes());
    Ok(())
}
pub(super) fn read(c: &mut Cursor<'_>) -> io::Result<ServerMessage> {
    let profile = c.u128()?;
    let session = c.u64()?;
    let current = c.u32()?;
    let max = c.u32()?;
    let revision = c.u64()?;
    let life = c.u64()?;
    let state = State {
        current,
        max,
        life,
        respawn: None,
    };
    if profile == 0 || session == 0 || state.validate().is_err() {
        return Err(invalid("invalid player health snapshot"));
    }
    Ok(ServerMessage::PlayerHealth {
        profile,
        session,
        health: View::new(state, revision),
    })
}
