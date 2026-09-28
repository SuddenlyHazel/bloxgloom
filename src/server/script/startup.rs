//! Startup-only adapter: bounded Luau declarations become a single public
//! extension bundle. No VM or partial catalog survives installation. Generation
//! and semantic action registrations retain immutable sources, not VM callbacks.
//!
//! Opt in via `server-packages <package-root> <address> <save-dir> [max-clients]`.
//! Add `requires bloxgloom:content/v1` to package.txt, then return an entry like:
//! `function(host) host.register_item("demo:token", "Token", "bloxgloom:stone") end`.
//! This host replaces integer inputs only for startup, not execute_package.
//!
//! Each package may declare 32 non-placeable sprite items, with builtin texture
//! references, a white swatch, no components and the ordinary 128 stack cap.
//! Keys must belong to the entry package; imported helpers may receive the
//! callback but gain only that entry's authority. No assets, grants or client
//! loading are provided. Every dependency entry runs too; library packages can
//! return a no-op entry and export helpers from other modules.
//! With `requires bloxgloom:generation/v1`, an entry may additionally declare one
//! `host.register_generator("demo:terrain", 1, "demo:terrain")`. The named own
//! module returns a function receiving a chunk context (see `generation`).
//! `bloxgloom:actions/v1` permits one `register_action` declaration (see `gameplay`).
//! `bloxgloom:owner_systems/v1` permits one persistent chunk `register_system`
//! declaration (see `system`); plans run on existing owner workers, not this worker.
//!
//! The bundle records identity:package with public contract version 1. Source
//! semver is checked exactly during discovery, not persisted or converted to
//! the public u32 version. Retry/restart rediscovers sources and uses fresh VMs;
//! failed declaration collection publishes nothing and never opens the world.
//! Generator execution failures are reported later by chunk loading. Existing catalog
//! identity/reconciliation and client compatibility checks remain authoritative.
use super::{Invocation, Output, Program, ScriptInput, ScriptWorker, package::PackageSnapshot};
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, content::Item};
use mlua::{Function, Lua, Value};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

const MAX_ITEMS_PER_PACKAGE: usize = 32;

pub(in crate::server) struct Declarations {
    packages: Vec<bloxgloom_host_api::composition::Package>,
    items: Vec<Item>,
    generation: Vec<bloxgloom_host_api::generation::Registration>,
    actions: Vec<super::gameplay::Registration>,
    systems: Vec<bloxgloom_host_api::system::System>,
}

impl Declarations {
    pub(in crate::server) fn discover(root: &Path) -> std::io::Result<Self> {
        let snapshot = Arc::new(PackageSnapshot::discover(root).map_err(std::io::Error::other)?);
        let packages = snapshot.startup_packages().map_err(std::io::Error::other)?;
        let worker = ScriptWorker::spawn(super::Limits::default())?;
        let mut items = Vec::new();
        let mut generation = Vec::new();
        let mut actions = Vec::new();
        let mut systems = Vec::new();
        // At most 64 packages * 32 items, in lexical package order. Each entry
        // gets a fresh VM; completion timing cannot affect assignment order.
        for package in &packages {
            let name = package.key.split_once(':').expect("package key").0;
            let entry = snapshot.entry(name).map_err(std::io::Error::other)?;
            let output = worker
                .submit(
                    Program::Package {
                        snapshot: Arc::clone(&snapshot),
                        entry,
                        invocation: Invocation::Startup,
                    },
                    ScriptInput { tick: 0, seed: 0 },
                )
                .map_err(std::io::Error::other)?;
            let Output::Declarations(declarations) = output else {
                unreachable!("startup execution")
            };
            items.extend(declarations.items);
            if let Some(system) = declarations.system {
                systems.push(system);
            }
            if let Some(declaration) = declarations.action {
                actions.push(super::gameplay::registration(
                    Arc::clone(&snapshot),
                    declaration,
                ));
            }
            if let Some(declaration) = declarations.generation {
                generation.push(super::generation::registration(
                    Arc::clone(&snapshot),
                    declaration,
                ));
            }
        }
        Ok(Self {
            packages,
            items,
            generation,
            actions,
            systems,
        })
    }
}

impl Extension for Declarations {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        for package in &self.packages {
            registrar.package(package.clone())?;
        }
        for item in &self.items {
            registrar.item(item.clone())?;
        }
        for generation in &self.generation {
            registrar.generation_contributor(generation.clone())?;
        }
        for (action, handler) in &self.actions {
            registrar.action(action.clone())?;
            registrar.gameplay_handler(handler.clone())?;
        }
        for system in &self.systems {
            registrar.owner_system(system.clone())?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Pending {
    items: Vec<Item>,
    pub(super) generation: Option<super::generation::Declaration>,
    pub(super) action: Option<super::gameplay::Declaration>,
    pub(super) system: Option<bloxgloom_host_api::system::System>,
    pub(super) error: Option<&'static str>,
}

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    namespace: &str,
    snapshot: &Arc<PackageSnapshot>,
) -> mlua::Result<Pending> {
    let permits_content = snapshot.permits_content(namespace);
    let pending = Rc::new(RefCell::new(Pending::default()));
    let capture = Rc::clone(&pending);
    let generation =
        super::generation::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let action =
        super::gameplay::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let system =
        super::system::declarer(lua, Rc::clone(&pending), namespace, Arc::clone(snapshot))?;
    let namespace = namespace.to_owned();
    let register = lua.create_function(move |_, (key, name, texture): (Value, Value, Value)| {
        let mut pending = capture.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !permits_content {
                return Err("register_item requires bloxgloom:content/v1");
            }
            if pending.items.len() >= MAX_ITEMS_PER_PACKAGE {
                return Err("startup item limit exceeded (32 per package)");
            }
            // Validate lengths before copying VM strings into host allocations.
            // Do not traverse Lua tables or invoke metamethods during decoding.
            let key = text(key)?;
            let name = text(name)?;
            let texture = text(texture)?;
            let Some((owner, local)) = key.split_once(':') else {
                return Err("item key must be namespaced");
            };
            if owner != namespace || !super::package::manifest::identifier(local) {
                return Err("item key must belong to the startup package namespace");
            }
            if !texture.starts_with("bloxgloom:") {
                return Err("startup items require an existing builtin texture");
            }
            if pending.items.iter().any(|item| item.key == key) {
                return Err("duplicate startup item");
            }
            pending.items.push(Item {
                key,
                name,
                texture,
                swatch: [1.0; 4],
                placeable: None,
                sprite: true,
                components: bloxgloom_host_api::content::Components::None,
            });
            Ok(())
        })();
        result.map_err(|error| {
            // pcall cannot turn a rejected host declaration into partial success.
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })?;
    let host = lua.create_table()?;
    host.set("register_item", register)?;
    host.set("register_generator", generation)?;
    host.set("register_action", action)?;
    host.set("register_system", system)?;
    host.set_readonly(true);
    entry.call::<()>(host)?;
    let mut pending = pending.borrow_mut();
    if let Some(error) = pending.error {
        return Err(mlua::Error::RuntimeError(error.into()));
    }
    Ok(std::mem::take(&mut *pending))
}

fn text(value: Value) -> Result<String, &'static str> {
    let Value::String(value) = value else {
        return Err("register_item expects three UTF-8 strings");
    };
    if value.as_bytes().is_empty() || value.as_bytes().len() > 255 {
        return Err("startup item strings must contain 1..=255 bytes");
    }
    value
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| "invalid UTF-8 in startup item")
}
