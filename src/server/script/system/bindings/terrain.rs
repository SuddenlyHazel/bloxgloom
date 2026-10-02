//! Box queries remain within the declared captured chunk neighborhood.
use super::*;
use mlua::{Scope, Table};
pub(super) fn install<'scope, 'env: 'scope>(
    scope: &'scope Scope<'scope, 'env>,
    host: &Table,
    context: &'env api::Context<'_>,
    reads: &'env Cell<usize>,
    rejected: &'env RefCell<Option<&'static str>>,
) -> mlua::Result<()> {
    host.set(
        "blocks",
        scope.create_function(
            |lua, (x, y, z, w, h, d): (Value, Value, Value, Value, Value, Value)| {
                let blocks = checked(rejected, || {
                    let size = [w, h, d].map(|v| integer(v, 1, 64).map(|v| v as u8));
                    let [Ok(w), Ok(h), Ok(d)] = size else {
                        return Err("invalid terrain query dimensions");
                    };
                    let cells = bloxgloom_host_api::queries::box_cells(cell(x, y, z)?, [w, h, d])
                        .map_err(|_| "invalid terrain query bounds")?;
                    if reads.get() + cells.len() > 64 {
                        return Err("system world read limit exceeded (64)");
                    }
                    reads.set(reads.get() + cells.len());
                    cells
                        .into_iter()
                        .map(|cell| {
                            context
                                .block(cell)
                                .map(|block| (cell, block))
                                .map_err(|_| "system world read unavailable")
                        })
                        .collect::<Result<Vec<_>, _>>()
                })?;
                (|| {
                    let result = lua.create_table_with_capacity(blocks.len(), 0)?;
                    for (index, (cell, block)) in blocks.iter().enumerate() {
                        let value = lua.create_table()?;
                        value.set("state", block.state.as_str())?;
                        value.set("block_type", block.block_type.as_str())?;
                        value.set("primary_item", block.primary_item.as_deref())?;
                        value.set("plant", block.plant)?;
                        value.set("supports_plant", block.supports_plant)?;
                        let coordinates = lua.create_sequence_from(*cell)?;
                        coordinates.set_readonly(true);
                        value.set("cell", coordinates)?;
                        value.set_readonly(true);
                        result.raw_set(index + 1, value)?;
                    }
                    result.set_readonly(true);
                    Ok(result)
                })()
                .inspect_err(|_: &mlua::Error| {
                    rejected
                        .borrow_mut()
                        .get_or_insert("terrain query result failed");
                })
            },
        )?,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests;
