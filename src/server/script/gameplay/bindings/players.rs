//! Exact actor identity and captured online-directory queries.
use super::*;
use crate::server::script::handles;
use bloxgloom_host_api::gameplay::Player;

pub(crate) fn present(lua: &Lua, player: &Player) -> mlua::Result<mlua::Table> {
    let view = lua.create_table()?;
    view.set("profile", handles::profile(lua, player.profile)?)?;
    view.set(
        "session",
        handles::session(lua, player.profile, player.session)?,
    )?;
    if player.entity != 0 {
        view.set("entity", handles::entity(lua, player.entity)?)?;
    }
    view.set("name", player.name.as_str())?;
    let position = lua.create_sequence_from(player.position)?;
    position.set_readonly(true);
    view.set("position", position)?;
    let appearance = lua.create_table()?;
    for (key, value) in ["skin", "shirt", "pants", "flags"]
        .into_iter()
        .zip(player.appearance)
    {
        appearance.raw_set(key, value)?;
    }
    appearance.set_readonly(true);
    view.set("appearance", appearance)?;
    view.set("online", true)?;
    view.set("identity_trust", "claimed_profile")?;
    view.set_readonly(true);
    Ok(view)
}

pub(super) fn install<'scope>(
    scope: &'scope mlua::Scope<'scope, '_>,
    host: &mlua::Table,
    context: &'scope RefCell<&mut Context<'_>>,
    rejected: &'scope RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "set_player_appearance",
        scope.create_function(
            |_, (session, skin, shirt, pants): (Value, Value, Value, Value)| {
                checked(rejected, || {
                    let session = handles::session_value(session).map_err(invalid)?;
                    let palette = |value| integer(value, 0, 255).map(|v| v as u8).map_err(invalid);
                    context.borrow_mut().set_player_appearance(
                        session.profile,
                        session.epoch,
                        [palette(skin)?, palette(shirt)?, palette(pants)?],
                    )
                })
            },
        )?,
    )?;
    for (key, kick) in [("message_player", false), ("kick_player", true)] {
        host.set(
            key,
            scope.create_function(move |_, (session, text_value): (Value, Value)| {
                checked(rejected, || {
                    let session = handles::session_value(session).map_err(invalid)?;
                    let text = super::super::super::values::text(text_value).map_err(invalid)?;
                    if kick {
                        context
                            .borrow_mut()
                            .kick_player(session.profile, session.epoch, &text)
                    } else {
                        context
                            .borrow_mut()
                            .message_player(session.profile, session.epoch, &text)
                    }
                })
            })?,
        )?;
    }
    host.set(
        "players",
        scope.create_function(|lua, ()| {
            checked(rejected, || context.borrow_mut().players())
                .and_then(|players| {
                    let views = players
                        .iter()
                        .map(|player| present(lua, player))
                        .collect::<mlua::Result<Vec<_>>>()?;
                    let list = lua.create_sequence_from(views)?;
                    list.set_readonly(true);
                    Ok(list)
                })
                .inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    host.set(
        "player_by_profile",
        scope.create_function(|lua, value: Value| {
            let player = checked(rejected, || {
                context
                    .borrow_mut()
                    .player_by_profile(handles::profile_value(value).map_err(invalid)?)
            })?;
            player
                .as_ref()
                .map(|player| present(lua, player))
                .transpose()
                .inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    host.set(
        "player_by_session",
        scope.create_function(|lua, value: Value| {
            let player = checked(rejected, || {
                let session = handles::session_value(value).map_err(invalid)?;
                context
                    .borrow_mut()
                    .player_by_session(session.profile, session.epoch)
            })?;
            player
                .as_ref()
                .map(|player| present(lua, player))
                .transpose()
                .inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    Ok(())
}
