//! Lane-owned execution infrastructure and explicitly retained advisory realms.
use super::{Compiled, Diagnostics, Execution};
use crate::server::script::{
    Function, Limits, Lua, Program, ScriptError, ScriptFailure, SourceModule, imports::Imports,
};
use mlua::{RegistryKey, VmState};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};

const MAX_RESIDENT: usize = 256 * 1024 * 1024;
static RESIDENT: AtomicUsize = AtomicUsize::new(0);
static RUNTIMES: AtomicUsize = AtomicUsize::new(0);
static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(1);
/// A conservative reservation for a VM's entire allowed heap. Released on teardown.
pub(crate) struct Reservation {
    bytes: usize,
    id: u64,
}
impl Reservation {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn acquire(bytes: usize) -> mlua::Result<Self> {
        if bytes == 0
            || RUNTIMES
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                    (used < 64).then_some(used + 1)
                })
                .is_err()
        {
            return Err(mlua::Error::RuntimeError(
                "script runtime count admission limit exceeded".into(),
            ));
        }
        if RESIDENT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|n| *n <= MAX_RESIDENT)
            })
            .is_err()
        {
            RUNTIMES.fetch_sub(1, Ordering::AcqRel);
            return Err(mlua::Error::RuntimeError(
                "script runtime resident admission limit exceeded".into(),
            ));
        }
        Ok(Self {
            bytes,
            id: NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed),
        })
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        RESIDENT.fetch_sub(self.bytes, Ordering::AcqRel);
        RUNTIMES.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Engine {
    lua: Lua,
    diagnostics: Diagnostics,
    compiled: Rc<RefCell<Compiled>>,
    imports: Option<Rc<Imports>>,
    entry: Option<RegistryKey>,
    _reservation: Reservation,
    memory: usize,
}
impl Engine {
    fn new(id: &str, limits: Limits, execution: Execution) -> mlua::Result<Self> {
        let reservation = Reservation::acquire(limits.max_memory_bytes)?;
        let (lua, diagnostics) = super::create(id, execution)?;
        lua.set_memory_limit(limits.max_memory_bytes)?;
        Ok(Self {
            lua,
            diagnostics,
            compiled: Rc::default(),
            imports: None,
            entry: None,
            _reservation: reservation,
            memory: limits.max_memory_bytes,
        })
    }
    fn clear(&mut self) -> mlua::Result<()> {
        self.lua.remove_interrupt();
        self.entry.take();
        if let Some(imports) = self.imports.take() {
            imports.clear();
        }
        self.lua.expire_registry_values();
        self.lua.sandbox(false)?;
        self.lua.sandbox(true)?;
        self.lua.gc_collect()?;
        Ok(())
    }
    fn execute<T>(
        &mut self,
        program: &Program,
        limits: Limits,
        execution: Execution,
        retained: bool,
        invoke: impl FnOnce(&Lua, Function) -> mlua::Result<T>,
    ) -> Result<T, ScriptError> {
        let id = program.identity();
        let fail = |failure| ScriptError {
            module: id.clone(),
            failure,
        };
        if matches!(program, Program::Source(module) if module.source.len() > limits.max_source_bytes)
        {
            return Err(fail(ScriptFailure::SourceTooLarge));
        }
        let deadline = Instant::now() + limits.max_wall_time;
        let remaining = Rc::new(Cell::new(limits.max_interrupts));
        let exceeded = Rc::new(RefCell::new(None::<(bool, String)>));
        if self.entry.is_none() {
            self.imports = match program {
                Program::Package { snapshot, .. } => Some(Imports::new(
                    Arc::clone(snapshot),
                    limits.max_source_bytes,
                    Rc::clone(&self.compiled),
                )),
                _ => None,
            };
        }
        let active = self.imports.clone();
        let status = Rc::clone(&exceeded);
        let entry_id = id.clone();
        self.lua.set_interrupt(move |_| {
            let time = Instant::now() >= deadline;
            if time || remaining.get() == 0 {
                status.borrow_mut().get_or_insert_with(|| {
                    (
                        time,
                        active
                            .as_ref()
                            .and_then(|imports| imports.active())
                            .unwrap_or_else(|| entry_id.clone()),
                    )
                });
                return Err(mlua::Error::RuntimeError(
                    "script execution limit exceeded".into(),
                ));
            }
            remaining.set(remaining.get() - 1);
            Ok(VmState::Continue)
        });
        super::memory_begin(&self.lua);
        let result = (|| {
            let mut initialization = execution.clone();
            if retained {
                initialization.seed = 0;
            }
            super::begin(
                &self.lua,
                &self.diagnostics,
                &id,
                if self.entry.is_none() {
                    initialization
                } else {
                    execution.clone()
                },
            )?;
            let entry: Function = if let Some(key) = &self.entry {
                self.lua.registry_value(key)?
            } else {
                let entry = match program {
                    Program::Source(module) => {
                        let code = Compiled::get(&self.compiled, &id, &module.source)?;
                        self.lua
                            .load(code.as_slice())
                            .set_name(&id)
                            .set_mode(mlua::chunk::ChunkMode::Binary)
                            .eval::<Function>()?
                    }
                    Program::Package { entry, .. } => self.lua.unpack(
                        self.imports
                            .as_ref()
                            .expect("package imports")
                            .load(&self.lua, entry)?,
                    )?,
                };
                if retained {
                    self.entry = Some(self.lua.create_registry_value(entry.clone())?);
                    self.diagnostics.finish("initialized");
                    super::begin(&self.lua, &self.diagnostics, &id, execution)?;
                }
                entry
            };
            invoke(&self.lua, entry)
        })();
        self.lua.remove_interrupt();
        let result = if let Some((time, module)) = exceeded.take() {
            Err(ScriptError {
                module,
                failure: if time {
                    ScriptFailure::TimeLimit
                } else {
                    ScriptFailure::InstructionLimit
                },
            })
        } else if Instant::now() >= deadline {
            Err(fail(ScriptFailure::TimeLimit))
        } else if super::memory_exceeded(&self.lua) {
            Err(fail(ScriptFailure::Lua(
                "script memory limit exceeded".into(),
            )))
        } else {
            result.map_err(|error| {
                error
                    .downcast_ref::<ScriptError>()
                    .cloned()
                    .unwrap_or_else(|| fail(ScriptFailure::Lua(error.to_string())))
            })
        };
        self.diagnostics.finish(if result.is_ok() {
            "evaluated"
        } else {
            "script_error"
        });
        if retained && result.is_ok() {
            // Registry roots remain; temporary arguments and dead coroutine frames do not.
            self.lua.expire_registry_values();
            self.lua
                .gc_step()
                .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
            if self.lua.used_memory() > self.memory * 3 / 4 {
                self.lua
                    .gc_collect()
                    .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
            }
        }
        result
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.lua.remove_interrupt();
        if let Some(imports) = &self.imports {
            imports.clear();
        }
    }
}
thread_local! { static ISOLATED: RefCell<Option<Engine>> = const { RefCell::new(None) }; }
pub(in crate::server::script) fn isolated<T>(
    program: &Program,
    limits: Limits,
    execution: Execution,
    invoke: impl FnOnce(&Lua, Function) -> mlua::Result<T>,
) -> Result<T, ScriptError> {
    let current = program.current();
    let program = &current;
    let fail = |error: mlua::Error| ScriptError {
        module: program.identity(),
        failure: ScriptFailure::Lua(error.to_string()),
    };
    let mut engine = ISOLATED
        .with(|slot| slot.borrow_mut().take())
        .filter(|engine| engine.memory == limits.max_memory_bytes);
    if engine.is_none() {
        engine = Some(Engine::new(&program.identity(), limits, execution.clone()).map_err(fail)?);
    }
    let mut engine = engine.expect("created engine");
    let result = engine.execute(program, limits, execution, false, invoke);
    // A failed attempt is discarded, including uncertain allocation/interrupt state.
    if result.is_ok() {
        engine.clear().map_err(fail)?;
        ISOLATED.with(|slot| *slot.borrow_mut() = Some(engine));
    }
    result
}
/// One entry's serialized, worker-owned ephemeral state. Never used for decisions.
#[derive(Default)]
pub(crate) struct Retained {
    engine: Option<Engine>,
    identity: Option<(String, String)>,
    package: Option<std::sync::Weak<crate::server::script::package::PackageSnapshot>>,
    generation: u64,
    realm: String,
    execution: Option<Execution>,
}
impl Retained {
    pub(crate) fn run_source<T>(
        &mut self,
        module: &SourceModule,
        limits: Limits,
        execution: Execution,
        invoke: impl FnOnce(&Lua, Function) -> mlua::Result<T>,
    ) -> Result<T, ScriptError> {
        let identity = (module.id.clone(), module.source.clone());
        if self.identity.as_ref().is_some_and(|old| *old != identity) {
            self.reset("source_changed");
        }
        self.identity = Some(identity);
        self.run(
            &Program::Source(SourceModule {
                id: module.id.clone(),
                source: module.source.clone(),
            }),
            limits,
            execution,
            invoke,
        )
    }
    pub(in crate::server::script) fn run<T>(
        &mut self,
        program: &Program,
        limits: Limits,
        execution: Execution,
        invoke: impl FnOnce(&Lua, Function) -> mlua::Result<T>,
    ) -> Result<T, ScriptError> {
        let current = program.current();
        let program = &current;
        if let Program::Package { snapshot, .. } = program {
            if self
                .package
                .as_ref()
                .is_some_and(|old| !old.ptr_eq(&Arc::downgrade(snapshot)))
            {
                self.reset("package_reloaded");
            }
            self.package = Some(Arc::downgrade(snapshot));
        }
        self.realm = program.identity();
        self.execution = Some(execution.clone());
        if self
            .engine
            .as_ref()
            .is_some_and(|engine| engine.memory != limits.max_memory_bytes)
        {
            self.reset("memory_limit_changed");
        }
        if self.engine.is_none() {
            match Engine::new(&program.identity(), limits, execution.clone()) {
                Ok(engine) => {
                    tracing::debug!(module = %self.realm, side = execution.side, generation = self.generation, runtime_id = engine._reservation.id(), "script realm initialized");
                    self.engine = Some(engine);
                }
                Err(error) => {
                    return Err(ScriptError {
                        module: program.identity(),
                        failure: ScriptFailure::Lua(error.to_string()),
                    });
                }
            }
        }
        let result = self
            .engine
            .as_mut()
            .expect("created realm")
            .execute(program, limits, execution, true, invoke);
        if result.is_err() {
            self.reset("callback_failed");
        }
        result
    }
    fn reset(&mut self, reason: &'static str) {
        let runtime_id = self.engine.take().map(|engine| engine._reservation.id());
        self.generation = self.generation.saturating_add(1);
        tracing::warn!(?runtime_id, module = %self.realm, side = self.execution.as_ref().map_or("unknown", |e| e.side), callback = self.execution.as_ref().map_or("unknown", |e| e.kind), invocation = self.execution.as_ref().map_or("unknown", |e| e.correlation.as_str()), generation = self.generation, reason, "script realm reset");
    }
}
#[cfg(test)]
mod tests;
