//! Session-only client startup. Convention: a format-2 client/shared module
//! named `client_startup` returns a function accepting a presentation host.
//! `host.set_text("package:document/node", "ASCII")` and
//! `host.set_state("package:document", "ASCII")` register initial UI state.
//! Only the module's own package may be changed. `import("package:module")`
//! sees this package and its direct exact dependencies, never server sources.
//! No VM, host callback, or registration survives the worker invocation.
use crate::server::client_bundle::ClientBundle;
use mlua::{Lua, LuaOptions, StdLib, Value, VmState};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    pub(crate) texts: BTreeMap<String, String>,
    pub(crate) states: BTreeMap<String, String>,
    pub(crate) replica: Option<Arc<crate::client::presentation::Script>>,
}

pub(crate) fn prepare(bundle: Arc<ClientBundle>) -> io::Result<State> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("client-luau-startup".into())
        .spawn(move || {
            let _ = sender.send(run(bundle));
        })?;
    // Each invocation has its own bounded interrupt, wall, memory and source
    // budget. The joining thread owns no VM or mutable session state.
    receiver
        .recv()
        .map_err(|_| io::Error::other("client startup worker stopped"))?
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn run(bundle: Arc<ClientBundle>) -> Result<State, String> {
    let mut state = State::default();
    for (owner, package) in bundle.packages() {
        if package.sources.contains_key("client_startup") {
            let entry = format!("{owner}:client_startup");
            execute(Arc::clone(&bundle), &entry, &mut state)?;
        }
    }
    Ok(state)
}

fn identity(bundle: &ClientBundle, key: &str) -> String {
    let (owner, module) = key.split_once(':').expect("validated module key");
    format!("{owner}@{}:{module}", bundle.packages()[owner].version)
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn ascii(value: mlua::LuaString, max: usize) -> mlua::Result<String> {
    let bytes = value.as_bytes();
    if bytes.len() > max || !bytes.iter().all(|b| (32..=126).contains(b)) {
        return Err(mlua::Error::RuntimeError("invalid startup text".into()));
    }
    Ok(value.to_str()?.to_owned())
}

fn execute(bundle: Arc<ClientBundle>, entry: &str, state: &mut State) -> Result<(), String> {
    let id = identity(&bundle, entry);
    let fail = |error: mlua::Error| format!("client startup {id}: {error}");
    let lua = Lua::new_with(StdLib::TABLE | StdLib::STRING, LuaOptions::default()).map_err(fail)?;
    for name in ["require", "print", "gcinfo", "getfenv", "setfenv"] {
        lua.globals().set(name, Value::Nil).map_err(fail)?;
    }
    lua.sandbox(true).map_err(fail)?;
    lua.set_memory_limit(8 * 1024 * 1024).map_err(fail)?;
    let deadline = Instant::now() + Duration::from_millis(50);
    let interrupts = Rc::new(Cell::new(10_000u64));
    let limit = Rc::clone(&interrupts);
    let exceeded = Rc::new(Cell::new(false));
    let tripped = Rc::clone(&exceeded);
    lua.set_interrupt(move |_| {
        if Instant::now() >= deadline || limit.get() == 0 {
            tripped.set(true);
            return Err(mlua::Error::RuntimeError(
                "client startup execution limit exceeded".into(),
            ));
        }
        limit.set(limit.get() - 1);
        Ok(VmState::Continue)
    });
    let cached = Rc::new(RefCell::new(BTreeMap::<String, mlua::RegistryKey>::new()));
    let stack = Rc::new(RefCell::new(Vec::new()));
    let registrations = Rc::new(RefCell::new(State::default()));
    let result = (|| -> mlua::Result<()> {
        let value = load(&lua, Arc::clone(&bundle), entry, &cached, &stack)?;
        let function: mlua::Function = lua.unpack(value)?;
        let host = lua.create_table()?;
        let owner = entry.split_once(':').unwrap().0.to_owned();
        let texts = Rc::clone(&registrations);
        let text_owner = owner.clone();
        host.set(
            "set_text",
            lua.create_function(move |_, (key, value): (mlua::LuaString, mlua::LuaString)| {
                let key = ascii(key, 194)?;
                let value = ascii(value, 128)?;
                if !key.starts_with(&format!("{text_owner}:"))
                    || !key.contains('/')
                    || texts.borrow().texts.len() >= 64
                {
                    return Err(mlua::Error::RuntimeError(
                        "invalid startup text target or limit".into(),
                    ));
                }
                texts.borrow_mut().texts.insert(key, value);
                Ok(())
            })?,
        )?;
        let states = Rc::clone(&registrations);
        host.set(
            "set_state",
            lua.create_function(move |_, (key, value): (mlua::LuaString, mlua::LuaString)| {
                let key = ascii(key, 129)?;
                let value = ascii(value, 128)?;
                if key
                    .split_once(':')
                    .is_none_or(|(package, document)| package != owner || !identifier(document))
                    || states.borrow().states.len() >= 8
                {
                    return Err(mlua::Error::RuntimeError(
                        "invalid startup state target or limit".into(),
                    ));
                }
                states.borrow_mut().states.insert(key, value);
                Ok(())
            })?,
        )?;
        let handler_bundle = Arc::clone(&bundle);
        let handler_owner = entry.split_once(':').unwrap().0.to_owned();
        let handler_state = Rc::clone(&registrations);
        host.set(
            "set_replica_handler",
            lua.create_function(move |_, module: mlua::LuaString| {
                let key = ascii(module, 129)?;
                let (owner, local) = key
                    .split_once(':')
                    .ok_or_else(|| mlua::Error::RuntimeError("invalid replica module".into()))?;
                if owner != handler_owner
                    || !identifier(local)
                    || handler_state.borrow().replica.is_some()
                {
                    return Err(mlua::Error::RuntimeError(
                        "invalid or duplicate replica handler".into(),
                    ));
                }
                let source = handler_bundle
                    .packages()
                    .get(owner)
                    .and_then(|package| package.sources.get(local))
                    .ok_or_else(|| {
                        mlua::Error::RuntimeError(
                            "replica handler must be a declared client module".into(),
                        )
                    })?;
                handler_state.borrow_mut().replica =
                    Some(Arc::new(crate::client::presentation::Script {
                        module: identity(&handler_bundle, &key),
                        source: source.source.clone(),
                    }));
                Ok(())
            })?,
        )?;
        function.call::<()>(host)
    })();
    if exceeded.get() || Instant::now() >= deadline {
        return Err(format!("client startup {id}: execution limit exceeded"));
    }
    result.map_err(fail)?;
    let output = registrations.borrow();
    if let Some(ui) = bundle.ui() {
        ui.validate_startup(&output)
            .map_err(|error| format!("client startup {id}: {error}"))?;
    } else if !output.texts.is_empty() || !output.states.is_empty() {
        return Err(format!(
            "client startup {id}: no package UI for registered presentation"
        ));
    }
    if output.replica.is_some() && state.replica.is_some() {
        return Err(format!(
            "client startup {id}: only one replica handler per session"
        ));
    }
    state.texts.extend(output.texts.clone());
    state.states.extend(output.states.clone());
    state.replica = output.replica.clone().or_else(|| state.replica.take());
    Ok(())
}

// Imports keep the lexical caller in their closure even when exported functions
// travel across packages. Loading and the export cache are invocation-local.
fn load(
    lua: &Lua,
    bundle: Arc<ClientBundle>,
    key: &str,
    cached: &Rc<RefCell<BTreeMap<String, mlua::RegistryKey>>>,
    stack: &Rc<RefCell<Vec<String>>>,
) -> mlua::Result<Value> {
    if let Some(value) = cached.borrow().get(key) {
        return lua.registry_value(value);
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
        cached
            .borrow_mut()
            .insert(key.to_owned(), lua.create_registry_value(value.clone())?);
        Ok(value)
    })();
    stack.borrow_mut().pop();
    result.map_err(|error: mlua::Error| mlua::Error::RuntimeError(format!("{id}: {error}")))
}
