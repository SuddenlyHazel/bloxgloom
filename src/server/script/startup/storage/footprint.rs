//! Small storage footprints use only the registered block placement state.
use super::*;
use bloxgloom_host_api::lifecycle::FootprintCell;

pub(super) fn parse(value: Value, state: &str) -> Result<Vec<FootprintCell>, &'static str> {
    let Value::Table(list) = value else {
        return if value.is_nil() {
            Ok(vec![FootprintCell {
                offset: [0; 3],
                state: state.into(),
            }])
        } else {
            Err("storage footprint must be a list")
        };
    };
    if list.metatable().is_some() {
        return Err("storage footprint metatable forbidden");
    }
    let count = list.raw_len();
    if !(1..=8).contains(&count) || list.clone().pairs::<Value, Value>().count() != count {
        return Err("storage footprint must be a dense list of 1..=8 cells");
    }
    let mut cells = Vec::with_capacity(count);
    for index in 1..=count {
        let cell: mlua::Table = list.raw_get(index).map_err(|_| "invalid storage cell")?;
        if cell.metatable().is_some()
            || cell.raw_len() != 3
            || cell.clone().pairs::<Value, Value>().count() != 3
        {
            return Err("storage cell needs three offsets");
        }
        let mut offset = [0; 3];
        for (axis, coordinate) in offset.iter_mut().enumerate() {
            *coordinate = integer(
                cell.raw_get(axis + 1)
                    .map_err(|_| "invalid storage offset")?,
                -2,
                2,
            )? as i32;
        }
        if cells.iter().any(|old: &FootprintCell| old.offset == offset) {
            return Err("duplicate storage cell");
        }
        cells.push(FootprintCell {
            offset,
            state: state.into(),
        });
    }
    if !cells.iter().any(|cell| cell.offset == [0; 3]) {
        return Err("storage footprint needs an anchor");
    }
    Ok(cells)
}
