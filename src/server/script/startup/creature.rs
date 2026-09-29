//! Bounded mobile creature declarations. Script source stays server-side; the
//! bundle carries only model, body and schema identity for presentation.
use super::*;
use crate::server::script::values::integer;
use bloxgloom_host_api::entity::{Animation, Body, Cuboid, MobileEntity, PartMotion};

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, declaration: Value| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !snapshot.permits_creatures(&namespace) {
                return Err("register_creature requires content and mobile_entities/v1");
            }
            if pending.creatures.len() >= 8 {
                return Err("creature declaration limit exceeded (8)");
            }
            let Value::Table(declaration) = declaration else {
                return Err("creature declaration must be a table");
            };
            let key = text(field(&declaration, "key")?)?;
            if key.split_once(':').is_none_or(|(owner, local)| {
                owner != namespace || !super::super::package::manifest::identifier(local)
            }) || pending.creatures.iter().any(|old| old.key == key)
                || pending.entities.iter().any(|old| old.key == key)
            {
                return Err("invalid or duplicate creature key");
            }
            let module = text(field(&declaration, "module")?)?;
            if module.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                || snapshot.source(&module).is_none()
            {
                return Err("creature module must be a declared package source");
            }
            let schema = integer(field(&declaration, "schema")?, 1, u16::MAX.into())? as u16;
            let revision = integer(field(&declaration, "revision")?, 1, u16::MAX.into())? as u16;
            let max_private = integer(field(&declaration, "max_state_bytes")?, 1, 256)? as usize;
            let initial = match field(&declaration, "initial_state")? {
                Value::Nil => Vec::new(),
                Value::String(value) if value.as_bytes().len() <= max_private => {
                    value.as_bytes().to_vec()
                }
                _ => return Err("invalid creature initial state"),
            };
            let interval = integer(field(&declaration, "interval")?, 1, 1000)? as u32;
            let read_radius = match field(&declaration, "read_radius")? {
                Value::Nil => 0,
                value => integer(value, 0, 1)? as u8,
            };
            let reads_neighbours = match field(&declaration, "reads_neighbours")? {
                Value::Nil | Value::Boolean(false) => false,
                Value::Boolean(true) => true,
                _ => return Err("creature reads_neighbours must be boolean"),
            };
            let wakes_on_terrain_change = match field(&declaration, "wakes_on_terrain_change")? {
                Value::Nil | Value::Boolean(true) => true,
                Value::Boolean(false) => false,
                _ => return Err("creature wakes_on_terrain_change must be boolean"),
            };
            let Value::Table(body) = field(&declaration, "body")? else {
                return Err("creature body must be a table");
            };
            let body = Body {
                half_width: number(field(&body, "half_width")?, 0.05, 1.0)?,
                height: number(field(&body, "height")?, 0.1, 3.0)?,
                speed: number(field(&body, "speed")?, 0.0, 4.0)?,
            };
            let Value::Table(parts) = field(&declaration, "model")? else {
                return Err("creature model must be a sequence");
            };
            let count = parts.raw_len();
            if count == 0 || count > 16 {
                return Err("creature model must have 1..=16 parts");
            }
            let mut seen = 0;
            for pair in parts.clone().pairs::<Value, Value>().take(count + 1) {
                let (index, _) = pair.map_err(|_| "invalid creature model")?;
                seen += 1;
                if !matches!(index, Value::Integer(i) if i > 0 && i as usize <= count) {
                    return Err("creature model must be dense");
                }
            }
            if seen != count {
                return Err("creature model must be dense");
            }
            let mut model = Vec::with_capacity(count);
            for index in 1..=count {
                let part: mlua::Table =
                    parts.raw_get(index).map_err(|_| "invalid creature part")?;
                let motion = match field(&part, "motion")? {
                    Value::Nil => PartMotion::Body,
                    Value::String(s) if s.as_bytes().as_ref() == b"body" => PartMotion::Body,
                    Value::String(s) if s.as_bytes().as_ref() == b"left_foot" => {
                        PartMotion::LeftFoot
                    }
                    Value::String(s) if s.as_bytes().as_ref() == b"right_foot" => {
                        PartMotion::RightFoot
                    }
                    _ => return Err("invalid creature part motion"),
                };
                model.push(Cuboid {
                    min: triple(field(&part, "min")?, -4.0, 4.0)?,
                    max: triple(field(&part, "max")?, -4.0, 4.0)?,
                    color: triple(field(&part, "color")?, 0.0, 1.0)?,
                    motion,
                });
            }
            let animation = match field(&declaration, "animation")? {
                Value::Nil => Animation::default(),
                Value::Table(table) if table.metatable().is_none() => {
                    let default = Animation::default();
                    let option = |key, default, max| match field(&table, key)? {
                        Value::Nil => Ok(default),
                        value => number(value, 0.0, max),
                    };
                    Animation {
                        stride_rate: option("stride_rate", default.stride_rate, 40.0)?,
                        stride_amplitude: option(
                            "stride_amplitude",
                            default.stride_amplitude,
                            3.0,
                        )?,
                        idle_rate: option("idle_rate", default.idle_rate, 20.0)?,
                        idle_bob: option("idle_bob", default.idle_bob, 0.1)?,
                        walk_bob: option("walk_bob", default.walk_bob, 0.2)?,
                        fall_stretch: option("fall_stretch", default.fall_stretch, 0.5)?,
                        landing_squash: option("landing_squash", default.landing_squash, 0.5)?,
                    }
                }
                _ => return Err("invalid creature animation"),
            };
            let interaction = match field(&declaration, "interaction")? {
                Value::Nil => Vec::new(),
                Value::String(request) if (1..=128).contains(&request.as_bytes().len()) => {
                    request.as_bytes().to_vec()
                }
                _ => return Err("invalid creature interaction request"),
            };
            let creature = MobileEntity {
                key: key.clone(),
                schema_version: schema,
                schema_fingerprint: snapshot.creature_schema(&module, schema, revision),
                max_state_bytes: 12 + max_private,
                max_public_bytes: 5,
                body,
                interval,
                read_radius,
                reads_neighbours,
                wakes_on_terrain_change,
                model,
                animation,
                interaction,
                behavior: Arc::new(crate::server::script::creature::ScriptCreature::server(
                    Arc::clone(&snapshot),
                    key.clone(),
                    module,
                    initial,
                    max_private,
                )),
            };
            creature
                .validate()
                .map_err(|_| "invalid creature declaration")?;
            pending.creatures.push(creature);
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}

fn field(table: &mlua::Table, key: &str) -> Result<Value, &'static str> {
    table.raw_get(key).map_err(|_| "invalid creature field")
}

fn number(value: Value, min: f32, max: f32) -> Result<f32, &'static str> {
    let number = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => return Err("creature number required"),
    };
    if !number.is_finite() || number < f64::from(min) || number > f64::from(max) {
        return Err("creature number out of bounds");
    }
    Ok(number as f32)
}

fn triple(value: Value, min: f32, max: f32) -> Result<[f32; 3], &'static str> {
    let Value::Table(table) = value else {
        return Err("creature vector must be a sequence");
    };
    if table.raw_len() != 3 || table.pairs::<Value, Value>().take(4).count() != 3 {
        return Err("creature vector must have three values");
    }
    Ok([
        number(
            table.raw_get(1).map_err(|_| "invalid creature vector")?,
            min,
            max,
        )?,
        number(
            table.raw_get(2).map_err(|_| "invalid creature vector")?,
            min,
            max,
        )?,
        number(
            table.raw_get(3).map_err(|_| "invalid creature vector")?,
            min,
            max,
        )?,
    ])
}
