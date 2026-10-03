//! Fresh-VM player services; decisions remain staged until their host commit.
use super::{Invocation, Limits, Program, package::PackageSnapshot, startup::Pending};
use bloxgloom_host_api::{
    gameplay::{Context, Error},
    players::{Behavior, Decision, Event, Registration, State},
};
use mlua::{Function, Lua, Value};
use std::{cell::RefCell, rc::Rc, sync::Arc};
mod decisions;

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (key, revision, max_bytes, initial, module): (Value, Value, Value, Value, Value)| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error { return Err(error); }
            if !snapshot.permits_players(&namespace) { return Err("register_player_lifecycle requires bloxgloom:players/v1"); }
            if pending.player_lifecycles.len() >= 8 { return Err("at most eight player services per package"); }
            let key = super::values::text(key)?;
            if key.split_once(':').is_none_or(|(owner, local)| owner != namespace || !super::package::manifest::identifier(local)) { return Err("player service must belong to its startup package"); }
            if pending.player_lifecycles.iter().any(|r| r.key == key) { return Err("duplicate player service"); }
            let module = super::values::text(module)?;
            if module.split_once(':').map(|p| p.0) != Some(namespace.as_str()) || snapshot.source(&module).is_none() { return Err("player service requires an own-package module"); }
            let max_state_bytes = super::values::integer(max_bytes, 1, 4096)? as u16;
            let registration = Registration {
                key, version: snapshot.gameplay_version(&module, super::values::integer(revision, 1, u16::MAX.into())? as u16),
                max_state_bytes, initial_state: decisions::bytes(initial, usize::from(max_state_bytes))?,
                behavior: Arc::new(ScriptBehavior { snapshot: Arc::clone(&snapshot), module, max_bytes: usize::from(max_state_bytes) }),
            };
            registration.validate().map_err(|_| "invalid player service")?;
            pending.player_lifecycles.push(registration);
            Ok(())
        })();
        result.map_err(|error| { pending.error.get_or_insert(error); mlua::Error::RuntimeError(error.into()) })
    })
}

struct ScriptBehavior {
    snapshot: Arc<PackageSnapshot>,
    module: String,
    max_bytes: usize,
}
impl Behavior for ScriptBehavior {
    fn handle(
        &self,
        context: &mut Context<'_>,
        event: &Event,
        state: &State,
        session_data: &[u8],
    ) -> Result<Decision, Error> {
        let rejected = RefCell::new(None);
        let seed = super::runtime::Seed::new()
            .word(context.random_stream_seed())
            .bytes(&event.profile.to_le_bytes())
            .word(event.transition)
            .bytes(event.kind.name().as_bytes())
            .finish();
        let result = super::run_with(
            &Program::Package {
                snapshot: Arc::clone(&self.snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            super::runtime::Execution::new(
                event.kind.name(),
                seed,
                format!("player:{:032x}:{}", event.profile, event.transition),
            ),
            |lua, entry| {
                let fields = lua.create_table()?;
                fields.set("kind", event.kind.name())?;
                if let Some(region) = &event.region {
                    fields.set("region", region.as_str())?;
                }
                fields.set("profile", super::handles::profile(lua, event.profile)?)?;
                fields.set("identity_trust", "claimed_profile")?;
                fields.set("transition", super::handles::tick(lua, event.transition)?)?;
                if let Some(player) = &event.player {
                    fields.set(
                        "player",
                        super::gameplay::bindings::players::present(lua, player)?,
                    )?;
                }
                fields.set("state", lua.create_string(&state.data)?)?;
                fields.set("public_state", lua.create_string(&state.public_data)?)?;
                fields.set("session_state", lua.create_string(session_data)?)?;
                fields.set_readonly(true);
                let value = super::gameplay::bindings::invoke_fields::<Value>(
                    lua, entry, context, fields, &rejected,
                )?;
                decisions::decode(value, self.max_bytes, state).map_err(mlua::Error::RuntimeError)
            },
        );
        if let Some(error) = rejected.into_inner() {
            return Err(error);
        }
        result.map_err(|error| Error::Invalid(error.to_string()))
    }
}
