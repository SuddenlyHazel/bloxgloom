//! Pure immutable moderation inputs. No mutation-capable gameplay host is exposed.
use crate::server::script::{self, Invocation, Limits, Program, package::PackageSnapshot, values};
use bloxgloom_host_api::chat::{Decision, Moderator, Request, Route};
use mlua::Value;
use std::sync::Arc;
pub(super) struct ScriptModerator {
    pub(super) snapshot: Arc<PackageSnapshot>,
    pub(super) module: String,
}
impl Moderator for ScriptModerator {
    fn moderate(&self, request: &Request) -> Result<Decision, String> {
        script::run_with(
            &Program::Package {
                snapshot: Arc::clone(&self.snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            script::runtime::Execution::new(
                "ChatModeration",
                0,
                format!(
                    "chat:{:032x}:{}",
                    request.sender.profile, request.sender.session
                ),
            ),
            |lua, entry| {
                let input = lua.create_table()?;
                input.set(
                    "sender",
                    script::gameplay::bindings::players::present(lua, &request.sender)?,
                )?;
                input.set("text", request.text.as_str())?;
                input.set("identity_trust", "claimed_profile")?;
                let players = lua.create_table()?;
                for (i, player) in request.players.iter().enumerate() {
                    players.raw_set(
                        i + 1,
                        script::gameplay::bindings::players::present(lua, player)?,
                    )?;
                }
                players.set_readonly(true);
                input.set("players", players)?;
                input.set_readonly(true);
                let value = entry.call::<Value>(input)?;
                decode(value).map_err(mlua::Error::RuntimeError)
            },
        )
        .map_err(|e| e.to_string())
    }
}
fn decode(value: Value) -> Result<Decision, String> {
    let Value::Table(table) = value else {
        return Err("chat hook must explicitly return allow or deny".into());
    };
    if table.metatable().is_some() {
        return Err("chat decision cannot have a metatable".into());
    }
    let mut allow = None;
    let mut text = None;
    let mut reason = None;
    let mut route = Route::All;
    for pair in table.pairs::<Value, Value>().take(5) {
        let (key, value) = pair.map_err(|e| e.to_string())?;
        match values::text(key)?.as_str() {
            "allow" => {
                let Value::Boolean(value) = value else {
                    return Err("allow must be boolean".into());
                };
                allow = Some(value);
            }
            "text" => text = Some(values::text(value)?),
            "reason" => reason = Some(values::text(value)?),
            "recipients" => {
                let Value::Table(list) = value else {
                    return Err("recipients must be a session list".into());
                };
                if list.metatable().is_some() {
                    return Err("recipient list cannot have a metatable".into());
                }
                let mut recipients = Vec::new();
                for pair in list
                    .pairs::<Value, Value>()
                    .take(bloxgloom_host_api::chat::MAX_RECIPIENTS + 1)
                {
                    let (index, value) = pair.map_err(|e| e.to_string())?;
                    values::integer(index, 1, bloxgloom_host_api::chat::MAX_RECIPIENTS as i64)?;
                    let Value::Table(player) = value else {
                        return Err("recipient must contain exact profile/session handles".into());
                    };
                    let profile = script::handles::profile_value(
                        player
                            .raw_get::<Value>("profile")
                            .map_err(|e| e.to_string())?,
                    )
                    .map_err(|_| "invalid profile handle")?;
                    let session = script::handles::session_value(
                        player
                            .raw_get::<Value>("session")
                            .map_err(|e| e.to_string())?,
                    )
                    .map_err(|_| "invalid session handle")?;
                    if session.profile != profile {
                        return Err("recipient profile and session disagree".into());
                    }
                    recipients.push((profile, session.epoch));
                }
                route = Route::Sessions(recipients);
            }
            _ => return Err("unsupported chat decision field".into()),
        }
    }
    match allow {
        Some(true) if reason.is_none() => Ok(Decision::Allow {
            text: text.ok_or("allowed chat requires text")?,
            route,
        }),
        Some(false) if text.is_none() && route == Route::All => Ok(Decision::Deny {
            reason: reason.ok_or("denied chat requires reason")?,
        }),
        _ => Err("chat hook must explicitly allow or deny without conflicting fields".into()),
    }
}
