//! Player rig controls use the same frozen baked clip and appearance contracts
//! as creature models. Every setter stages an epoch-fenced player operation.
use super::*;
use crate::server::script::handles;
use bloxgloom_host_api::entity::{Tint, TintMode};
pub(super) fn install<'scope>(
    scope: &'scope mlua::Scope<'scope, '_>,
    host: &mlua::Table,
    context: &'scope RefCell<&mut Context<'_>>,
    rejected: &'scope RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "set_player_model",
        scope.create_function(|_, (session, key): (Value, Value)| {
            checked(rejected, || {
                let session = handles::session_value(session).map_err(invalid)?;
                let key = if key.is_nil() {
                    None
                } else {
                    Some(crate::server::script::values::text(key).map_err(invalid)?)
                };
                context.borrow_mut().set_player_model(
                    session.profile,
                    session.epoch,
                    key.as_deref(),
                )
            })
        })?,
    )?;
    host.set(
        "play_player_animation",
        scope.create_function(|_, (session, value): (Value, Value)| {
            checked(rejected, || {
                let session = handles::session_value(session).map_err(invalid)?;
                let table = plain(value)?;
                fields(&table, &["clip", "speed", "looping", "crossfade_ms"])?;
                let clip =
                    crate::server::script::values::text(table.raw_get("clip").map_err(lua_error)?)
                        .map_err(invalid)?;
                let speed = optional_number(&table, "speed", 1.0, 0.0, 8.0)?;
                let crossfade =
                    optional_number(&table, "crossfade_ms", 200.0, 0.0, 5000.0)? / 1000.0;
                let mut context = context.borrow_mut();
                let (key, _) = context
                    .player_model(session.profile, session.epoch)?
                    .ok_or_else(|| invalid("player has no packaged model"))?;
                let schema = context.player_model_schema(&key)?;
                let index = schema
                    .clips
                    .iter()
                    .position(|c| c == &clip)
                    .ok_or_else(|| invalid("unknown player clip"))?;
                let looping = match table.raw_get::<Value>("looping").map_err(lua_error)? {
                    Value::Nil => schema.clip_loops[index],
                    Value::Boolean(v) => v,
                    _ => return Err(invalid("looping must be boolean")),
                };
                context.play_player_animation(
                    session.profile,
                    session.epoch,
                    &clip,
                    speed,
                    looping,
                    crossfade,
                )
            })
        })?,
    )?;
    host.set(
        "stop_player_animation",
        scope.create_function(|_, (session, value): (Value, Value)| {
            checked(rejected, || {
                let session = handles::session_value(session).map_err(invalid)?;
                let value = match value {
                    Value::Nil => 0.2,
                    Value::Integer(v) => v as f32 / 1000.0,
                    Value::Number(v) => v as f32 / 1000.0,
                    _ => return Err(invalid("crossfade requires milliseconds or nil")),
                };
                context
                    .borrow_mut()
                    .stop_player_animation(session.profile, session.epoch, value)
            })
        })?,
    )?;
    host.set(
        "set_player_model_variant",
        scope.create_function(|_, (session, group, option): (Value, Value, Value)| {
            checked(rejected, || {
                let session = handles::session_value(session).map_err(invalid)?;
                let group = crate::server::script::values::text(group).map_err(invalid)?;
                let option = if option.is_nil() {
                    None
                } else {
                    Some(crate::server::script::values::text(option).map_err(invalid)?)
                };
                let mut context = context.borrow_mut();
                let (key, mut state) = context
                    .player_model(session.profile, session.epoch)?
                    .ok_or_else(|| invalid("player has no packaged model"))?;
                let schema = context.player_model_schema(&key)?;
                let group = schema
                    .variants
                    .iter()
                    .position(|(name, _)| name == &group)
                    .ok_or_else(|| invalid("unknown player variant group"))?;
                state.variants[group] = match option {
                    None => 255,
                    Some(option) => schema.variants[group]
                        .1
                        .iter()
                        .position(|s| s == &option)
                        .ok_or_else(|| invalid("unknown player variant option"))?
                        as u8,
                };
                context.set_player_model_visual(session.profile, session.epoch, state)
            })
        })?,
    )?;
    host.set(
        "set_player_model_layer",
        scope.create_function(|_, (session, name, visible): (Value, Value, Value)| {
            checked(rejected, || {
                let session = handles::session_value(session).map_err(invalid)?;
                let name = crate::server::script::values::text(name).map_err(invalid)?;
                let visible = match visible {
                    Value::Nil => -1,
                    Value::Boolean(v) => i8::from(v),
                    _ => return Err(invalid("layer visibility must be boolean or nil")),
                };
                let mut context = context.borrow_mut();
                let (key, mut state) = context
                    .player_model(session.profile, session.epoch)?
                    .ok_or_else(|| invalid("player has no packaged model"))?;
                let schema = context.player_model_schema(&key)?;
                let index = schema
                    .layers
                    .iter()
                    .position(|s| s == &name)
                    .ok_or_else(|| invalid("unknown player layer"))?;
                state.layers[index] = visible;
                context.set_player_model_visual(session.profile, session.epoch, state)
            })
        })?,
    )?;
    host.set(
        "set_player_model_tint",
        scope.create_function(|_, (session, name, value): (Value, Value, Value)| {
            checked(rejected, || {
                let session = handles::session_value(session).map_err(invalid)?;
                let name = crate::server::script::values::text(name).map_err(invalid)?;
                let tint = if value.is_nil() {
                    None
                } else {
                    let table = plain(value)?;
                    fields(&table, &["rgb", "mode"])?;
                    let rgb = plain(table.raw_get("rgb").map_err(lua_error)?)?;
                    if rgb.raw_len() != 3
                        || rgb.clone().pairs::<Value, Value>().take(4).count() != 3
                    {
                        return Err(invalid("tint RGB requires three bytes"));
                    }
                    let mut color = [0; 3];
                    for (i, c) in color.iter_mut().enumerate() {
                        *c = integer(rgb.raw_get(i + 1).map_err(lua_error)?, 0, 255)
                            .map_err(invalid)? as u8;
                    }
                    let mode = match table.raw_get::<Value>("mode").map_err(lua_error)? {
                        Value::Nil => TintMode::Multiply,
                        v => match crate::server::script::values::text(v)
                            .map_err(invalid)?
                            .as_str()
                        {
                            "multiply" => TintMode::Multiply,
                            "replace" => TintMode::Replace,
                            _ => return Err(invalid("unknown player tint mode")),
                        },
                    };
                    Some(Tint { rgb: color, mode })
                };
                let mut context = context.borrow_mut();
                let (key, mut state) = context
                    .player_model(session.profile, session.epoch)?
                    .ok_or_else(|| invalid("player has no packaged model"))?;
                let schema = context.player_model_schema(&key)?;
                let index = schema
                    .tints
                    .iter()
                    .position(|s| s == &name)
                    .ok_or_else(|| invalid("unknown player tint"))?;
                state.tints[index] = tint;
                context.set_player_model_visual(session.profile, session.epoch, state)
            })
        })?,
    )?;
    Ok(())
}
fn lua_error(error: mlua::Error) -> Error {
    invalid(&error.to_string())
}
fn plain(value: Value) -> Result<mlua::Table, Error> {
    match value {
        Value::Table(table) if table.metatable().is_none() => Ok(table),
        _ => Err(invalid("player model options must be plain tables")),
    }
}
fn fields(table: &mlua::Table, names: &[&str]) -> Result<(), Error> {
    for pair in table.clone().pairs::<Value, Value>().take(names.len() + 1) {
        let (name, _) = pair.map_err(lua_error)?;
        let name = crate::server::script::values::text(name).map_err(invalid)?;
        if !names.contains(&name.as_str()) {
            return Err(invalid("unknown player model option"));
        }
    }
    Ok(())
}
fn optional_number(
    table: &mlua::Table,
    name: &str,
    default: f32,
    min: f32,
    max: f32,
) -> Result<f32, Error> {
    let value = match table.raw_get::<Value>(name).map_err(lua_error)? {
        Value::Nil => return Ok(default),
        Value::Integer(v) => v as f64,
        Value::Number(v) => v,
        _ => return Err(invalid("expected numeric player clip option")),
    };
    if !value.is_finite() || value < f64::from(min) || value > f64::from(max) {
        return Err(invalid("player clip option out of range"));
    }
    Ok(value as f32)
}
