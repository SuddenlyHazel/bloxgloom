//! Raw bounded profile identities and package-owned state operations.
use super::*;
use crate::server::script::handles;
use bloxgloom_host_api::players::State;
fn bytes(value: Value, max: usize) -> Result<Vec<u8>, Error> {
    let Value::String(value) = value else {
        return Err(invalid("expected binary string"));
    };
    if value.as_bytes().len() > max {
        return Err(invalid("profile state byte limit exceeded"));
    }
    Ok(value.as_bytes().to_vec())
}
pub(super) fn install<'scope>(
    scope: &'scope mlua::Scope<'scope, '_>,
    host: &mlua::Table,
    context: &'scope RefCell<&mut Context<'_>>,
    rejected: &'scope RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "profile_id",
        scope.create_function(|lua, value: Value| {
            let profile = checked(rejected, || {
                let token = text(value).map_err(invalid)?;
                let raw = token
                    .strip_prefix("profile:")
                    .filter(|v| {
                        v.len() == 32
                            && v.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    })
                    .ok_or_else(|| invalid("expected canonical profile token"))?;
                u128::from_str_radix(raw, 16)
                    .ok()
                    .filter(|v| *v != 0)
                    .ok_or_else(|| invalid("invalid profile ID"))
            })?;
            handles::profile(lua, profile).inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    host.set(
        "profile_state",
        scope.create_function(|lua, (key, profile): (Value, Value)| {
            let cell = checked(rejected, || {
                context.borrow_mut().profile_state(
                    &text(key).map_err(invalid)?,
                    handles::profile_value(profile).map_err(invalid)?,
                )
            })?;
            (|| {
                let value = lua.create_table()?;
                value.set("revision", handles::revision(lua, cell.revision)?)?;
                value.set("initialized", cell.initialized)?;
                value.set("state", lua.create_string(&cell.state.data)?)?;
                value.set("public_state", lua.create_string(&cell.state.public_data)?)?;
                if let Some(tick) = cell.next_tick {
                    value.set("next_tick", handles::tick(lua, tick)?)?;
                }
                value.set_readonly(true);
                Ok(value)
            })()
            .inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    host.set(
        "set_profile_state",
        scope.create_function(
            |_, (key, profile, private, public): (Value, Value, Value, Value)| {
                checked(rejected, || {
                    context.borrow_mut().set_profile_state(
                        &text(key).map_err(invalid)?,
                        handles::profile_value(profile).map_err(invalid)?,
                        State {
                            data: bytes(private, 4096)?,
                            public_data: bytes(public, 1024)?,
                        },
                    )
                })
            },
        )?,
    )?;
    Ok(())
}
