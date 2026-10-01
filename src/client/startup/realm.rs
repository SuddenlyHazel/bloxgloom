//! Connection-owned player module exports and lexical client imports.
use super::{ascii, identifier, identity};
use crate::server::client_bundle::ClientBundle;
use mlua::{Lua, Value};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

pub(super) enum Export {
    Ready(mlua::RegistryKey),
    Failed(String),
}

/// One handler's connection-owned realm; errors retire the entire VM.
pub(in crate::client) struct EventRealm {
    pub(super) lua: Lua,
    _reservation: crate::server::script_runtime::Reservation,
    pub(super) diagnostics: crate::server::script_runtime::Diagnostics,
    pub(super) cached: Rc<RefCell<BTreeMap<String, Export>>>,
    pub(super) stack: Rc<RefCell<Vec<String>>>,
}
impl EventRealm {
    pub(in crate::client) fn id(&self) -> u64 {
        self._reservation.id()
    }
    pub(in crate::client) fn new(bundle: &ClientBundle, entry: &str) -> Result<Self, String> {
        let id = identity(bundle, entry);
        let reservation = crate::server::script_runtime::Reservation::acquire(8 * 1024 * 1024)
            .map_err(|error| format!("client realm {id}: {error}"))?;
        let (lua, diagnostics) = crate::server::script_runtime::create(
            &id,
            crate::server::script_runtime::Execution::new("client_module_init", 0, "local_session")
                .client(),
        )
        .map_err(|error| format!("client realm {id}: {error}"))?;
        tracing::debug!(module=%id, runtime_id=reservation.id(), "client player realm initialized");
        Ok(Self {
            lua,
            _reservation: reservation,
            diagnostics,
            cached: Rc::default(),
            stack: Rc::default(),
        })
    }
}
impl Drop for EventRealm {
    fn drop(&mut self) {
        // Break registry/export/import ownership before disposing the VM.
        self.cached.borrow_mut().clear();
        self.lua.expire_registry_values();
    }
}

// Imports keep the lexical caller in their closure even when exported functions
// travel across packages. Export caches belong to a single handler realm.
pub(super) fn load(
    lua: &Lua,
    bundle: Arc<ClientBundle>,
    key: &str,
    cached: &Rc<RefCell<BTreeMap<String, Export>>>,
    stack: &Rc<RefCell<Vec<String>>>,
) -> mlua::Result<Value> {
    if let Some(value) = cached.borrow().get(key) {
        return match value {
            Export::Ready(value) => lua.registry_value(value),
            Export::Failed(error) => Err(mlua::Error::RuntimeError(error.clone())),
        };
    }
    let id = identity(&bundle, key);
    if stack.borrow().len() >= 32 || stack.borrow().iter().any(|k| k == key) {
        return Err(mlua::Error::RuntimeError(format!(
            "{id}: cyclic or deep import"
        )));
    }
    let (owner, module) = key.split_once(':').unwrap();
    let source = &bundle.packages()[owner].sources[module].source;
    if source.len() > 64 * 1024 {
        return Err(mlua::Error::RuntimeError(format!("{id}: source too large")));
    }
    stack.borrow_mut().push(key.to_owned());
    let result = (|| {
        let environment = lua.create_table()?;
        let meta = lua.create_table()?;
        meta.set("__index", lua.globals())?;
        meta.set("__metatable", false)?;
        environment.set_metatable(Some(meta))?;
        let caller = key.to_owned();
        let import_cached = Rc::clone(cached);
        let import_stack = Rc::clone(stack);
        let imports_bundle = Arc::clone(&bundle);
        environment.set(
            "import",
            lua.create_function(move |lua, name: mlua::LuaString| {
                let name = ascii(name, 129)?;
                let (target, module) = name
                    .split_once(':')
                    .ok_or_else(|| mlua::Error::RuntimeError("invalid import".into()))?;
                let (owner, _) = caller.split_once(':').unwrap();
                let package = &imports_bundle.packages()[owner];
                if !identifier(target)
                    || !identifier(module)
                    || (target != owner && !package.dependencies.contains_key(target))
                    || imports_bundle
                        .packages()
                        .get(target)
                        .is_none_or(|p| !p.sources.contains_key(module))
                {
                    return Err(mlua::Error::RuntimeError(format!(
                        "{}: inaccessible client import {name}",
                        identity(&imports_bundle, &caller)
                    )));
                }
                load(
                    lua,
                    Arc::clone(&imports_bundle),
                    &name,
                    &import_cached,
                    &import_stack,
                )
            })?,
        )?;
        let value: Value = lua
            .load(source)
            .set_name(&id)
            .set_mode(mlua::chunk::ChunkMode::Text)
            .set_environment(environment)
            .eval()?;
        if value.is_nil() {
            return Err(mlua::Error::RuntimeError(format!("{id}: nil export")));
        }
        cached.borrow_mut().insert(
            key.to_owned(),
            Export::Ready(lua.create_registry_value(value.clone())?),
        );
        Ok(value)
    })();
    stack.borrow_mut().pop();
    result.map_err(|error: mlua::Error| {
        let message = format!("{id}: {error}");
        cached
            .borrow_mut()
            .insert(key.to_owned(), Export::Failed(message.clone()));
        mlua::Error::RuntimeError(message)
    })
}
