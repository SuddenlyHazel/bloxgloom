//! Session-only client startup. Convention: a format-2 client/shared module
//! named `client_startup` returns a function accepting a presentation host.
//! `host.set_text("package:document/node", "ASCII")` and
//! `host.set_state("package:document", "ASCII")` register initial UI state.
//! Only the module's own package may be changed. `import("package:module")`
//! sees this package and its direct exact dependencies, never server sources.
//! No VM, host callback, or registration survives the worker invocation.
use crate::server::client_bundle::ClientBundle;

pub(super) mod players;
mod readiness;
use mlua::{Lua, Value, VmState};
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
    pub(crate) replica_observations: bool,
    pub(crate) player_handlers: BTreeMap<String, String>,
    pub(crate) parameters: crate::render::parameters::State,
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
    readiness::validate_sources(&bundle)?;
    let mut state = State {
        parameters: bundle.parameter_state()?,
        ..Default::default()
    };
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

fn display(value: mlua::LuaString, max: usize) -> mlua::Result<String> {
    let text = value.to_str()?;
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(mlua::Error::RuntimeError("invalid startup text".into()));
    }
    Ok(text.to_owned())
}

fn execute(bundle: Arc<ClientBundle>, entry: &str, state: &mut State) -> Result<(), String> {
    execute_event(bundle, entry, state, None)
}

pub(super) fn execute_event(
    bundle: Arc<ClientBundle>,
    entry: &str,
    state: &mut State,
    event: Option<&players::Event<'_>>,
) -> Result<(), String> {
    let id = identity(&bundle, entry);
    let phase = event.map_or("client startup", |_| "client player callback");
    let fail = |error: mlua::Error| format!("{phase} {id}: {error}");
    let (lua, diagnostics) = crate::server::script_runtime::create(
        &id,
        crate::server::script_runtime::Execution::new(
            event.map_or("client_startup", |e| e.kind),
            event.map_or(0, |e| e.seed()),
            "local_session",
        )
        .client(),
    )
    .map_err(fail)?;
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
    let registrations = Rc::new(RefCell::new(State {
        parameters: state.parameters.clone(),
        ..Default::default()
    }));
    let result = (|| -> mlua::Result<()> {
        let value = load(&lua, Arc::clone(&bundle), entry, &cached, &stack)?;
        let function: mlua::Function = lua.unpack(value)?;
        let host = lua.create_table()?;
        let startup = event.is_none();
        host.set(
            "set_player_handler",
            players::declarer(
                &lua,
                Rc::clone(&registrations),
                Arc::clone(&bundle),
                entry.split_once(':').unwrap().0.to_owned(),
                startup,
            )?,
        )?;
        let owner = entry.split_once(':').unwrap().0.to_owned();
        let texts = Rc::clone(&registrations);
        let text_owner = owner.clone();
        host.set(
            "set_text",
            lua.create_function(move |_, (key, value): (mlua::LuaString, mlua::LuaString)| {
                let key = ascii(key, 194)?;
                let value = display(value, 128)?;
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
                let value = display(value, 128)?;
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
            lua.create_function(
                move |_, (module, observations): (mlua::LuaString, Option<bool>)| {
                    let key = ascii(module, 129)?;
                    let (owner, local) = key.split_once(':').ok_or_else(|| {
                        mlua::Error::RuntimeError("invalid replica module".into())
                    })?;
                    if !startup
                        || owner != handler_owner
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
                    handler_state.borrow_mut().replica_observations = observations.unwrap_or(false);
                    Ok(())
                },
            )?,
        )?;
        let parameters = Rc::clone(&registrations);
        let parameter_owner = entry.split_once(':').unwrap().0.to_owned();
        host.set(
            "set_parameter",
            lua.create_function(
                move |_, (resource, name, value): (mlua::LuaString, mlua::LuaString, Value)| {
                    let update = crate::render::parameters::Update {
                        resource: ascii(resource, 129)?,
                        name: ascii(name, 64)?,
                        value: crate::client::presentation::parameters::decode(value)?,
                    };
                    parameters
                        .borrow_mut()
                        .parameters
                        .apply(&parameter_owner, &[update])
                        .map_err(mlua::Error::RuntimeError)
                },
            )?,
        )?;
        if let Some(event) = event {
            // A caught invalid host call still rejects the whole local update.
            let failed = Rc::new(Cell::new(false));
            let entries = host
                .pairs::<String, mlua::Function>()
                .collect::<mlua::Result<Vec<_>>>()?;
            for (key, function) in entries {
                let failed = Rc::clone(&failed);
                host.set(
                    key,
                    lua.create_function(move |_, args: mlua::MultiValue| {
                        let result = function.call::<mlua::MultiValue>(args);
                        if result.is_err() {
                            failed.set(true);
                        }
                        result
                    })?,
                )?;
            }
            host.set_readonly(true);
            function.call::<()>((host, event.present(&lua, entry.split_once(':').unwrap().0)?))?;
            if failed.get() {
                return Err(mlua::Error::RuntimeError(
                    "player callback rejected a host operation".into(),
                ));
            }
            Ok(())
        } else {
            function.call::<()>(host)
        }
    })();
    if exceeded.get() || Instant::now() >= deadline {
        diagnostics.finish("execution_limit");
        return Err(format!("client startup {id}: execution limit exceeded"));
    }
    diagnostics.finish(if result.is_ok() {
        "evaluated"
    } else {
        "script_error"
    });
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
    for (owner, module) in &output.player_handlers {
        if state
            .player_handlers
            .insert(owner.clone(), module.clone())
            .is_some()
        {
            return Err(format!(
                "client startup {id}: duplicate player handler owner"
            ));
        }
    }
    state.texts.extend(output.texts.clone());
    state.states.extend(output.states.clone());
    if output.replica.is_some() {
        state.replica_observations = output.replica_observations;
    }
    state.replica = output.replica.clone().or_else(|| state.replica.take());
    state.parameters = output.parameters.clone();
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
