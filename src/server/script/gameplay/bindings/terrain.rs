//! Bounded spatial queries use the same charged overlay reads as host.block.
use super::*;
use mlua::{Scope, Table};
pub(super) fn install<'scope, 'env: 'scope>(
    scope: &'scope Scope<'scope, 'env>,
    host: &Table,
    context: &'env RefCell<&mut Context<'_>>,
    rejected: &'env RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "blocks",
        scope.create_function(
            |lua, (x, y, z, w, h, d): (Value, Value, Value, Value, Value, Value)| {
                let blocks = checked(rejected, || {
                    let size =
                        [w, h, d].map(|v| integer(v, 1, 64).map(|v| v as u8).map_err(invalid));
                    let [Ok(w), Ok(h), Ok(d)] = size else {
                        return Err(invalid("invalid terrain query dimensions"));
                    };
                    let cells =
                        bloxgloom_host_api::queries::box_cells(cell_at(x, y, z)?, [w, h, d])?;
                    let mut context = context.borrow_mut();
                    cells
                        .into_iter()
                        .map(|cell| context.block(cell).map(|block| (cell, block)))
                        .collect::<Result<Vec<_>, _>>()
                })?;
                (|| {
                    let result = lua.create_table_with_capacity(blocks.len(), 0)?;
                    for (index, (cell, block)) in blocks.iter().enumerate() {
                        let record = events::block(lua, block)?;
                        record.set_readonly(false);
                        let coordinates = lua.create_sequence_from(*cell)?;
                        coordinates.set_readonly(true);
                        record.set("cell", coordinates)?;
                        record.set_readonly(true);
                        result.raw_set(index + 1, record)?;
                    }
                    result.set_readonly(true);
                    Ok(result)
                })()
                .inspect_err(|_: &mlua::Error| {
                    rejected
                        .borrow_mut()
                        .get_or_insert(invalid("terrain query result failed"));
                })
            },
        )?,
    )?;
    Ok(())
}
