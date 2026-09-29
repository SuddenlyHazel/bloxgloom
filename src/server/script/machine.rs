//! Server-only Luau machine plans. Work is interpreted and committed by the
//! host machine scheduler; the downloaded catalog carries an inert behavior.
use super::{Invocation, Limits, Program, package::PackageSnapshot, run_with, values::integer};
use bloxgloom_host_api::{RegistrationError, machine as api};
use mlua::{Function, Lua, Value};
use std::sync::Arc;

pub(in crate::server::script) struct ScriptMachine {
    snapshot: Option<Arc<PackageSnapshot>>,
    module: String,
}

impl ScriptMachine {
    pub(in crate::server::script) fn server(
        snapshot: Arc<PackageSnapshot>,
        module: String,
    ) -> Self {
        Self {
            snapshot: Some(snapshot),
            module,
        }
    }

    pub(in crate::server::script) fn client() -> Self {
        Self {
            snapshot: None,
            module: String::new(),
        }
    }
}

impl api::Behavior for ScriptMachine {
    fn plan(&self, context: &api::Context<'_>) -> Result<api::Plan, RegistrationError> {
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or_else(|| RegistrationError("client machine cannot plan".into()))?;
        run_with(
            &Program::Package {
                snapshot: Arc::clone(snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            |lua, entry| invoke(lua, entry, context),
        )
        .map_err(|error| {
            RegistrationError(format!("machine {} plan rejected: {error}", self.module))
        })
    }
}

fn invoke(lua: &Lua, entry: Function, context: &api::Context<'_>) -> mlua::Result<api::Plan> {
    let input = lua.create_table()?;
    input.set("data", lua.create_string(context.data)?)?;
    input.set("tick_lo", context.tick as u32)?;
    input.set("tick_hi", (context.tick >> 32) as u32)?;
    input.set("due_lo", context.due as u32)?;
    input.set("due_hi", (context.due >> 32) as u32)?;
    input.set("fuel", context.fuel)?;
    input.set("progress", context.progress)?;
    let slots = lua.create_table()?;
    for (index, slot) in context.slots.iter().enumerate() {
        if let Some(slot) = slot {
            let value = lua.create_table()?;
            value.set("item", slot.item)?;
            value.set("count", slot.count)?;
            value.set("has_components", slot.has_components)?;
            value.set_readonly(true);
            slots.raw_set(index + 1, value)?;
        }
    }
    slots.set_readonly(true);
    input.set("slots", slots)?;
    input.set_readonly(true);
    let (data, delay, process): (Value, Value, Value) = entry.call(input)?;
    let Value::String(data) = data else {
        return Err(mlua::Error::RuntimeError(
            "machine data must be a binary string".into(),
        ));
    };
    if data.as_bytes().len() > 1024 {
        return Err(mlua::Error::RuntimeError(
            "machine data exceeds 1 KiB".into(),
        ));
    }
    let delay =
        integer(delay, 1, 60000).map_err(|error| mlua::Error::RuntimeError(error.into()))? as u64;
    let process = match process {
        Value::Boolean(value) => value,
        _ => {
            return Err(mlua::Error::RuntimeError(
                "machine process flag must be boolean".into(),
            ));
        }
    };
    Ok(api::Plan {
        data: data.as_bytes().to_vec(),
        next_tick: context
            .due
            .checked_add(delay)
            .ok_or_else(|| mlua::Error::RuntimeError("machine tick exhausted".into()))?,
        work: if process {
            vec![api::Work::Process]
        } else {
            vec![]
        },
    })
}
