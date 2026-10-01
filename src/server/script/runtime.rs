//! Shared standard libraries and attempt-local tools; no VM lifetime policy.
use mlua::{Lua, LuaOptions, StdLib, Value};
pub(super) mod compiled;
mod diagnostics;
mod engine;
mod memory;
pub(crate) use super::Limits;
use compiled::Compiled;
pub(crate) use diagnostics::Diagnostics;
pub(super) use engine::isolated;
pub(crate) use engine::{Reservation, Retained};
pub(crate) use memory::{begin as memory_begin, exceeded as memory_exceeded};

#[derive(Clone)]
pub(crate) struct Execution {
    pub kind: &'static str,
    pub side: &'static str,
    pub seed: u64,
    pub correlation: String,
}
impl Execution {
    pub fn new(kind: &'static str, seed: u64, correlation: impl Into<String>) -> Self {
        Self {
            kind,
            side: "server",
            seed,
            correlation: correlation.into(),
        }
    }
    pub fn client(mut self) -> Self {
        self.side = "client";
        self
    }
}

/// Explicit length-delimited stable inputs; never use process-random HashMap hashing.
pub(crate) struct Seed(u64);
impl Seed {
    pub fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
    pub fn bytes(mut self, bytes: &[u8]) -> Self {
        for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
        self
    }
    pub fn word(self, word: u64) -> Self {
        self.bytes(&word.to_le_bytes())
    }
    pub fn finish(self) -> u64 {
        self.0
    }
}

pub(crate) fn create(id: &str, execution: Execution) -> mlua::Result<(Lua, Diagnostics)> {
    let libraries = StdLib::TABLE
        | StdLib::STRING
        | StdLib::MATH
        | StdLib::UTF8
        | StdLib::BIT
        | StdLib::BUFFER
        | StdLib::VECTOR
        | StdLib::INTEGER
        | StdLib::COROUTINE
        | StdLib::DEBUG
        | StdLib::OS;
    let lua = Lua::new_with(libraries, LuaOptions::default())?;
    for name in ["require", "gcinfo", "getfenv", "setfenv"] {
        lua.globals().set(name, Value::Nil)?;
    }
    // Luau debug only exposes info/traceback. OS clocks and local time/date
    // introduce uncaptured inputs; difftime is ordinary arithmetic.
    let os: mlua::Table = lua.globals().get("os")?;
    for name in ["clock", "date", "time"] {
        os.set(name, Value::Nil)?;
    }
    let math: mlua::Table = lua.globals().get("math")?;
    let randomseed: mlua::Function = math.get("randomseed")?;
    lua.set_app_data(TrustedSeed(lua.create_registry_value(randomseed)?));
    memory::install(&lua)?;
    reseed(&lua, id, execution.seed)?;
    let diagnostics = Diagnostics::install(&lua, id, execution)?;
    lua.sandbox(true)?;
    Ok((lua, diagnostics))
}

pub(crate) fn begin(
    lua: &Lua,
    diagnostics: &Diagnostics,
    id: &str,
    execution: Execution,
) -> mlua::Result<()> {
    reseed(lua, id, execution.seed)?;
    diagnostics.begin(id, execution);
    Ok(())
}

struct TrustedSeed(mlua::RegistryKey);
fn reseed(lua: &Lua, id: &str, seed: u64) -> mlua::Result<()> {
    let seed = Seed::new().word(seed).bytes(id.as_bytes()).finish();
    let randomseed: mlua::Function = lua.registry_value(
        &lua.app_data_ref::<TrustedSeed>()
            .expect("trusted RNG installed")
            .0,
    )?;
    // The bundled Luau seed API takes a signed 32-bit integer. Fold all host
    // bits before conversion; authors retain the standard randomseed API.
    randomseed.call::<()>((seed ^ (seed >> 32)) as u32 as i32)?;
    Ok(())
}

#[cfg(test)]
mod tests;
