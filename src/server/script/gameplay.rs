//! Gameplay decision adapter. All effects belong to the public Context overlay;
//! success is still only a candidate for the server's existing WAL transaction.
//! No script VM/callback or mutable module state survives an invocation.
//!
//! Startup capability: `requires bloxgloom:actions/v1`. Declare at most one
//! `h.register_action("demo:shift", 1, "Shift", "item", "bloxgloom:stick", "demo:shift")`.
//! Arguments are own action key, u16 revision, label, target kind/key, and an
//! own-package module. Target alternatives are `"block", key` and `"empty", nil`.
//! An optional seventh argument `{permission = "Player" | "Admin", arguments = {...}}`
//! adds a bounded typed command facet, only for empty targets. No aliases.
//! The module returns `function(context, event)`. Dot-call methods are
//! `block(x,y,z)`, `set_block(x,y,z,state_key)` and `transfer(from_slot,to_slot,count)`
//! (zero-based requesting-player slots, exact components, count 1..128). A false
//! transfer is unchanged, not a partially fulfilled request. Block reads return
//! the public Block fields; writes use exact i32 coordinates and host validation.
//! Event fields are kind/action/slot, binary arguments, and readonly 1-indexed
//! position/cell triples (cell nil for item/empty). Tick uses exact u32 lo/hi halves.
//!
//! Handler version fingerprints the revision and entire frozen installation,
//! conservatively including unrelated package sources. content.map rejects any
//! identity mismatch on restart; this is compatibility, not authenticity. There
//! is no state migration/hot reload.
//!
//! The same capability permits up to 32 exact decision owners per package:
//! `h.register_handler("demo:harvest", 1, "BlockRemoved", "bloxgloom:sand", "demo:harvest")`.
//! Events: BlockRemoved, BlockPlaced, NeighborChanged, EntityTick, PickupRequested
//! (target `bloxgloom:drop`). No fallback owners. EntityTick targets must be
//! own-package registered gameplay entities; `register_entity` supplies fixed-byte
//! schemas (see `entities`). Pickup supplies readonly `drops` entries with exact
//! entity ID halves and eligible counts; only the host chooses those candidates.
//! Event tables and nested blocks/triples are readonly; u64 IDs/random/ticks use
//! `_lo`/`_hi` u32 halves. See `bindings` and `inventory` for staged operations.
mod bindings;
mod declarations;
mod events;
mod inventory;

pub(super) use declarations::handler_declarer;

use super::values::{integer, text};
use super::{Invocation, Limits, Program, package::PackageSnapshot, startup::Pending};
use bloxgloom_host_api::{
    actions::{
        Action, Command, CommandArgument, CommandPermission, MAX_COMMAND_ARGUMENTS, Operation,
        Target,
    },
    gameplay::{Context, Error, Event, EventKind, Handler, HandlerRegistration},
};
use mlua::{Function, Lua, Value};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(super) struct Declaration {
    action: Action,
    module: String,
}
pub(super) type Registration = (Action, HandlerRegistration);

/// One own-package semantic decision and discoverable action per package.
/// This deliberately exposes no fallback owners or other gameplay event kinds.
pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(
        move |_,
              (key, revision, label, kind, target, module, command): (
            Value,
            Value,
            Value,
            Value,
            Value,
            Value,
            mlua::Variadic<Value>,
        )| {
            let mut pending = pending.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !snapshot.permits_actions(&namespace) {
                    return Err("register_action requires bloxgloom:actions/v1");
                }
                if pending.action.is_some() {
                    return Err("only one action per package is allowed");
                }
                let key = text(key)?;
                let Some((owner, local)) = key.split_once(':') else {
                    return Err("action key must be namespaced");
                };
                if owner != namespace || !super::package::manifest::identifier(local) {
                    return Err("action key must belong to the startup package");
                }
                let module = text(module)?;
                if module.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                    || snapshot.source(&module).is_none()
                {
                    return Err("action must name a declared module in its package");
                }
                let target = match text(kind)?.as_str() {
                    "empty" if target.is_nil() => Target::Empty,
                    "item" => Target::Item(text(target)?),
                    "block" => Target::Block(text(target)?),
                    // Entity operations/events are intentionally not bound yet.
                    _ => return Err("action target must be empty/nil, item/key or block/key"),
                };
                let action = Action {
                    key,
                    version: integer(revision, 1, i64::from(u16::MAX))? as u16,
                    label: text(label)?,
                    target,
                    operation: Operation::Gameplay,
                    panel: None,
                    command: command_declaration(command)?,
                };
                action.validate().map_err(|_| "invalid action contract")?;
                pending.action = Some(Declaration { action, module });
                Ok(())
            })();
            result.map_err(|error| {
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )
}

fn command_declaration(values: mlua::Variadic<Value>) -> Result<Option<Command>, &'static str> {
    let value = match values.as_slice() {
        [] | [Value::Nil] => return Ok(None),
        [value] => value,
        _ => return Err("register_action expects at most one command descriptor"),
    };
    let Value::Table(table) = value else {
        return Err("command descriptor must be a table");
    };
    if table.metatable().is_some() {
        return Err("command descriptor must not have a metatable");
    }
    // Inspect at most three entries; any third/unknown field is unsupported.
    // Raw iteration never executes caller-provided indexing metamethods.
    let mut permission = None;
    let mut arguments = Vec::new();
    for pair in table.pairs::<Value, Value>().take(3) {
        let (key, value) = pair.map_err(|_| "invalid command descriptor")?;
        match text(key)?.as_str() {
            "permission" => {
                permission = Some(match text(value)?.as_str() {
                    "Player" => CommandPermission::Player,
                    "Admin" => CommandPermission::Admin,
                    _ => return Err("command permission must be Player or Admin"),
                })
            }
            "arguments" => arguments = command_schema(value)?,
            _ => return Err("unsupported command descriptor field"),
        }
    }
    Ok(Some(Command {
        permission: permission.ok_or("command requires explicit permission")?,
        arguments,
    }))
}

fn command_schema(value: Value) -> Result<Vec<CommandArgument>, &'static str> {
    let Value::Table(table) = value else {
        return Err("command arguments must be an array");
    };
    if table.metatable().is_some() {
        return Err("command arguments must not have a metatable");
    }
    let mut ordered = vec![None; MAX_COMMAND_ARGUMENTS];
    let mut count = 0;
    for pair in table
        .pairs::<Value, Value>()
        .take(MAX_COMMAND_ARGUMENTS + 1)
    {
        let (index, value) = pair.map_err(|_| "invalid command arguments")?;
        let index = integer(index, 1, MAX_COMMAND_ARGUMENTS as i64)? as usize - 1;
        let Value::Table(field) = value else {
            return Err("command argument must be a table");
        };
        if field.metatable().is_some() {
            return Err("command argument must not have a metatable");
        }
        let mut kind = None;
        let mut max_bytes = None;
        let mut default = None;
        for pair in field.pairs::<Value, Value>().take(4) {
            let (key, value) = pair.map_err(|_| "invalid command argument")?;
            match text(key)?.as_str() {
                "kind" => kind = Some(text(value)?),
                "max_bytes" => max_bytes = Some(integer(value, 3, 128)? as u8),
                "default" => default = Some(integer(value, 1, 128)? as u8),
                _ => return Err("unsupported command argument field"),
            }
        }
        let argument = match kind.as_deref() {
            Some("item_key") if default.is_none() => CommandArgument::ItemKey {
                max_bytes: max_bytes.ok_or("key requires max_bytes")?,
            },
            Some("entity_key") if default.is_none() => CommandArgument::EntityKey {
                max_bytes: max_bytes.ok_or("key requires max_bytes")?,
            },
            Some("count") if max_bytes.is_none() => CommandArgument::Count { default },
            _ => return Err("invalid command argument kind or fields"),
        };
        ordered[index] = Some(argument);
        count += 1;
    }
    if count > MAX_COMMAND_ARGUMENTS || ordered[count..].iter().any(Option::is_some) {
        return Err("command arguments must be a dense bounded array");
    }
    ordered
        .into_iter()
        .take(count)
        .map(|value| value.ok_or("command arguments must be dense"))
        .collect()
}

pub(super) fn registration(
    snapshot: Arc<PackageSnapshot>,
    declaration: Declaration,
) -> Registration {
    let Declaration { action, module } = declaration;
    let handler = HandlerRegistration {
        key: action.key.clone(),
        version: snapshot.gameplay_version(&module, action.version),
        event: EventKind::ActionRequested,
        target: Some(action.key.clone()),
        handler: Arc::new(ScriptHandler { snapshot, module }),
    };
    (action, handler)
}

struct ScriptHandler {
    snapshot: Arc<PackageSnapshot>,
    module: String,
}

impl Handler for ScriptHandler {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        // Live commands/timers -> gameplay planner -> registered Context
        // dispatch on the server coordinator, never the client/window thread.
        let rejected = RefCell::new(None);
        let result = super::run_with(
            &Program::Package {
                snapshot: Arc::clone(&self.snapshot),
                entry: self.module.clone(),
                invocation: Invocation::Integer,
            },
            Limits::default(),
            |lua, entry| bindings::invoke(lua, entry, context, event, &rejected),
        );
        // Preserve Unavailable, including when caught by pcall: the host requests
        // missing terrain and retains retry eligibility. Never stringify it into
        // a terminal script failure, nor let a caught decode error publish effects.
        if let Some(error) = rejected.into_inner() {
            return Err(error);
        }
        result.map_err(|error| Error::Invalid(error.to_string()))
    }
}
