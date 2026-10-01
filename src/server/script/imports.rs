//! A request-local module lifecycle: unseen -> loading -> cached value/error.
//! Registry keys (not Lua handles) avoid a VM/callback ownership cycle.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use mlua::{Lua, RegistryKey, Value};

use super::{ScriptError, ScriptFailure, package::PackageSnapshot};

const MAX_IMPORT_DEPTH: usize = 32;

enum ModuleState {
    Loading,
    Ready(RegistryKey),
    Failed(ScriptError),
}

pub(super) struct Imports {
    snapshot: Arc<PackageSnapshot>,
    modules: RefCell<BTreeMap<String, ModuleState>>,
    stack: RefCell<Vec<String>>,
    max_source_bytes: usize,
    compiled: Rc<RefCell<super::runtime::compiled::Compiled>>,
}

impl Imports {
    pub fn new(
        snapshot: Arc<PackageSnapshot>,
        max_source_bytes: usize,
        compiled: Rc<RefCell<super::runtime::compiled::Compiled>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            snapshot,
            modules: RefCell::new(BTreeMap::new()),
            stack: RefCell::new(Vec::new()),
            max_source_bytes,
            compiled,
        })
    }

    pub fn clear(&self) {
        self.modules.borrow_mut().clear();
    }

    pub fn active(&self) -> Option<String> {
        self.stack
            .borrow()
            .last()
            .map(|key| self.snapshot.identity(key))
    }

    pub fn load(self: &Rc<Self>, lua: &Lua, key: &str) -> mlua::Result<Value> {
        let id = self.snapshot.identity(key);
        let fail = |failure| ScriptError {
            module: id.clone(),
            failure,
        };
        match self.modules.borrow().get(key) {
            Some(ModuleState::Ready(value)) => return lua.registry_value(value),
            Some(ModuleState::Failed(error)) => return Err(mlua::Error::external(error.clone())),
            Some(ModuleState::Loading) => {
                return Err(mlua::Error::external(fail(ScriptFailure::Package(
                    "cyclic import".into(),
                ))));
            }
            None => {}
        }
        if self.stack.borrow().len() == MAX_IMPORT_DEPTH {
            return Err(mlua::Error::external(fail(ScriptFailure::Package(
                "import depth limit exceeded".into(),
            ))));
        }
        let source = self.snapshot.source(key).ok_or_else(|| {
            mlua::Error::external(fail(ScriptFailure::Package("unknown module".into())))
        })?;
        if source.len() > self.max_source_bytes {
            return Err(mlua::Error::external(fail(ScriptFailure::SourceTooLarge)));
        }
        self.modules
            .borrow_mut()
            .insert(key.to_owned(), ModuleState::Loading);
        self.stack.borrow_mut().push(key.to_owned());
        let result = (|| {
            let environment = lua.create_table()?;
            let meta = lua.create_table()?;
            meta.set("__index", lua.globals())?;
            meta.set("__metatable", false)?;
            environment.set_metatable(Some(meta))?;
            let imports = Rc::clone(self);
            let caller = key.to_owned();
            environment.set(
                "import",
                lua.create_function(move |lua, name: mlua::LuaString| {
                    // Check bytes before making a host-owned allocation.
                    if name.as_bytes().len() > 129 {
                        return Err(mlua::Error::external(super::package::error(
                            &imports.snapshot.identity(&caller),
                            "import name too long",
                        )));
                    }
                    let name = name.to_str()?;
                    let resolved = imports
                        .snapshot
                        .resolve(&caller, &name)
                        .map_err(mlua::Error::external)?;
                    imports.load(lua, &resolved)
                })?,
            )?;
            let code = super::runtime::compiled::Compiled::get(&self.compiled, &id, source)?;
            let value: Value = lua
                .load(code.as_slice())
                .set_name(&id)
                .set_mode(mlua::chunk::ChunkMode::Binary)
                .set_environment(environment)
                .eval()?;
            if value.is_nil() {
                return Err(mlua::Error::RuntimeError(
                    "module must return a non-nil export".into(),
                ));
            }
            let cached = lua.create_registry_value(value.clone())?;
            Ok((value, cached))
        })();
        self.stack.borrow_mut().pop();
        match result {
            Ok((value, cached)) => {
                self.modules
                    .borrow_mut()
                    .insert(key.to_owned(), ModuleState::Ready(cached));
                Ok(value)
            }
            Err(error) => {
                let error = error
                    .downcast_ref::<ScriptError>()
                    .cloned()
                    .unwrap_or_else(|| fail(ScriptFailure::Lua(error.to_string())));
                self.modules
                    .borrow_mut()
                    .insert(key.to_owned(), ModuleState::Failed(error.clone()));
                Err(mlua::Error::external(error))
            }
        }
    }
}
