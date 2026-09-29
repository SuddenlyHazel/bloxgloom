//! Decode one typed presentation value without unbounded table traversal.
pub(crate) fn decode(value: mlua::Value) -> mlua::Result<crate::render::parameters::Value> {
    use crate::render::parameters::Value;
    let invalid = || mlua::Error::RuntimeError("invalid visual parameter value".into());
    let scalar = |value: mlua::Value| {
        let number = match value {
            mlua::Value::Integer(value) => value as f32,
            mlua::Value::Number(value) => value as f32,
            _ => return Err(invalid()),
        };
        if number.is_finite() {
            Ok(number)
        } else {
            Err(invalid())
        }
    };
    match value {
        mlua::Value::Boolean(value) => Ok(Value::Bool(value)),
        mlua::Value::Table(table) if table.metatable().is_none() => {
            let count = table.raw_len();
            if !(2..=4).contains(&count) {
                return Err(invalid());
            }
            let mut seen = 0;
            for pair in table.clone().pairs::<mlua::Value, mlua::Value>().take(5) {
                let (key, _) = pair?;
                seen += 1;
                if !matches!(key, mlua::Value::Integer(i) if i > 0 && i as usize <= count) {
                    return Err(invalid());
                }
            }
            if seen != count {
                return Err(invalid());
            }
            let mut values = Vec::with_capacity(count);
            for index in 1..=count {
                values.push(scalar(table.raw_get(index)?)?);
            }
            Ok(Value::Vector(values))
        }
        value => scalar(value).map(Value::Scalar),
    }
}
