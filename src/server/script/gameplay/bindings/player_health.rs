//! Exact session handles and exact revision handles; no numeric identity coercion.
use super::*;
use crate::server::script::handles;
use bloxgloom_host_api::player_health::{MAX_HEALTH, View};
pub(in crate::server::script) fn present(lua: &Lua, value: View) -> mlua::Result<mlua::Table> {
    let view = lua.create_table()?;
    view.set("current", value.current)?;
    view.set("max", value.max)?;
    view.set("alive", value.alive)?;
    view.set("revision", handles::revision(lua, value.revision)?)?;
    view.set("life", handles::tick(lua, value.life)?)?;
    view.set_readonly(true);
    Ok(view)
}
fn target(value: Value) -> Result<(u128, u64), Error> {
    let value = handles::session_value(value).map_err(invalid)?;
    Ok((value.profile, value.epoch))
}
fn revision(value: Value) -> Result<u64, Error> {
    handles::revision_value(value).map_err(invalid)
}
pub(super) fn install<'scope>(
    scope: &'scope mlua::Scope<'scope, '_>,
    host: &mlua::Table,
    context: &'scope RefCell<&mut Context<'_>>,
    rejected: &'scope RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "player_health",
        scope.create_function(|lua, value: Value| {
            let value = checked(rejected, || {
                let (profile, session) = target(value)?;
                context.borrow_mut().player_health(profile, session)
            })?;
            present(lua, value).inspect_err(|e| queries::latch(rejected, e))
        })?,
    )?;
    host.set(
        "damage_player",
        scope.create_function(
            |_, (target_value, revision_value, amount, cause): (Value, Value, Value, Value)| {
                checked(rejected, || {
                    let (profile, session) = target(target_value)?;
                    let revision = revision(revision_value)?;
                    let amount = integer(amount, 1, i64::from(MAX_HEALTH)).map_err(invalid)? as u32;
                    let cause = text(cause).map_err(invalid)?;
                    context
                        .borrow_mut()
                        .damage_player(profile, session, revision, amount, &cause)
                })
            },
        )?,
    )?;
    for (name, maximum) in [("heal_player", false), ("set_player_max_health", true)] {
        host.set(
            name,
            scope.create_function(
                move |_, (target_value, revision_value, amount): (Value, Value, Value)| {
                    checked(rejected, || {
                        let (profile, session) = target(target_value)?;
                        let revision = revision(revision_value)?;
                        let amount =
                            integer(amount, 1, i64::from(MAX_HEALTH)).map_err(invalid)? as u32;
                        if maximum {
                            context
                                .borrow_mut()
                                .set_player_max_health(profile, session, revision, amount)
                        } else {
                            context
                                .borrow_mut()
                                .heal_player(profile, session, revision, amount)
                        }
                    })
                },
            )?,
        )?;
    }
    host.set(
        "respawn_player",
        scope.create_function(
            |_, (target_value, revision_value, x, y, z): (Value, Value, Value, Value, Value)| {
                checked(rejected, || {
                    let (profile, session) = target(target_value)?;
                    let revision = revision(revision_value)?;
                    let mut position = [0.; 3];
                    for (axis, value) in [x, y, z].into_iter().enumerate() {
                        let value = match value {
                            Value::Integer(v) => v as f64,
                            Value::Number(v) => v,
                            _ => return Err(invalid("respawn coordinates must be finite numbers")),
                        };
                        if !value.is_finite() || value.abs() >= 1_000_000. {
                            return Err(invalid("invalid respawn coordinates"));
                        }
                        position[axis] = value as f32;
                    }
                    context
                        .borrow_mut()
                        .respawn_player(profile, session, revision, position)
                })
            },
        )?,
    )?;
    Ok(())
}
