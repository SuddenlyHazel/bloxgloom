//! Package-owned health declarations. Callback source is server-only.
use super::Pending;
use crate::server::script::{package::PackageSnapshot, values};
use mlua::{Function, Lua, Value};
use std::{cell::RefCell, rc::Rc, sync::Arc};
mod callbacks;
pub(super) fn declare(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
    hook: bool,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (key, revision, module): (Value, Value, Value)| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !snapshot.permits_players(&namespace) {
                return Err("health declarations require bloxgloom:players/v1");
            }
            let key = values::text(key)?;
            let module = values::text(module)?;
            if key.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                || module.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                || snapshot.source(&module).is_none()
            {
                return Err("health declaration and module must belong to package");
            }
            let revision = snapshot.gameplay_version(
                &module,
                values::integer(revision, 1, u16::MAX.into())? as u16,
            );
            let callback = Arc::new(callbacks::ScriptCallback {
                snapshot: Arc::clone(&snapshot),
                module,
            });
            if hook {
                if pending.health_hooks.len() >= 8
                    || pending.health_hooks.iter().any(|r| r.key == key)
                {
                    return Err("duplicate or over-limit health hook");
                }
                let value = bloxgloom_host_api::player_health::HookRegistration {
                    key,
                    revision,
                    hook: callback,
                };
                value.validate().map_err(|_| "invalid health hook")?;
                pending.health_hooks.push(value);
            } else {
                if pending.damage_policies.len() >= 8
                    || pending.damage_policies.iter().any(|r| r.key == key)
                {
                    return Err("duplicate or over-limit damage policy");
                }
                let value = bloxgloom_host_api::player_health::DamageRegistration {
                    key,
                    revision,
                    policy: callback,
                };
                value.validate().map_err(|_| "invalid damage policy")?;
                pending.damage_policies.push(value);
            }
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}
