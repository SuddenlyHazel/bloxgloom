//! Bounded presentation options for non-placeable startup items.
use bloxgloom_host_api::content::{DropAnimation, DropSize};
use mlua::Value;

pub(super) fn options(options: Value) -> Result<(bool, DropSize, DropAnimation), &'static str> {
    let table = match options {
        Value::Nil => return Ok((true, DropSize::Normal, DropAnimation::default())),
        Value::Table(table) => table,
        _ => return Err("item options must be a table"),
    };
    let mut sprite = true;
    let mut size = DropSize::Normal;
    let mut animation = DropAnimation::default();
    for (index, pair) in table.pairs::<Value, Value>().enumerate() {
        if index >= 3 {
            return Err("too many item options");
        }
        let (key, value) = pair.map_err(|_| "invalid item option")?;
        let Value::String(key) = key else {
            return Err("invalid item option key");
        };
        match key.as_bytes().as_ref() {
            b"sprite" => {
                let Value::Boolean(value) = value else {
                    return Err("item sprite option must be boolean");
                };
                sprite = value;
            }
            b"drop_size" => {
                let Value::String(value) = value else {
                    return Err("item drop_size option must be small, normal or large");
                };
                size = match value.as_bytes().as_ref() {
                    b"small" => DropSize::Small,
                    b"normal" => DropSize::Normal,
                    b"large" => DropSize::Large,
                    _ => return Err("item drop_size option must be small, normal or large"),
                };
            }
            b"drop_animation" => {
                let Value::Table(fields) = value else {
                    return Err("item drop_animation must be a table");
                };
                for (index, field) in fields.pairs::<Value, Value>().enumerate() {
                    if index >= 8 {
                        return Err("too many drop_animation fields");
                    }
                    let (key, value) = field.map_err(|_| "invalid drop_animation field")?;
                    let Value::String(key) = key else {
                        return Err("drop_animation fields require numeric values");
                    };
                    let value = match value {
                        Value::Number(value) => value,
                        Value::Integer(value) => value as f64,
                        _ => return Err("drop_animation fields require numeric values"),
                    };
                    let target = match key.as_bytes().as_ref() {
                        b"pop_duration" => &mut animation.pop_duration,
                        b"pop_height" => &mut animation.pop_height,
                        b"hover_amplitude" => &mut animation.hover_amplitude,
                        b"hover_speed" => &mut animation.hover_speed,
                        b"spin_speed" => &mut animation.spin_speed,
                        b"pickup_duration" => &mut animation.pickup_duration,
                        b"pickup_arc" => &mut animation.pickup_arc,
                        b"pickup_turn" => &mut animation.pickup_turn,
                        _ => return Err("unknown drop_animation field"),
                    };
                    // Reject values that round out of bounds when stored as f32.
                    *target = value as f32;
                }
                if !animation.valid() {
                    return Err("invalid drop_animation range");
                }
            }
            _ => return Err("unknown item option"),
        }
    }
    Ok((sprite, size, animation))
}
