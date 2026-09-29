//! Bounded presentation and authoritative lifecycle options for startup items.
use crate::server::script::values::integer;
use bloxgloom_host_api::content::{Components, DropAnimation, DropPolicy, DropSize};
use mlua::Value;

pub(super) fn options(
    options: Value,
) -> Result<(bool, DropSize, DropAnimation, DropPolicy, Components), &'static str> {
    let table = match options {
        Value::Nil => {
            return Ok((
                true,
                DropSize::Normal,
                DropAnimation::default(),
                DropPolicy::default(),
                Components::None,
            ));
        }
        Value::Table(table) => table,
        _ => return Err("item options must be a table"),
    };
    let mut sprite = true;
    let mut size = DropSize::Normal;
    let mut animation = DropAnimation::default();
    let mut policy = DropPolicy::default();
    let mut components = Components::None;
    for (index, pair) in table.pairs::<Value, Value>().enumerate() {
        if index >= 5 {
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
            b"drop_policy" => policy = drop_policy(value)?,
            b"components" => components = component_schema(value)?,
            _ => return Err("unknown item option"),
        }
    }
    Ok((sprite, size, animation, policy, components))
}

fn component_schema(value: Value) -> Result<Components, &'static str> {
    let Value::Table(table) = value else {
        return Err("item components must be a table");
    };
    if table.metatable().is_some() {
        return Err("component schema metatable is unsupported");
    }
    for (index, pair) in table.clone().pairs::<Value, Value>().enumerate() {
        if index >= 5 {
            return Err("too many component schema fields");
        }
        let (key, _) = pair.map_err(|_| "invalid component schema")?;
        let Value::String(key) = key else {
            return Err("invalid component schema field");
        };
        if ![
            b"version".as_slice(),
            b"fingerprint_lo",
            b"fingerprint_hi",
            b"max_bytes",
            b"required",
        ]
        .contains(&key.as_bytes().as_ref())
        {
            return Err("unknown component schema field");
        }
    }
    let get = |name| {
        table
            .raw_get::<Value>(name)
            .map_err(|_| "invalid component schema")
    };
    let version = integer(get("version")?, 1, u16::MAX.into())? as u16;
    let lo = integer(get("fingerprint_lo")?, 0, u32::MAX.into())? as u64;
    let hi = integer(get("fingerprint_hi")?, 0, u32::MAX.into())? as u64;
    let max_bytes = integer(get("max_bytes")?, 1, 1024)? as u16;
    let required = match get("required")? {
        Value::Nil => false,
        Value::Boolean(value) => value,
        _ => return Err("component required must be boolean"),
    };
    if lo == 0 && hi == 0 {
        return Err("component fingerprint must be nonzero");
    }
    Ok(Components::Opaque {
        version,
        fingerprint: lo | (hi << 32),
        max_bytes,
        required,
    })
}

fn drop_policy(value: Value) -> Result<DropPolicy, &'static str> {
    let Value::Table(fields) = value else {
        return Err("item drop_policy must be a table");
    };
    let mut policy = DropPolicy::default();
    for (index, field) in fields.pairs::<Value, Value>().enumerate() {
        if index >= 6 {
            return Err("too many drop_policy fields");
        }
        let (key, value) = field.map_err(|_| "invalid drop_policy field")?;
        let Value::String(key) = key else {
            return Err("invalid drop_policy field");
        };
        let value = match value {
            Value::Number(value) => value,
            Value::Integer(value) => value as f64,
            _ => return Err("drop_policy fields require numeric values"),
        };
        match key.as_bytes().as_ref() {
            b"gravity" => policy.gravity = value as f32,
            b"terminal_speed" => policy.terminal_speed = value as f32,
            b"radius" => policy.radius = value as f32,
            b"pickup_range" => policy.pickup_range = value as f32,
            b"merge_range" => policy.merge_range = value as f32,
            b"lifetime_ms" => {
                if !value.is_finite()
                    || !(1_000.0..=86_400_000.0).contains(&value)
                    || value.fract() != 0.0
                {
                    return Err("invalid drop_policy lifetime_ms");
                }
                policy.lifetime_ms = value as u64;
            }
            _ => return Err("unknown drop_policy field"),
        }
    }
    if !policy.valid() {
        return Err("invalid drop_policy range");
    }
    Ok(policy)
}
