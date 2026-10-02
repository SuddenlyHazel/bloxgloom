//! Weather reads and actor-authorized durable controls.
use super::*;
pub(super) fn install<'scope, 'env>(
    scope: &'scope mlua::Scope<'scope, 'env>,
    host: &mlua::Table,
    context: &'scope RefCell<&mut Context<'_>>,
    rejected: &'scope RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "weather",
        scope.create_function(|lua, ()| {
            checked(rejected, || context.borrow_mut().weather())
                .and_then(|weather| crate::weather::luau::present(lua, weather))
                .inspect_err(|error| {
                    rejected
                        .borrow_mut()
                        .get_or_insert_with(|| invalid(&error.to_string()));
                })
        })?,
    )?;
    host.set(
        "admin_set_weather",
        scope.create_function(|_, (kind, duration): (Value, Value)| {
            checked(rejected, || {
                let kind = match text(kind).map_err(invalid)?.as_str() {
                    "clear" => 0,
                    "rain" => 1,
                    "storm" => 2,
                    "storm_mild" => 3,
                    "storm_severe" => 4,
                    _ => return Err(invalid("invalid weather kind")),
                };
                context
                    .borrow_mut()
                    .admin_set_weather(kind, integer(duration, 0, 60_000).map_err(invalid)? as u32)
            })
        })?,
    )?;
    Ok(())
}
