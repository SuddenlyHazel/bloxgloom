//! Bounded raw-value decoding shared by script adapters. Never coerce through
//! metamethods or copy VM strings before checking the host allocation bound.
use mlua::Value;

pub(super) fn integer(value: Value, min: i64, max: i64) -> Result<i64, &'static str> {
    let value = match value {
        Value::Integer(v) => v as f64,
        Value::Number(v) => v,
        _ => return Err("expected exact integer"),
    };
    if !value.is_finite() || value.fract() != 0.0 || value < min as f64 || value > max as f64 {
        return Err("integer out of bounds");
    }
    Ok(value as i64)
}

pub(super) fn text(value: Value) -> Result<String, &'static str> {
    let Value::String(value) = value else {
        return Err("expected UTF-8 string");
    };
    if value.as_bytes().is_empty() || value.as_bytes().len() > 255 {
        return Err("string must contain 1..=255 bytes");
    }
    value
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| "invalid UTF-8")
}
