//! Bounded player-model selection and authoritative movement configuration.
use super::*;
use bloxgloom_host_api::player_modifiers::Movement;

pub(super) fn write_model(
    out: &mut Vec<u8>,
    value: Option<crate::appearance::PackagedAppearance>,
    catalog: &Catalog,
) -> io::Result<()> {
    out.push(u8::from(value.is_some()));
    if let Some(value) = value {
        if !catalog.valid_appearance_state(crate::appearance::AppearanceState {
            packaged: Some(value),
            ..Default::default()
        }) {
            return Err(invalid("unregistered player model or look"));
        }
        let bytes = value.encode();
        out.extend((bytes.len() as u16).to_le_bytes());
        out.extend(bytes);
    }
    Ok(())
}
pub(super) fn read_model(
    c: &mut Cursor<'_>,
    catalog: &Catalog,
) -> io::Result<Option<crate::appearance::PackagedAppearance>> {
    match c.u8()? {
        0 => Ok(None),
        1 => {
            let len = usize::from(c.u16()?);
            let value = crate::appearance::PackagedAppearance::decode(c.take(len)?)
                .ok_or_else(|| invalid("invalid player model selection"))?;
            if !catalog.valid_appearance_state(crate::appearance::AppearanceState {
                packaged: Some(value),
                ..Default::default()
            }) {
                return Err(invalid("unregistered player model or look"));
            }
            Ok(Some(value))
        }
        _ => Err(invalid("invalid model presence")),
    }
}
fn validate(
    profile: u128,
    session: u64,
    reset: u64,
    position: [f32; 3],
    movement: Movement,
) -> io::Result<()> {
    if profile == 0
        || session == 0
        || reset == 0
        || position
            .iter()
            .any(|v| !v.is_finite() || v.abs() >= 1_000_000.0)
        || movement.validate().is_err()
    {
        return Err(invalid("invalid player modifier configuration"));
    }
    Ok(())
}
pub(super) fn write_modifiers(
    out: &mut Vec<u8>,
    profile: u128,
    session: u64,
    reset: u64,
    position: [f32; 3],
    movement: Movement,
) -> io::Result<()> {
    validate(profile, session, reset, position, movement)?;
    out.extend(profile.to_le_bytes());
    out.extend(session.to_le_bytes());
    out.extend(reset.to_le_bytes());
    for value in position.into_iter().chain([
        movement.speed,
        movement.sprint,
        movement.jump,
        movement.gravity,
    ]) {
        out.extend(value.to_le_bytes());
    }
    Ok(())
}
pub(super) fn read_modifiers(c: &mut Cursor<'_>) -> io::Result<ServerMessage> {
    let profile = c.u128()?;
    let session = c.u64()?;
    let reset = c.u64()?;
    let position = [c.f32()?, c.f32()?, c.f32()?];
    let movement = Movement {
        speed: c.f32()?,
        sprint: c.f32()?,
        jump: c.f32()?,
        gravity: c.f32()?,
    };
    validate(profile, session, reset, position, movement)?;
    Ok(ServerMessage::PlayerModifiers {
        profile,
        session,
        reset,
        position,
        movement,
    })
}
