//! Immutable handles select profile/session lifetime; no script-selected authority.
use super::*;
use crate::server::script::handles;
use bloxgloom_host_api::player_modifiers::{MAX_DURATION_TICKS, Movement};

fn target(value: Value) -> Result<(u128, Option<u64>), Error> {
    if let Value::UserData(ref handle) = value
        && handle.is::<handles::ProfileId>()
    {
        return handles::profile_value(value)
            .map(|profile| (profile, None))
            .map_err(invalid);
    }
    let session = handles::session_value(value).map_err(invalid)?;
    Ok((session.profile, Some(session.epoch)))
}
fn options(value: Value) -> Result<(Movement, Option<u32>), Error> {
    let Value::Table(table) = value else {
        return Err(invalid("modifier options require a plain table"));
    };
    if table.metatable().is_some() {
        return Err(invalid("modifier options require a plain table"));
    }
    for pair in table.clone().pairs::<Value, Value>().take(6) {
        let (key, _) = pair.map_err(|_| invalid("invalid modifier option"))?;
        if !matches!(key,Value::String(key) if matches!(key.as_bytes().as_ref(),b"speed"|b"sprint"|b"jump"|b"gravity"|b"duration_ticks"))
        {
            return Err(invalid("unknown modifier option"));
        }
    }
    let number = |key| -> Result<f32, Error> {
        let value: Value = table
            .raw_get(key)
            .map_err(|_| invalid("invalid modifier multiplier"))?;
        let value = match value {
            Value::Nil => 1.0,
            Value::Integer(value) => value as f64,
            Value::Number(value) => value,
            _ => return Err(invalid("modifier multiplier must be finite")),
        };
        if !value.is_finite() || !(0.1..=4.0).contains(&value) {
            return Err(invalid("modifier multiplier out of bounds"));
        }
        Ok(value as f32)
    };
    let duration: Value = table
        .raw_get("duration_ticks")
        .map_err(|_| invalid("invalid modifier duration"))?;
    let duration = if duration.is_nil() {
        None
    } else {
        Some(integer(duration, 1, i64::from(MAX_DURATION_TICKS)).map_err(invalid)? as u32)
    };
    Ok((
        Movement {
            speed: number("speed")?,
            sprint: number("sprint")?,
            jump: number("jump")?,
            gravity: number("gravity")?,
        },
        duration,
    ))
}
pub(super) fn install<'scope>(
    scope: &'scope mlua::Scope<'scope, '_>,
    host: &mlua::Table,
    context: &'scope RefCell<&mut Context<'_>>,
    rejected: &'scope RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "set_player_modifier",
        scope.create_function(|_, (value, key, opts): (Value, Value, Value)| {
            checked(rejected, || {
                let (profile, session) = target(value)?;
                let key = text(key).map_err(invalid)?;
                let (movement, duration) = options(opts)?;
                context
                    .borrow_mut()
                    .set_player_modifier(profile, session, &key, movement, duration)
            })
        })?,
    )?;
    host.set(
        "remove_player_modifier",
        scope.create_function(|_, (value, key): (Value, Value)| {
            checked(rejected, || {
                let (profile, session) = target(value)?;
                let key = text(key).map_err(invalid)?;
                context
                    .borrow_mut()
                    .remove_player_modifier(profile, session, &key)
            })
        })?,
    )?;
    host.set(
        "player_modifiers",
        scope.create_function(|lua, value: Value| {
            let effects = checked(rejected, || {
                let (profile, session) = target(value)?;
                context.borrow_mut().player_modifiers(profile, session)
            })?;
            (|| {
                let result = lua.create_table()?;
                for (index, effect) in effects.into_iter().enumerate() {
                    let view = lua.create_table()?;
                    view.set("key", effect.key)?;
                    for (key, value) in ["speed", "sprint", "jump", "gravity"].into_iter().zip([
                        effect.movement.speed,
                        effect.movement.sprint,
                        effect.movement.jump,
                        effect.movement.gravity,
                    ]) {
                        view.set(key, value)?;
                    }
                    if let Some(until) = effect.expires_at {
                        view.set("expires_at", handles::tick(lua, until)?)?;
                    }
                    view.set_readonly(true);
                    result.raw_set(index + 1, view)?;
                }
                result.set_readonly(true);
                Ok(result)
            })()
            .inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    Ok(())
}
