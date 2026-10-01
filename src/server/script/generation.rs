//! One invocation per chunk, on the loader's own thread. Only immutable source
//! and registration data are shared; each call gets isolated, bounded Lua state
//! while its execution worker reuses the physical VM and compiled code.
//!
//! Entry: `host.register_generator("demo:terrain", 1, "demo:terrain")`, with
//! `requires bloxgloom:generation/v1` and a declared own-package terrain module.
//! That module returns `function(c) ... end`; dot-call methods are
//! `world_position(lx, ly, lz)`, `builtin_terrain_height(x, z)`,
//! `builtin_base_block(x, y, z)`, `random_at(x, y, z, salt?)` and
//! `set_block(lx, ly, lz, state_key)`. The host consumes its exact world seed;
//! random calls return deterministic [0,1) samples. Coordinates are exact
//! integers in public Context bounds.
//!
//! The existing persisted key/revision identity is authoritative, not a source
//! hash. Authors must bump revision for any algorithm/dependency/config change
//! and obey Contributor purity (including not using object identity or table
//! iteration order for decisions). Restart rediscovers sources; retries reuse
//! frozen sources and isolated attempts. Persistent failure does not fall back to air.
use super::values::{integer, text};
use super::{Invocation, Limits, Program, ScriptInput, package::PackageSnapshot, startup::Pending};
use bloxgloom_host_api::generation::{Context, Contributor, GenerationError, Output, Registration};
use mlua::{Function, Lua, Value};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(super) struct Declaration {
    key: String,
    revision: u32,
    module: String,
}

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (key, revision, module): (Value, Value, Value)| {
        let mut pending = pending.borrow_mut();
        let declaration = format!(
            "register_generator {}",
            super::startup::declaration_key(&key)
        );
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !snapshot.permits_generation(&namespace) {
                return Err("register_generator requires bloxgloom:generation/v1");
            }
            if pending.generation.len() >= super::capacity::GENERATORS_PER_PACKAGE {
                return Err("generators/package limit exceeded");
            }
            let key = text(key)?;
            let module = text(module)?;
            let Some((owner, local)) = key.split_once(':') else {
                return Err("generator key must be namespaced");
            };
            if owner != namespace || !super::package::manifest::identifier(local) {
                return Err("generator key must belong to the startup package");
            }
            if pending
                .generation
                .iter()
                .any(|generator| generator.key == key)
            {
                return Err("duplicate generator key in startup package");
            }
            // Require an own-package module; dependencies remain available via import.
            if module.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                || snapshot.source(&module).is_none()
            {
                return Err("generator must name a declared module in its package");
            }
            let revision = integer(revision, 1, i64::from(u32::MAX))? as u32;
            pending.generation.push(Declaration {
                key,
                revision,
                module,
            });
            Ok(())
        })();
        result.map_err(|error| pending.reject(error, &declaration))
    })
}

pub(super) fn registration(
    snapshot: Arc<PackageSnapshot>,
    declaration: Declaration,
) -> Registration {
    Registration {
        key: declaration.key,
        revision: declaration.revision,
        contributor: Arc::new(ScriptContributor {
            snapshot,
            module: declaration.module,
        }),
    }
}

struct ScriptContributor {
    snapshot: Arc<PackageSnapshot>,
    module: String,
}

impl Contributor for ScriptContributor {
    fn generate(&self, context: Context, output: &mut Output) -> Result<(), GenerationError> {
        // The production callers are server startup and chunk/edit workers,
        // never the window thread. No extra queue, thread, or global VM lock.
        let result = super::run(
            Program::Package {
                snapshot: Arc::clone(&self.snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Generation(context),
            },
            ScriptInput { tick: 0, seed: 0 },
            Limits::default(),
        )
        .map_err(|error| GenerationError::Contributor(error.to_string()))?;
        let super::Output::Generation(candidate) = result else {
            unreachable!("generation invocation")
        };
        // Publish only a successful whole invocation; Output already counted
        // repeated writes and rejected caught errors before returning here.
        for (index, state) in candidate.writes() {
            output.set(
                [
                    i32::from(index % 16),
                    i32::from(index / 256),
                    i32::from(index / 16 % 16),
                ],
                state,
            )?;
        }
        Ok(())
    }
}

pub(super) fn invoke(lua: &Lua, entry: Function, context: Context) -> mlua::Result<Output> {
    let host = lua.create_table()?;
    // The host consumes the exact world seed internally. Legacy seed/word
    // fields remain available to older modules; new random calls return [0,1).
    host.set("seed_lo", context.seed as u32)?;
    host.set("seed_hi", (context.seed >> 32) as u32)?;
    for (name, axis) in ["chunk_x", "chunk_y", "chunk_z"]
        .into_iter()
        .zip(context.chunk)
    {
        host.set(name, axis)?;
    }
    host.set(
        "world_position",
        lua.create_function(move |_, (x, y, z): (Value, Value, Value)| {
            let local = [integer(x, 0, 15), integer(y, 0, 15), integer(z, 0, 15)];
            let [x, y, z] = local.map(|v| v.map(|v| v as i32));
            let position = context
                .world_position([
                    x.map_err(runtime)?,
                    y.map_err(runtime)?,
                    z.map_err(runtime)?,
                ])
                .map_err(|e| runtime(format!("{e:?}")))?;
            Ok((position[0], position[1], position[2]))
        })?,
    )?;
    host.set(
        "builtin_terrain_height",
        lua.create_function(move |_, (x, z): (Value, Value)| {
            context
                .builtin_terrain_height(coordinate(x)?, coordinate(z)?)
                .map_err(|e| runtime(format!("{e:?}")))
        })?,
    )?;
    host.set(
        "builtin_base_block",
        lua.create_function(move |_, (x, y, z): (Value, Value, Value)| {
            context
                .builtin_base_block([coordinate(x)?, coordinate(y)?, coordinate(z)?])
                .map_err(|e| runtime(format!("{e:?}")))
        })?,
    )?;
    host.set(
        "random_at",
        lua.create_function(
            move |_, (x, y, z, lo, hi): (Value, Value, Value, Value, Value)| {
                let legacy = !hi.is_nil();
                let salt = if legacy {
                    integer(lo, 0, i64::from(u32::MAX)).map_err(runtime)? as u64
                        | ((integer(hi, 0, i64::from(u32::MAX)).map_err(runtime)? as u64) << 32)
                } else if lo.is_nil() {
                    0
                } else {
                    integer(lo, 0, i64::from(u32::MAX)).map_err(runtime)? as u64
                };
                let value =
                    context.random_at([coordinate(x)?, coordinate(y)?, coordinate(z)?], salt);
                Ok(if legacy {
                    (
                        Value::Integer((value as u32).into()),
                        Value::Integer(((value >> 32) as u32).into()),
                    )
                } else {
                    (
                        Value::Number(crate::server::script::handles::unit_random(value)),
                        Value::Nil,
                    )
                })
            },
        )?,
    )?;
    let output = Rc::new(RefCell::new(Output::default()));
    let rejected = Rc::new(RefCell::new(None));
    let writes = Rc::clone(&output);
    let failure = Rc::clone(&rejected);
    host.set(
        "set_block",
        lua.create_function(move |_, (x, y, z, key): (Value, Value, Value, Value)| {
            let result = (|| {
                if let Some(error) = failure.borrow().as_ref() {
                    return Err(runtime(error));
                }
                let local = [
                    integer(x, 0, 15).map_err(runtime)? as i32,
                    integer(y, 0, 15).map_err(runtime)? as i32,
                    integer(z, 0, 15).map_err(runtime)? as i32,
                ];
                let key = text(key).map_err(runtime)?;
                writes
                    .borrow_mut()
                    .set(local, &key)
                    .map_err(|e| runtime(format!("{e:?}")))
            })();
            if let Err(error) = &result {
                failure
                    .borrow_mut()
                    .get_or_insert_with(|| error.to_string());
            }
            result
        })?,
    )?;
    host.set_readonly(true);
    entry.call::<()>(host)?;
    if let Some(error) = rejected.take() {
        return Err(runtime(error));
    }
    let result = std::mem::take(&mut *output.borrow_mut());
    result.finish().map_err(|e| runtime(format!("{e:?}")))?;
    Ok(result)
}

fn runtime(message: impl ToString) -> mlua::Error {
    mlua::Error::RuntimeError(message.to_string())
}

fn coordinate(value: Value) -> mlua::Result<i64> {
    integer(
        value,
        i64::from(i32::MIN) * 16,
        i64::from(i32::MAX) * 16 + 15,
    )
    .map_err(runtime)
}
