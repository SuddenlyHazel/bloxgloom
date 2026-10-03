//! Transactional appearance and clip selection. Calls only mutate the candidate
//! plan; a caught invalid call poisons the whole callback just like world reads.
use super::*;
use api::{ClipPlayback, Tint, TintMode, VisualSchema, VisualState};
use std::cell::{Cell, RefCell};

pub(super) fn with<T>(
    lua: &Lua,
    host: &mlua::Table,
    schema: Option<&VisualSchema>,
    initial: Option<VisualState>,
    tick: u64,
    run: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<(T, Option<VisualState>)> {
    let (Some(schema), Some(mut initial)) = (schema, initial) else {
        return run().map(|result| (result, None));
    };
    initial.sample_tick = tick;
    let state = RefCell::new(initial);
    let calls = Cell::new(0);
    let rejected = Cell::new(false);
    let guarded = |action: &mut dyn FnMut() -> mlua::Result<()>| {
        if rejected.get() || calls.get() >= 16 {
            rejected.set(true);
            return Err(invalid("creature visual call limit exceeded"));
        }
        calls.set(calls.get() + 1);
        let result = action();
        if result.is_err() {
            rejected.set(true)
        }
        result
    };
    lua.scope(|scope| {
        host.set(
            "play_animation",
            scope.create_function(|_, value: Value| {
                guarded(&mut || {
                    let table = plain_table(&value)?;
                    fields(
                        &table,
                        &["clip", "speed", "looping", "crossfade_ms", "restart"],
                    )?;
                    let name = text(table.raw_get("clip")?)?;
                    let clip = schema
                        .clips
                        .iter()
                        .position(|s| s == &name)
                        .ok_or_else(|| invalid("unknown creature animation clip"))?;
                    let speed = optional_number(&table, "speed", 1.0, 0.0, 8.0)?;
                    let looping = optional_bool(&table, "looping", schema.clip_loops[clip])?;
                    let crossfade_s =
                        optional_number(&table, "crossfade_ms", 200.0, 0.0, 5000.0)? / 1000.0;
                    let restart = optional_bool(&table, "restart", false)?;
                    let mut state = state.borrow_mut();
                    if !restart
                        && state.playback.is_some_and(|p| {
                            p.clip as usize == clip && p.speed == speed && p.looping == looping
                        })
                    {
                        return Ok(());
                    }
                    state.sequence = state
                        .sequence
                        .checked_add(1)
                        .ok_or_else(|| invalid("creature animation sequence exhausted"))?;
                    let sequence = state.sequence;
                    state.playback = Some(ClipPlayback {
                        clip: clip as u16,
                        speed,
                        looping,
                        crossfade_s,
                        started_tick: tick,
                        sequence,
                    });
                    state.transition_s = crossfade_s;
                    Ok(())
                })
            })?,
        )?;
        host.set(
            "stop_animation",
            scope.create_function(|_, value: Value| {
                guarded(&mut || {
                    let crossfade_s = if value.is_nil() {
                        0.2
                    } else {
                        number(value.clone(), 0.0, 5000.0)? / 1000.0
                    };
                    let mut state = state.borrow_mut();
                    state.playback = None;
                    state.transition_s = crossfade_s;
                    Ok(())
                })
            })?,
        )?;
        host.set(
            "set_variant",
            scope.create_function(|_, (group, option): (Value, Value)| {
                guarded(&mut || {
                    let group = text(group.clone())?;
                    let option = text(option.clone())?;
                    let i = schema
                        .variants
                        .iter()
                        .position(|g| g.0 == group)
                        .ok_or_else(|| invalid("unknown creature variant group"))?;
                    let choice = schema.variants[i]
                        .1
                        .iter()
                        .position(|o| o == &option)
                        .ok_or_else(|| invalid("unknown creature variant option"))?;
                    state.borrow_mut().variants[i] = choice as u8;
                    Ok(())
                })
            })?,
        )?;
        host.set(
            "set_layer",
            scope.create_function(|_, (name, visible): (Value, Value)| {
                guarded(&mut || {
                    let name = text(name.clone())?;
                    let Value::Boolean(visible) = visible else {
                        return Err(invalid("creature layer visibility must be boolean"));
                    };
                    let i = schema
                        .layers
                        .iter()
                        .position(|n| n == &name)
                        .ok_or_else(|| invalid("unknown creature layer"))?;
                    state.borrow_mut().layers[i] = i8::from(visible);
                    Ok(())
                })
            })?,
        )?;
        host.set(
            "set_tint",
            scope.create_function(|_, (name, value): (Value, Value)| {
                guarded(&mut || {
                    let name = text(name.clone())?;
                    let i = schema
                        .tints
                        .iter()
                        .position(|n| n == &name)
                        .ok_or_else(|| invalid("unknown creature tint"))?;
                    let table = plain_table(&value)?;
                    fields(&table, &["rgb", "mode"])?;
                    let rgb: Value = table.raw_get("rgb")?;
                    let rgb = plain_table(&rgb)?;
                    if rgb.raw_len() != 3 || rgb.clone().pairs::<Value, Value>().count() != 3 {
                        return Err(invalid("creature tint RGB needs three bytes"));
                    }
                    let mut color = [0; 3];
                    for (axis, c) in color.iter_mut().enumerate() {
                        *c = super::super::values::integer(rgb.raw_get(axis + 1)?, 0, 255)
                            .map_err(invalid)? as u8;
                    }
                    let mode = match table.raw_get::<Value>("mode")? {
                        Value::Nil => TintMode::Multiply,
                        value => match text(value)?.as_str() {
                            "multiply" => TintMode::Multiply,
                            "replace" => TintMode::Replace,
                            _ => return Err(invalid("unknown creature tint mode")),
                        },
                    };
                    state.borrow_mut().tints[i] = Some(Tint { rgb: color, mode });
                    Ok(())
                })
            })?,
        )?;
        let summary = lua.create_table()?;
        if let Some(p) = initial.playback {
            summary.set("clip", schema.clips[p.clip as usize].as_str())?;
            summary.set("speed", p.speed)?;
            summary.set("looping", p.looping)?;
        }
        summary.set_readonly(true);
        host.set("visual", summary)?;
        let result = run()?;
        if rejected.get() {
            return Err(invalid("creature visual call failed"));
        }
        let visual = *state.borrow();
        if !schema.accepts(&visual) {
            return Err(invalid("invalid creature visual state"));
        }
        Ok((result, Some(visual)))
    })
}
fn plain_table(value: &Value) -> mlua::Result<mlua::Table> {
    match value {
        Value::Table(t) if t.metatable().is_none() => Ok(t.clone()),
        _ => Err(invalid("creature visual options must be a plain table")),
    }
}
fn fields(table: &mlua::Table, allowed: &[&str]) -> mlua::Result<()> {
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        let key = text(key)?;
        if !allowed.contains(&key.as_str()) {
            return Err(invalid("unknown creature visual option"));
        }
    }
    Ok(())
}
fn text(value: Value) -> mlua::Result<String> {
    if let Value::String(s) = value {
        let s = s.to_str()?;
        if !s.is_empty() && s.len() <= 96 {
            return Ok(s.to_owned());
        }
    }
    Err(invalid("creature visual name required"))
}
fn optional_number(
    t: &mlua::Table,
    key: &str,
    default: f32,
    min: f32,
    max: f32,
) -> mlua::Result<f32> {
    match t.raw_get::<Value>(key)? {
        Value::Nil => Ok(default),
        v => number(v, min, max),
    }
}
fn optional_bool(t: &mlua::Table, key: &str, default: bool) -> mlua::Result<bool> {
    match t.raw_get::<Value>(key)? {
        Value::Nil => Ok(default),
        Value::Boolean(v) => Ok(v),
        _ => Err(invalid("creature animation option must be boolean")),
    }
}
fn number(v: Value, min: f32, max: f32) -> mlua::Result<f32> {
    let n = match v {
        Value::Integer(v) => v as f64,
        Value::Number(v) => v,
        _ => return Err(invalid("creature animation number required")),
    };
    if n.is_finite() && n >= f64::from(min) && n <= f64::from(max) {
        Ok(n as f32)
    } else {
        Err(invalid("creature animation number out of bounds"))
    }
}
