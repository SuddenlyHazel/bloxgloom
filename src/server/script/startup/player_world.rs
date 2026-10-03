//! Startup bindings for named regions and pure chat moderation.
use super::Pending;
use crate::server::script::{package::PackageSnapshot, values};
use mlua::{Function, Lua, Value};
use std::{cell::RefCell, rc::Rc, sync::Arc};
mod moderation;
fn own_key(value: Value, namespace: &str) -> Result<String, &'static str> {
    let key = values::text(value)?;
    if key
        .split_once(':')
        .is_none_or(|(owner, _)| owner != namespace)
    {
        return Err("declaration key must belong to its startup package");
    }
    Ok(key)
}
fn coordinate(value: Value) -> Result<[f32; 3], &'static str> {
    let Value::Table(table) = value else {
        return Err("region bounds must be coordinate triples");
    };
    if table.metatable().is_some() {
        return Err("region bounds cannot have a metatable");
    }
    let mut result = [0.; 3];
    let mut seen = [false; 3];
    for pair in table.pairs::<Value, Value>().take(4) {
        let (axis, value) = pair.map_err(|_| "invalid region bounds")?;
        let axis = values::integer(axis, 1, 3)? as usize - 1;
        let number = match value {
            Value::Number(n) => n,
            Value::Integer(n) => n as f64,
            _ => return Err("region bounds must be numbers"),
        };
        if !number.is_finite() || number.abs() > 1_000_000. {
            return Err("region bounds outside world limits");
        }
        result[axis] = number as f32;
        seen[axis] = true;
    }
    if seen != [true; 3] {
        return Err("region bounds require exactly three coordinates");
    }
    Ok(result)
}
pub(super) fn region(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(
        move |_, (key, minimum, maximum, service): (Value, Value, Value, Value)| {
            let mut pending = pending.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !snapshot.permits_players(&namespace) {
                    return Err("register_region requires bloxgloom:players/v1");
                }
                if pending.regions.len() >= 32 {
                    return Err("at most 32 regions per package");
                }
                let registration = bloxgloom_host_api::regions::Registration {
                    key: own_key(key, &namespace)?,
                    minimum: coordinate(minimum)?,
                    maximum: coordinate(maximum)?,
                    service: own_key(service, &namespace)?,
                };
                registration
                    .validate()
                    .map_err(|_| "invalid region declaration")?;
                if pending.regions.iter().any(|r| r.key == registration.key) {
                    return Err("duplicate region declaration");
                }
                pending.regions.push(registration);
                Ok(())
            })();
            result.map_err(|error| {
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )
}
pub(super) fn chat(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (key, revision, module): (Value, Value, Value)| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !snapshot.permits_players(&namespace) {
                return Err("register_chat_hook requires bloxgloom:players/v1");
            }
            if pending.chat_hooks.len() >= 8 {
                return Err("at most eight chat hooks per package");
            }
            let key = own_key(key, &namespace)?;
            let module = own_key(module, &namespace)?;
            if snapshot.source(&module).is_none() {
                return Err("chat hook requires an own-package module");
            }
            if pending.chat_hooks.iter().any(|r| r.key == key) {
                return Err("duplicate chat hook declaration");
            }
            let registration = bloxgloom_host_api::chat::Registration {
                key,
                revision: snapshot.gameplay_version(
                    &module,
                    values::integer(revision, 1, u16::MAX.into())? as u16,
                ),
                moderator: Arc::new(moderation::ScriptModerator {
                    snapshot: Arc::clone(&snapshot),
                    module,
                }),
            };
            registration.validate().map_err(|_| "invalid chat hook")?;
            pending.chat_hooks.push(registration);
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}
