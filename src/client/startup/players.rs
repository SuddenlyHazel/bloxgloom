//! Own-package registration and readonly local-player lifecycle inputs.
use super::*;
pub(in crate::client) struct Event<'a> {
    pub kind: &'static str,
    pub profile: u128,
    pub session: u64,
    pub states: &'a [crate::protocol::PlayerState],
    pub reason: &'a str,
}
impl Event<'_> {
    pub(super) fn seed(&self) -> u64 {
        let mut seed = crate::server::script_runtime::Seed::new()
            .bytes(&self.profile.to_le_bytes())
            .word(self.session)
            .bytes(self.kind.as_bytes());
        for state in self.states {
            seed = seed
                .bytes(state.key.as_bytes())
                .word(state.revision)
                .bytes(&state.public);
        }
        seed.finish()
    }
    pub(super) fn present(&self, lua: &Lua, owner: &str) -> mlua::Result<mlua::Table> {
        let event = lua.create_table()?;
        event.set("kind", self.kind)?;
        event.set(
            "profile",
            crate::server::script_handles::profile(lua, self.profile)?,
        )?;
        event.set(
            "session",
            crate::server::script_handles::session(lua, self.profile, self.session)?,
        )?;
        event.set("identity_trust", "claimed_profile")?;
        event.set("reason", self.reason)?;
        let states = lua.create_table()?;
        for state in self
            .states
            .iter()
            .filter(|s| s.key.split_once(':').map(|p| p.0) == Some(owner))
        {
            let value = lua.create_table()?;
            value.set(
                "revision",
                crate::server::script_handles::revision(lua, state.revision)?,
            )?;
            value.set("public", lua.create_string(&state.public)?)?;
            value.set_readonly(true);
            states.set(state.key.as_str(), value)?;
        }
        states.set_readonly(true);
        event.set("states", states)?;
        event.set_readonly(true);
        Ok(event)
    }
}

pub(super) fn declarer(
    lua: &Lua,
    registrations: Rc<RefCell<State>>,
    bundle: Arc<ClientBundle>,
    owner: String,
    startup: bool,
) -> mlua::Result<mlua::Function> {
    lua.create_function(move |_, module: mlua::LuaString| {
        let key = ascii(module, 129)?;
        let valid = key.split_once(':').is_some_and(|(namespace, local)| {
            namespace == owner
                && identifier(local)
                && bundle
                    .packages()
                    .get(namespace)
                    .is_some_and(|p| p.sources.contains_key(local))
        });
        if !startup
            || !bundle.permits_player_services(&owner)
            || !valid
            || registrations.borrow().player_handlers.contains_key(&owner)
        {
            return Err(mlua::Error::RuntimeError(
                "invalid or duplicate player lifecycle handler".into(),
            ));
        }
        registrations
            .borrow_mut()
            .player_handlers
            .insert(owner.clone(), key);
        Ok(())
    })
}
