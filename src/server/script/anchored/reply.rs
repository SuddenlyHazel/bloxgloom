//! Decode callback replies inside their VM; only bounded native values escape.
use super::*;
pub(super) enum Reply {
    Bytes(Vec<u8>),
    Valid,
    Reaction(api::Reaction),
    Refund(u16),
}
fn invalid() -> mlua::Error {
    mlua::Error::RuntimeError("invalid anchored callback reply".into())
}
fn bytes(value: Value, limit: usize) -> mlua::Result<Vec<u8>> {
    let Value::String(value) = value else {
        return Err(invalid());
    };
    if value.as_bytes().len() > limit {
        return Err(invalid());
    }
    Ok(value.as_bytes().to_vec())
}
pub(super) fn parse(
    value: Value,
    event: &Event<'_>,
    state_limit: usize,
    public_limit: usize,
) -> mlua::Result<Reply> {
    match event.kind {
        "Initialize" | "Interact" => Ok(Reply::Bytes(bytes(value, state_limit)?)),
        "Public" => Ok(Reply::Bytes(bytes(value, public_limit)?)),
        "Validate" if matches!(value, Value::Nil | Value::Boolean(true)) => Ok(Reply::Valid),
        "React" if value.is_nil() => Ok(Reply::Reaction(api::Reaction::Keep)),
        "React" => {
            let Value::Table(t) = value else {
                return Err(invalid());
            };
            if t.metatable().is_some() {
                return Err(invalid());
            }
            let mut pairs = t.pairs::<Value, Value>();
            let (key, value) = pairs.next().ok_or_else(invalid)??;
            if pairs.next().is_some() {
                return Err(invalid());
            }
            let Value::String(key) = key else {
                return Err(invalid());
            };
            match key.as_bytes().as_ref() {
                b"state" => Ok(Reply::Reaction(api::Reaction::Update(Payload::new(bytes(
                    value,
                    state_limit,
                )?)))),
                b"remove" if matches!(value, Value::Boolean(true)) => {
                    Ok(Reply::Reaction(api::Reaction::Remove))
                }
                _ => Err(invalid()),
            }
        }
        "Refund" => {
            let maximum = event.maximum.ok_or_else(invalid)?;
            let count = if value.is_nil() {
                maximum
            } else {
                super::super::values::integer(value, 0, i64::from(maximum))
                    .map_err(|_| invalid())? as u16
            };
            Ok(Reply::Refund(count))
        }
        _ => Err(invalid()),
    }
}
