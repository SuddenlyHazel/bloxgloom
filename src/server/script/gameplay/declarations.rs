//! Startup collection only; the catalog resolves target types and duplicate
//! decision ownership atomically before save I/O.
use super::*;

pub(in crate::server::script) fn handler_declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(
        move |_, (key, revision, event, target, module): (Value, Value, Value, Value, Value)| {
            let mut pending = pending.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !snapshot.permits_actions(&namespace) {
                    return Err("register_handler requires bloxgloom:actions/v1");
                }
                if pending.handlers.len() >= 32 {
                    return Err("gameplay handler limit exceeded (32 per package)");
                }
                let key = text(key)?;
                if key.split_once(':').is_none_or(|(owner, local)| {
                    owner != namespace || !super::super::package::manifest::identifier(local)
                }) {
                    return Err("handler key must belong to the startup package");
                }
                let module = text(module)?;
                if module.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                    || snapshot.source(&module).is_none()
                {
                    return Err("handler must name a declared module in its package");
                }
                let event = match text(event)?.as_str() {
                    "BlockRemoved" => EventKind::BlockRemoved,
                    "BlockPlaced" => EventKind::BlockPlaced,
                    "NeighborChanged" => EventKind::NeighborChanged,
                    "EntityTick" => EventKind::EntityTick,
                    "PickupRequested" => EventKind::PickupRequested,
                    _ => return Err("unsupported gameplay event"),
                };
                let target = text(target)?;
                if event == EventKind::EntityTick
                    && target.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                {
                    return Err("EntityTick target must belong to the startup package");
                }
                let revision = integer(revision, 1, u16::MAX.into())? as u16;
                let handler = HandlerRegistration {
                    key,
                    version: snapshot.gameplay_version(&module, revision),
                    event,
                    target: Some(target),
                    handler: Arc::new(ScriptHandler {
                        snapshot: Arc::clone(&snapshot),
                        module,
                        command: None,
                    }),
                };
                handler.validate().map_err(|_| "invalid handler contract")?;
                if pending.handlers.iter().any(|old| {
                    old.key == handler.key
                        || (old.event == handler.event && old.target == handler.target)
                }) {
                    return Err("duplicate gameplay handler key/decision owner");
                }
                pending.handlers.push(handler);
                Ok(())
            })();
            result.map_err(|error| {
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )
}
