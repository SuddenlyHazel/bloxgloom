//! Strict bounded decision decoding; no user metamethods execute here.
use bloxgloom_host_api::players::{Decision, State};
use mlua::Value;
pub(super) fn bytes(value: Value, max: usize) -> Result<Vec<u8>, &'static str> {
    let Value::String(value) = value else {
        return Err("expected binary state string");
    };
    if value.as_bytes().len() > max {
        return Err("player state byte limit exceeded");
    }
    Ok(value.as_bytes().to_vec())
}
fn delay(value: Value) -> Result<Option<u32>, &'static str> {
    if matches!(value, Value::Boolean(false)) {
        return Ok(None);
    }
    super::super::values::integer(value, 1, 100_000).map(|n| Some(n as u32))
}
pub(super) fn decode(value: Value, max: usize, old: &State) -> Result<Decision, String> {
    if value.is_nil() {
        return Ok(Decision::default());
    }
    let Value::Table(table) = value else {
        return Err("player callback must return nil or a decision table".into());
    };
    if table.metatable().is_some() {
        return Err("player decision must not have a metatable".into());
    }
    let mut decision = Decision::default();
    for pair in table.pairs::<Value, Value>().take(9) {
        let (key, value) = pair.map_err(|e| e.to_string())?;
        match super::super::values::text(key)?.as_str() {
            "state" => decision.state.get_or_insert_with(|| old.clone()).data = bytes(value, max)?,
            "public_state" => {
                decision
                    .state
                    .get_or_insert_with(|| old.clone())
                    .public_data = bytes(value, 1024)?
            }
            "session_state" => decision.session_data = Some(bytes(value, 4096)?),
            "profile_delay" => decision.profile_delay = Some(delay(value)?),
            "session_delay" => decision.session_delay = Some(delay(value)?),
            "deny" => decision.deny = Some(super::super::values::text(value)?),
            "spawn" => {
                let Value::Table(position) = value else {
                    return Err("spawn must be a coordinate triple".into());
                };
                if position.metatable().is_some() {
                    return Err("spawn must not have a metatable".into());
                }
                let mut xyz = [0.; 3];
                let mut seen = [false; 3];
                for pair in position.pairs::<Value, Value>().take(4) {
                    let (axis, value) = pair.map_err(|e| e.to_string())?;
                    let axis = super::super::values::integer(axis, 1, 3)? as usize - 1;
                    let number = match value {
                        Value::Number(n) => n,
                        Value::Integer(n) => n as f64,
                        _ => return Err("spawn coordinates must be numbers".into()),
                    };
                    if !number.is_finite() || number.abs() > i32::MAX as f64 / 2. {
                        return Err("spawn coordinate outside world bounds".into());
                    }
                    seen[axis] = true;
                    xyz[axis] = number as f32;
                }
                if seen != [true; 3] {
                    return Err("spawn must contain exactly three coordinates".into());
                }
                decision.spawn = Some(xyz);
            }
            _ => return Err("unsupported player decision field".into()),
        }
    }
    Ok(decision)
}
