//! Frozen center-origin moving-body declarations and their semantic handlers.
use super::*;
use crate::server::script::values::integer;
use bloxgloom_host_api::{
    entity::{Cuboid, PartMotion},
    gameplay::{EventKind, HandlerRegistration},
    motion::{self, Body, CollisionMask, MovingEntity, Response},
};

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, value: Value| {
        let mut pending = pending.borrow_mut();
        let result = parse(value, &pending, &namespace, &snapshot).and_then(|result| {
            let (declaration, handlers) = &result;
            pending
                .content_budget
                .reserve(
                    1 + handlers.len(),
                    2048 + declaration.model.len() * 256 + handlers.len() * 1024,
                    &declaration.key,
                )
                .map_err(|_| "moving declaration memory budget exceeded")?;
            Ok(result)
        });
        match result {
            Ok((declaration, handlers)) => {
                pending.moving.push(declaration);
                pending.handlers.extend(handlers);
                Ok(())
            }
            Err(error) => {
                pending.error.get_or_insert(error);
                Err(mlua::Error::RuntimeError(error.into()))
            }
        }
    })
}

fn parse(
    value: Value,
    pending: &Pending,
    namespace: &str,
    snapshot: &Arc<PackageSnapshot>,
) -> Result<(MovingEntity, Vec<HandlerRegistration>), &'static str> {
    if let Some(error) = pending.error {
        return Err(error);
    }
    if !snapshot.permits_moving(namespace) {
        return Err(
            "register_moving_entity requires content/v1, actions/v1 and moving_entities/v1",
        );
    }
    if pending.moving.len() >= 8 {
        return Err("moving declaration limit exceeded (8)");
    }
    let d = table(value)?;
    let key = text(field(&d, "key")?)?;
    owned(&key, namespace)?;
    if pending.moving.iter().any(|old| old.key == key)
        || pending.creatures.iter().any(|old| old.key == key)
        || pending.entities.iter().any(|old| old.key == key)
        || pending.anchored.iter().any(|old| old.entity == key)
    {
        return Err("duplicate moving entity identity");
    }
    let module = text(field(&d, "module")?)?;
    owned(&module, namespace)?;
    if snapshot.source(&module).is_none() {
        return Err("moving behavior module must be a declared package source");
    }
    let schema = integer(field(&d, "schema")?, 1, u16::MAX.into())? as u16;
    let revision = integer(field(&d, "revision")?, 1, u16::MAX.into())? as u16;
    let state_bytes = integer(field(&d, "max_state_bytes")?, 1, 4096)? as u16;
    let public_bytes = integer(
        field(&d, "max_public_bytes")?,
        0,
        i64::from(state_bytes.min(4000)),
    )? as u16;
    let interval = integer(field(&d, "interval")?, 1, 1000)? as u32;
    let lifetime_ticks = integer(
        field(&d, "lifetime_ticks")?,
        1,
        i64::from(motion::MAX_LIFETIME_TICKS),
    )? as u32;
    let source_exclusion_ticks = match field(&d, "source_exclusion_ticks")? {
        Value::Nil => 0,
        value => integer(value, 0, i64::from(motion::MAX_SOURCE_EXCLUSION_TICKS))? as u32,
    };
    let handles_impact = boolean(field(&d, "handles_impact")?, false)?;
    let handles_expiry = boolean(field(&d, "handles_expiry")?, false)?;
    let b = table(field(&d, "body")?)?;
    match field(&b, "origin")? {
        Value::Nil => (),
        Value::String(value) if value.as_bytes().as_ref() == b"center" => (),
        _ => return Err("moving body origin must be center"),
    }
    let mask = match field(&b, "collisions")? {
        Value::Nil => CollisionMask {
            terrain: true,
            players: false,
            creatures: false,
        },
        value => {
            let m = table(value)?;
            CollisionMask {
                terrain: boolean(field(&m, "terrain")?, true)?,
                players: boolean(field(&m, "players")?, false)?,
                creatures: boolean(field(&m, "creatures")?, false)?,
            }
        }
    };
    let response = match field(&b, "response")? {
        Value::Nil => Response::Stop,
        Value::String(value) => match value
            .to_str()
            .map_err(|_| "invalid moving response")?
            .as_ref()
        {
            "stop" => Response::Stop,
            "bounce" => Response::Bounce,
            "slide" => Response::Slide,
            _ => return Err("moving response must be stop, bounce or slide"),
        },
        _ => return Err("moving response must be stop, bounce or slide"),
    };
    let body = Body {
        half_extents: triple(field(&b, "half_extents")?, 0.025, 1.5)?,
        collisions: mask,
        response,
        restitution: optional_number(field(&b, "restitution")?, 0.0, 1.0, 0.0)?,
        gravity_scale: optional_number(field(&b, "gravity_scale")?, 0.0, 4.0, 1.0)?,
        max_speed: number(field(&b, "max_speed")?, 0.0, motion::MAX_SPEED)?,
        max_acceleration: number(
            field(&b, "max_acceleration")?,
            0.0,
            motion::MAX_ACCELERATION,
        )?,
    };
    let declaration = MovingEntity {
        key: key.clone(),
        schema_version: schema,
        schema_fingerprint: snapshot.creature_schema(&module, schema, revision),
        max_state_bytes: state_bytes,
        max_public_bytes: public_bytes,
        body,
        physics: physics(field(&d, "physics")?)?,
        lifetime_ticks,
        interval,
        source_exclusion_ticks,
        handles_impact,
        handles_expiry,
        model: model(field(&d, "model")?)?,
        state: Arc::new(crate::server::script::entities::FixedBytes {
            state_bytes,
            public_bytes,
        }),
    };
    declaration
        .validate()
        .map_err(|_| "invalid moving entity declaration")?;
    let mut handlers = Vec::new();
    for (event, suffix, enabled) in [
        (EventKind::MovingTick, ".motion_tick", true),
        (EventKind::MovingImpact, ".motion_impact", handles_impact),
        (EventKind::MovingExpiry, ".motion_expiry", handles_expiry),
    ] {
        if !enabled {
            continue;
        }
        let handler = HandlerRegistration {
            key: format!("{key}{suffix}"),
            version: snapshot.gameplay_version(&module, revision),
            event,
            target: Some(key.clone()),
            handler: Arc::new(crate::server::script::gameplay::ScriptHandler {
                snapshot: Arc::clone(snapshot),
                module: module.clone(),
                command: None,
            }),
        };
        handler
            .validate()
            .map_err(|_| "invalid moving handler key")?;
        if pending.handlers.iter().any(|old| {
            old.key == handler.key || (old.event == handler.event && old.target == handler.target)
        }) {
            return Err("duplicate moving handler identity or decision owner");
        }
        handlers.push(handler);
    }
    if pending.handlers.len() + handlers.len() > 32 {
        return Err("gameplay handler limit exceeded (32 per package)");
    }
    Ok((declaration, handlers))
}

fn model(value: Value) -> Result<Vec<Cuboid>, &'static str> {
    if value.is_nil() {
        return Ok(Vec::new());
    }
    let parts = table(value)?;
    let count = parts.raw_len();
    if !(1..=16).contains(&count) {
        return Err("moving model must have 1..=16 parts");
    }
    dense(&parts, count)?;
    let mut result = Vec::with_capacity(count);
    for index in 1..=count {
        let part = table(parts.raw_get(index).map_err(|_| "invalid moving part")?)?;
        match field(&part, "motion")? {
            Value::Nil => (),
            Value::String(value) if value.as_bytes().as_ref() == b"body" => (),
            _ => return Err("moving model parts use body motion"),
        }
        result.push(Cuboid {
            min: triple(field(&part, "min")?, -4.0, 4.0)?,
            max: triple(field(&part, "max")?, -4.0, 4.0)?,
            color: triple(field(&part, "color")?, 0.0, 1.0)?,
            motion: PartMotion::Body,
        });
    }
    Ok(result)
}
fn owned(key: &str, namespace: &str) -> Result<(), &'static str> {
    if key.split_once(':').is_none_or(|(owner, local)| {
        owner != namespace || !super::super::package::manifest::identifier(local)
    }) {
        Err("moving identity must belong to the declaring package")
    } else {
        Ok(())
    }
}
fn table(value: Value) -> Result<mlua::Table, &'static str> {
    match value {
        Value::Table(table) if table.metatable().is_none() => Ok(table),
        _ => Err("moving declarations require plain tables"),
    }
}
fn field(table: &mlua::Table, key: &str) -> Result<Value, &'static str> {
    table.raw_get(key).map_err(|_| "invalid moving field")
}
fn boolean(value: Value, default: bool) -> Result<bool, &'static str> {
    match value {
        Value::Nil => Ok(default),
        Value::Boolean(value) => Ok(value),
        _ => Err("moving boolean required"),
    }
}
fn number(value: Value, min: f32, max: f32) -> Result<f32, &'static str> {
    let value = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => return Err("moving number required"),
    };
    // Bounds are native f32 values. Compare after the documented conversion so
    // a decimal boundary such as 0.025 does not fall below its rounded f32 bound.
    let converted = value as f32;
    if !value.is_finite() || !converted.is_finite() || converted < min || converted > max {
        return Err("moving number out of bounds");
    }
    Ok(converted)
}
fn physics(value: Value) -> Result<Option<motion::Physics>, &'static str> {
    if value.is_nil() {
        return Ok(None);
    }
    let table = table(value)?;
    for pair in table.clone().pairs::<Value, Value>().take(5) {
        let (Value::String(key), _) = pair.map_err(|_| "invalid physics option")? else {
            return Err("invalid physics option");
        };
        if !matches!(
            key.to_str().map_err(|_| "invalid physics option")?.as_ref(),
            "linear_damping" | "angular_damping" | "friction" | "max_angular_speed"
        ) {
            return Err("unknown physics option");
        }
    }
    Ok(Some(motion::Physics {
        linear_damping: optional_number(field(&table, "linear_damping")?, 0.0, 32.0, 0.0)?,
        angular_damping: optional_number(field(&table, "angular_damping")?, 0.0, 32.0, 0.0)?,
        friction: optional_number(field(&table, "friction")?, 0.0, 4.0, 0.5)?,
        max_angular_speed: optional_number(
            field(&table, "max_angular_speed")?,
            0.0,
            motion::MAX_ANGULAR_SPEED,
            motion::MAX_ANGULAR_SPEED,
        )?,
    }))
}
fn optional_number(value: Value, min: f32, max: f32, default: f32) -> Result<f32, &'static str> {
    if value.is_nil() {
        Ok(default)
    } else {
        number(value, min, max)
    }
}
fn dense(table: &mlua::Table, count: usize) -> Result<(), &'static str> {
    let mut seen = 0;
    for pair in table.clone().pairs::<Value, Value>().take(count + 1) {
        let (index, _) = pair.map_err(|_| "invalid moving sequence")?;
        if !matches!(index, Value::Integer(i) if i > 0 && i as usize <= count) {
            return Err("moving sequence must be dense");
        }
        seen += 1;
    }
    if seen != count {
        return Err("moving sequence must be dense");
    }
    Ok(())
}
fn triple(value: Value, min: f32, max: f32) -> Result<[f32; 3], &'static str> {
    let table = table(value)?;
    if table.raw_len() != 3 {
        return Err("moving vector must have three values");
    }
    dense(&table, 3)?;
    Ok([
        number(
            table.raw_get(1).map_err(|_| "invalid moving vector")?,
            min,
            max,
        )?,
        number(
            table.raw_get(2).map_err(|_| "invalid moving vector")?,
            min,
            max,
        )?,
        number(
            table.raw_get(3).map_err(|_| "invalid moving vector")?,
            min,
            max,
        )?,
    ])
}

#[cfg(test)]
mod tests;
