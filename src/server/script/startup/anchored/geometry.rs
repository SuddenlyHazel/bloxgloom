//! Full native anchored footprints and explicit terrain observation windows.
use super::*;
use bloxgloom_host_api::lifecycle::FootprintCell;
fn list(value: Value, maximum: usize) -> Result<mlua::Table, &'static str> {
    let Value::Table(t) = value else {
        return Err("anchored geometry must be a dense sequence");
    };
    let len = t.raw_len();
    if t.metatable().is_some() || len > maximum {
        return Err("anchored geometry exceeds bounds");
    }
    let mut seen = 0;
    for pair in t.clone().pairs::<Value, Value>().take(maximum + 1) {
        let (key, _) = pair.map_err(|_| "invalid anchored geometry")?;
        if !matches!(key,Value::Integer(i) if i>0 && i as usize<=len) {
            return Err("anchored geometry must be a dense sequence");
        }
        seen += 1;
    }
    if seen != len {
        return Err("anchored geometry must be a dense sequence");
    }
    Ok(t)
}
fn offset(value: Value) -> Result<[i32; 3], &'static str> {
    let t = list(value, 3)?;
    if t.raw_len() != 3 {
        return Err("anchored offset needs three coordinates");
    }
    let mut result = [0; 3];
    for (axis, coordinate) in result.iter_mut().enumerate() {
        *coordinate = integer(
            t.raw_get(axis + 1).map_err(|_| "invalid anchored offset")?,
            -16,
            16,
        )? as i32;
    }
    Ok(result)
}
pub(super) fn footprint(
    value: Value,
    anchor: &str,
    block: &Block,
) -> Result<Vec<FootprintCell>, &'static str> {
    if value.is_nil() {
        return Ok(vec![FootprintCell {
            offset: [0; 3],
            state: anchor.into(),
        }]);
    }
    let t = list(value, 64)?;
    if t.raw_len() == 0 {
        return Err("anchored footprint needs an anchor");
    }
    let mut cells = Vec::with_capacity(t.raw_len());
    for i in 1..=t.raw_len() {
        let cell: mlua::Table = t
            .raw_get(i)
            .map_err(|_| "invalid anchored footprint cell")?;
        if cell.metatable().is_some() {
            return Err("anchored footprint cell metatable forbidden");
        }
        for pair in cell.clone().pairs::<Value, Value>().take(3) {
            let (Value::String(key), _) = pair.map_err(|_| "invalid anchored footprint field")?
            else {
                return Err("invalid anchored footprint field");
            };
            if !matches!(
                key.to_str()
                    .map_err(|_| "invalid anchored footprint field")?
                    .as_ref(),
                "offset" | "state"
            ) {
                return Err("unknown anchored footprint field");
            }
        }
        let axes = offset(field(&cell, "offset")?)?;
        let state = state_key(field(&cell, "state")?, anchor.into())?;
        if !has_state(block, &state) {
            return Err("anchored footprint states must belong to the block");
        }
        if cells.iter().any(|c: &FootprintCell| c.offset == axes) {
            return Err("duplicate anchored footprint offset");
        }
        cells.push(FootprintCell {
            offset: axes,
            state,
        });
    }
    Ok(cells)
}
pub(super) fn observe(value: Value) -> Result<Vec<[i32; 3]>, &'static str> {
    if value.is_nil() {
        return Ok(vec![]);
    }
    let t = list(value, 64)?;
    let mut cells = Vec::with_capacity(t.raw_len());
    for i in 1..=t.raw_len() {
        let axes = offset(t.raw_get(i).map_err(|_| "invalid anchored observation")?)?;
        if cells.contains(&axes) {
            return Err("duplicate anchored observed offset");
        }
        cells.push(axes);
    }
    Ok(cells)
}
