//! Local Luau source execution foundation. Each worker owns its VM exclusively;
//! no live world state or file handle is exposed to scripts. Startup may collect
//! bounded public declarations. Gameplay borrows only a staged public Context.

pub(in crate::server::script) mod creature;
mod entities;
mod gameplay;
mod generation;
pub(crate) mod handles;
mod imports;
pub(in crate::server::script) mod machine;
pub mod package;
pub(super) mod startup;
mod system;
mod values;

use mlua::{Function, Lua, LuaOptions, StdLib, VmState};
use std::cell::{Cell, RefCell};
use std::fmt;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// An already-loaded local source module. Discovery and file reads belong to
/// the host, not the VM. A chunk must return a function accepting one input.
pub struct SourceModule {
    pub id: String,
    pub source: String,
}

/// Only host-provided integer data enters the VM; no clock or OS RNG is given.
#[derive(Clone, Copy)]
pub struct ScriptInput {
    pub tick: i64,
    pub seed: i64,
}

/// Per-execution ceilings. Interrupts are periodic Luau instruction checks,
/// not an exact count of individual bytecode instructions. Wall time is checked
/// at those same safe points, not by killing a thread mid-instruction.
#[derive(Clone, Copy)]
pub struct Limits {
    pub max_source_bytes: usize,
    pub max_interrupts: u64,
    pub max_wall_time: Duration,
    pub max_memory_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_source_bytes: 64 * 1024,
            max_interrupts: 10_000,
            max_wall_time: Duration::from_millis(50),
            max_memory_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptFailure {
    SourceTooLarge,
    InstructionLimit,
    TimeLimit,
    Lua(String),
    Package(String),
    WorkerStopped,
}

/// Every failure names the local module responsible, including setup errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub module: String,
    pub failure: ScriptFailure,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "script {:?}: {:?}", self.module, self.failure)
    }
}

impl std::error::Error for ScriptError {}

struct Request {
    program: Program,
    input: ScriptInput,
    reply: SyncSender<Result<Output, ScriptError>>,
}

enum Output {
    Integer(i64),
    Declarations(Box<startup::Pending>),
    Generation(bloxgloom_host_api::generation::Output),
}

enum Program {
    Source(SourceModule),
    Package {
        snapshot: Arc<package::PackageSnapshot>,
        entry: String,
        invocation: Invocation,
    },
}

enum Invocation {
    Integer,
    Startup,
    Generation(bloxgloom_host_api::generation::Context),
}

impl Program {
    fn identity(&self) -> String {
        match self {
            Self::Source(module) => module.id.clone(),
            Self::Package {
                snapshot, entry, ..
            } => snapshot.identity(entry),
        }
    }
}

/// Dedicated worker; calls block awaiting their reply and must be submitted
/// off the window thread. The bounded queue has one pending request. Each run
/// creates a fresh sandboxed VM so previous runs cannot influence new inputs.
pub struct ScriptWorker {
    requests: Option<SyncSender<Request>>,
    thread: Option<JoinHandle<()>>,
}

impl ScriptWorker {
    pub fn spawn(limits: Limits) -> io::Result<Self> {
        if limits.max_source_bytes == 0
            || limits.max_interrupts == 0
            || limits.max_wall_time.is_zero()
            || limits.max_wall_time > Duration::from_secs(60)
            || limits.max_memory_bytes == 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "zero script limit",
            ));
        }
        let (requests, receiver) = mpsc::sync_channel::<Request>(1);
        let thread = thread::Builder::new()
            .name("luau-script".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let result = run(request.program, request.input, limits);
                    let _ = request.reply.send(result);
                }
            })?;
        Ok(Self {
            requests: Some(requests),
            thread: Some(thread),
        })
    }

    pub fn execute(&self, module: SourceModule, input: ScriptInput) -> Result<i64, ScriptError> {
        let Output::Integer(value) = self.submit(Program::Source(module), input)? else {
            unreachable!("integer execution")
        };
        Ok(value)
    }

    /// Execute an entry from an immutable snapshot. The same Arc may be reused
    /// for retries; globals and module exports are never reused between calls.
    pub fn execute_package(
        &self,
        snapshot: Arc<package::PackageSnapshot>,
        package: &str,
        input: ScriptInput,
    ) -> Result<i64, ScriptError> {
        let entry = snapshot.entry(package)?;
        let Output::Integer(value) = self.submit(
            Program::Package {
                snapshot,
                entry,
                invocation: Invocation::Integer,
            },
            input,
        )?
        else {
            unreachable!("integer execution")
        };
        Ok(value)
    }

    fn submit(&self, program: Program, input: ScriptInput) -> Result<Output, ScriptError> {
        let id = program.identity();
        let (reply, receiver) = mpsc::sync_channel(1);
        self.requests
            .as_ref()
            .ok_or_else(|| ScriptError {
                module: id.clone(),
                failure: ScriptFailure::WorkerStopped,
            })?
            .send(Request {
                program,
                input,
                reply,
            })
            .map_err(|_| ScriptError {
                module: id.clone(),
                failure: ScriptFailure::WorkerStopped,
            })?;
        receiver.recv().map_err(|_| ScriptError {
            module: id,
            failure: ScriptFailure::WorkerStopped,
        })?
    }
}

impl Drop for ScriptWorker {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(program: Program, input: ScriptInput, limits: Limits) -> Result<Output, ScriptError> {
    run_with(&program, limits, |lua, entry| {
        if let Program::Package {
            snapshot,
            entry: key,
            invocation: Invocation::Startup,
        } = &program
        {
            let package = key.split_once(':').expect("validated entry").0;
            return startup::invoke(lua, entry, package, snapshot)
                .map(Box::new)
                .map(Output::Declarations);
        }
        if let Program::Package {
            invocation: Invocation::Generation(context),
            ..
        } = &program
        {
            return generation::invoke(lua, entry, *context).map(Output::Generation);
        }
        let args = lua.create_table()?;
        args.set("tick", input.tick)?;
        args.set("seed", input.seed)?;
        entry.call(args).map(Output::Integer)
    })
}

/// The VM never escapes this call. Gameplay invokes this on the authoritative
/// coordinator; owner systems invoke it on existing owner workers. Both borrow
/// their public Context via mlua::scope rather than a shared VM or extra queue.
fn run_with<T>(
    program: &Program,
    limits: Limits,
    invoke: impl FnOnce(&Lua, Function) -> mlua::Result<T>,
) -> Result<T, ScriptError> {
    let id = program.identity();
    let fail = |failure| ScriptError {
        module: id.clone(),
        failure,
    };
    if matches!(&program, Program::Source(module) if module.source.len() > limits.max_source_bytes)
    {
        return Err(fail(ScriptFailure::SourceTooLarge));
    }

    // Deliberately omit OS, debug, and math (whose random state is not a host
    // input). Safe new_with disallows loading native C modules; Luau sandbox
    // makes globals read-only. Never expose a file loader or Rust I/O callback.
    let lua = Lua::new_with(StdLib::TABLE | StdLib::STRING, LuaOptions::default())
        .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
    // Luau's base library includes `require`, `print` (native stdout), and
    // `gcinfo` (VM-state-dependent), even without optional libraries.
    for name in ["require", "print", "gcinfo", "getfenv", "setfenv"] {
        lua.globals()
            .set(name, mlua::Value::Nil)
            .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
    }
    lua.sandbox(true)
        .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
    lua.set_memory_limit(limits.max_memory_bytes)
        .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;

    let deadline = Instant::now() + limits.max_wall_time;
    let imports = match &program {
        Program::Package { snapshot, .. } => Some(imports::Imports::new(
            Arc::clone(snapshot),
            limits.max_source_bytes,
        )),
        Program::Source(_) => None,
    };
    let active_imports = imports.clone();
    let entry_id = id.clone();
    let remaining = Rc::new(Cell::new(limits.max_interrupts));
    let exceeded = Rc::new(RefCell::new(None::<(LimitExceeded, String)>));
    let status = Rc::clone(&exceeded);
    lua.set_interrupt(move |_| {
        let reason = if Instant::now() >= deadline {
            Some(LimitExceeded::Time)
        } else if remaining.get() == 0 {
            Some(LimitExceeded::Instructions)
        } else {
            remaining.set(remaining.get() - 1);
            None
        };
        if let Some(reason) = reason {
            let mut status = status.borrow_mut();
            if status.is_none() {
                *status = Some((
                    reason,
                    active_imports
                        .as_ref()
                        .and_then(|imports| imports.active())
                        .unwrap_or_else(|| entry_id.clone()),
                ));
            }
            return Err(mlua::Error::RuntimeError(
                "script execution limit exceeded".into(),
            ));
        }
        Ok(VmState::Continue)
    });

    let result = (|| -> mlua::Result<T> {
        let entry: Function = match &program {
            Program::Source(module) => lua
                .load(&module.source)
                .set_name(&id)
                .set_mode(mlua::chunk::ChunkMode::Text)
                .eval()?,
            Program::Package { entry, .. } => {
                let value = imports
                    .as_ref()
                    .expect("package imports")
                    .load(&lua, entry)?;
                lua.unpack(value)?
            }
        };
        invoke(&lua, entry)
    })();
    if let Some((reason, module)) = exceeded.take() {
        return Err(ScriptError {
            module,
            failure: match reason {
                LimitExceeded::Time => ScriptFailure::TimeLimit,
                LimitExceeded::Instructions => ScriptFailure::InstructionLimit,
            },
        });
    }
    if Instant::now() >= deadline {
        return Err(fail(ScriptFailure::TimeLimit));
    }
    result.map_err(|error| {
        error
            .downcast_ref::<ScriptError>()
            .cloned()
            .unwrap_or_else(|| fail(ScriptFailure::Lua(error.to_string())))
    })
}

/// Client presentation uses the same isolated, instruction/memory/time-bounded
/// source sandbox, with its own closed input/output adapter and worker.
pub(crate) fn run_presentation<T>(
    module: SourceModule,
    invoke: impl FnOnce(&Lua, Function) -> mlua::Result<T>,
) -> Result<T, ScriptError> {
    run_with(&Program::Source(module), Limits::default(), invoke)
}

#[derive(Clone, Copy)]
enum LimitExceeded {
    Instructions,
    Time,
}

#[cfg(test)]
mod tests;
