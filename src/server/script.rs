//! Local Luau source execution foundation. Each worker owns its VM exclusively;
//! no Lua state, file handle, or host callback is exposed to scripts. This is
//! intentionally not connected to gameplay or module discovery yet.

use mlua::{Function, Lua, LuaOptions, StdLib, VmState};
use std::cell::Cell;
use std::fmt;
use std::io;
use std::rc::Rc;
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

#[derive(Debug, PartialEq, Eq)]
pub enum ScriptFailure {
    SourceTooLarge,
    InstructionLimit,
    TimeLimit,
    Lua(String),
    WorkerStopped,
}

/// Every failure names the local module responsible, including setup errors.
#[derive(Debug, PartialEq, Eq)]
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
    module: SourceModule,
    input: ScriptInput,
    reply: SyncSender<Result<i64, ScriptError>>,
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
                    let result = run(request.module, request.input, limits);
                    let _ = request.reply.send(result);
                }
            })?;
        Ok(Self {
            requests: Some(requests),
            thread: Some(thread),
        })
    }

    pub fn execute(&self, module: SourceModule, input: ScriptInput) -> Result<i64, ScriptError> {
        let id = module.id.clone();
        let (reply, receiver) = mpsc::sync_channel(1);
        self.requests
            .as_ref()
            .ok_or_else(|| ScriptError {
                module: id.clone(),
                failure: ScriptFailure::WorkerStopped,
            })?
            .send(Request {
                module,
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

fn run(module: SourceModule, input: ScriptInput, limits: Limits) -> Result<i64, ScriptError> {
    let id = module.id;
    let fail = |failure| ScriptError {
        module: id.clone(),
        failure,
    };
    if module.source.len() > limits.max_source_bytes {
        return Err(fail(ScriptFailure::SourceTooLarge));
    }

    // Deliberately omit OS, debug, and math (whose random state is not a host
    // input). Safe new_with disallows loading native C modules; Luau sandbox
    // makes globals read-only. Never expose a file loader or Rust I/O callback.
    let lua = Lua::new_with(StdLib::TABLE | StdLib::STRING, LuaOptions::default())
        .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
    // Luau's base library includes `require`, `print` (native stdout), and
    // `gcinfo` (VM-state-dependent), even without optional libraries.
    for name in ["require", "print", "gcinfo"] {
        lua.globals()
            .set(name, mlua::Value::Nil)
            .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
    }
    lua.sandbox(true)
        .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;
    lua.set_memory_limit(limits.max_memory_bytes)
        .map_err(|error| fail(ScriptFailure::Lua(error.to_string())))?;

    let deadline = Instant::now() + limits.max_wall_time;
    let remaining = Rc::new(Cell::new(limits.max_interrupts));
    let exceeded = Rc::new(Cell::new(None::<LimitExceeded>));
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
            status.set(Some(reason));
            return Err(mlua::Error::RuntimeError(
                "script execution limit exceeded".into(),
            ));
        }
        Ok(VmState::Continue)
    });

    let result = (|| -> mlua::Result<i64> {
        let entry: Function = lua.load(&module.source).set_name(&id).eval()?;
        let args = lua.create_table()?;
        args.set("tick", input.tick)?;
        args.set("seed", input.seed)?;
        entry.call(args)
    })();
    if let Some(reason) = exceeded.take() {
        return Err(fail(match reason {
            LimitExceeded::Time => ScriptFailure::TimeLimit,
            LimitExceeded::Instructions => ScriptFailure::InstructionLimit,
        }));
    }
    if Instant::now() >= deadline {
        return Err(fail(ScriptFailure::TimeLimit));
    }
    result.map_err(|error| fail(ScriptFailure::Lua(error.to_string())))
}

#[derive(Clone, Copy)]
enum LimitExceeded {
    Instructions,
    Time,
}

#[cfg(test)]
mod tests;
