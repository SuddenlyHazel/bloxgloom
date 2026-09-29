//! Small anchored footprints. Every cell uses the block's placement state.
use super::*;

pub(super) fn parse(value: Value, state: &str) -> Result<Vec<FootprintCell>, &'static str> {
    let Value::Table(list) = value else {
        return if value.is_nil() {
            Ok(vec![FootprintCell {
                offset: [0; 3],
                state: state.into(),
            }])
        } else {
            Err("machine footprint must be a list")
        };
    };
    let count = ports::sequence_len(&list, 8)?;
    if count == 0 {
        return Err("machine footprint needs an anchor");
    }
    let mut cells = Vec::with_capacity(count);
    for index in 1..=count {
        let offset: mlua::Table = list.raw_get(index).map_err(|_| "invalid machine cell")?;
        if ports::sequence_len(&offset, 3)? != 3 {
            return Err("machine cell needs three offsets");
        }
        let mut axes = [0; 3];
        for (axis, coordinate) in axes.iter_mut().enumerate() {
            *coordinate = integer(
                offset
                    .raw_get(axis + 1)
                    .map_err(|_| "invalid machine offset")?,
                -2,
                2,
            )? as i32;
        }
        if cells.iter().any(|cell: &FootprintCell| cell.offset == axes) {
            return Err("duplicate machine cell");
        }
        cells.push(FootprintCell {
            offset: axes,
            state: state.into(),
        });
    }
    if !cells.iter().any(|cell| cell.offset == [0; 3]) {
        return Err("machine footprint needs an anchor");
    }
    Ok(cells)
}
