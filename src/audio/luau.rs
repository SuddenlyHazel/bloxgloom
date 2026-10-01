//! Shared bounded table decoder for server host calls and client presentation.
use bloxgloom_host_api::sound::Kind;
use mlua::{Table, Value};
fn text(table: &Table, key: &str, max: usize) -> mlua::Result<String> {
    let Value::String(value) = table.raw_get(key)? else {
        return Err(mlua::Error::external("expected sound string"));
    };
    if value.as_bytes().len() > max {
        return Err(mlua::Error::external("sound string too long"));
    }
    Ok(value.to_str()?.to_owned())
}
fn number(table: &Table, key: &str, default: f32) -> mlua::Result<f32> {
    Ok(match table.raw_get::<Value>(key)? {
        Value::Nil => default,
        Value::Integer(n) => n as f32,
        Value::Number(n) => n as f32,
        _ => return Err(mlua::Error::external("expected sound number")),
    })
}
fn axis(value: Value) -> mlua::Result<f32> {
    match value {
        Value::Integer(n) => Ok(n as f32),
        Value::Number(n) => Ok(n as f32),
        _ => Err(mlua::Error::external("expected numeric sound coordinate")),
    }
}
fn position(value: Value) -> mlua::Result<Option<[f32; 3]>> {
    match value {
        Value::Nil => Ok(None),
        Value::Table(t) => Ok(Some([
            axis(t.raw_get(1)?)?,
            axis(t.raw_get(2)?)?,
            axis(t.raw_get(3)?)?,
        ])),
        _ => Err(mlua::Error::external("expected sound position")),
    }
}
pub(crate) fn decode(table: &Table) -> mlua::Result<(String, Kind)> {
    let voice = text(table, "voice", 64)?;
    let kind = match text(table, "kind", 8)?.as_str() {
        "stop" => Kind::Stop,
        "update" => Kind::Update {
            position: position(table.raw_get("position")?)?,
            gain: number(table, "gain", 1.0)?,
            pitch: number(table, "pitch", 1.0)?,
        },
        "play" => {
            let entity = match table.raw_get::<Value>("entity")? {
                Value::Nil => None,
                value => Some(
                    crate::server::script_handles::entity_value(value)
                        .map_err(mlua::Error::external)?,
                ),
            };
            let position = position(table.raw_get("position")?)?
                .or(entity.map(|_| [0.0; 3]))
                .ok_or_else(|| mlua::Error::external("sound requires position or entity"))?;
            let looping = match table.raw_get::<Value>("looping")? {
                Value::Nil => false,
                Value::Boolean(v) => v,
                _ => return Err(mlua::Error::external("expected looping boolean")),
            };
            Kind::Play {
                clip: text(table, "clip", 129)?,
                position,
                entity,
                gain: number(table, "gain", 1.0)?,
                pitch: number(table, "pitch", 1.0)?,
                looping,
            }
        }
        _ => return Err(mlua::Error::external("unknown sound kind")),
    };
    Ok((voice, kind))
}
