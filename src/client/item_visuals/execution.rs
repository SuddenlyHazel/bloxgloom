//! Isolated read-only stack callbacks. No world, host mutation, I/O or GPU API.
use super::{ClientBundle, Key, Visual};
use crate::client::startup::{EventRealm, realm::load};
use mlua::{Value, VmState};
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) fn run(
    bundle: Arc<ClientBundle>,
    module: &str,
    item: &str,
    key: &Key,
) -> Result<Visual, String> {
    let realm = EventRealm::new(&bundle, module)?;
    let lua = &realm.lua;
    lua.set_memory_limit(8 * 1024 * 1024)
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_millis(20);
    let interrupts = Rc::new(Cell::new(2_000u32));
    let budget = Rc::clone(&interrupts);
    lua.set_interrupt(move |_| {
        if budget.get() == 0 || Instant::now() >= deadline {
            return Err(mlua::Error::RuntimeError(
                "item visual execution limit exceeded".into(),
            ));
        }
        budget.set(budget.get() - 1);
        Ok(VmState::Continue)
    });
    let result = (|| -> mlua::Result<Visual> {
        let function: mlua::Function = lua.unpack(load(
            lua,
            Arc::clone(&bundle),
            module,
            &realm.cached,
            &realm.stack,
        )?)?;
        let stack = lua.create_table()?;
        stack.set("item", item)?;
        stack.set("count", key.count)?;
        stack.set("component_version", key.version)?;
        // Exact opaque bytes; scripts opt into decoding their own public schema.
        stack.set("components", lua.create_string(&key.bytes)?)?;
        stack.set_readonly(true);
        let reply: Value = function.call(stack)?;
        let Value::Table(reply) = reply else {
            return Err(mlua::Error::RuntimeError(
                "item visual must return a table".into(),
            ));
        };
        crate::content::icons::script::fields(&reply, &["icon", "drop_scale"])?;
        let icon = match reply.raw_get::<Value>("icon")? {
            Value::Nil => None,
            value => Some(Arc::new(crate::content::icons::script::decode(
                item.into(),
                value,
            )?)),
        };
        let drop_scale = reply.raw_get::<Option<f32>>("drop_scale")?.unwrap_or(1.0);
        if !drop_scale.is_finite() || !(0.5..=1.5).contains(&drop_scale) {
            return Err(mlua::Error::RuntimeError(
                "item drop_scale must be 0.5..1.5".into(),
            ));
        }
        Ok(Visual { icon, drop_scale })
    })();
    lua.remove_interrupt();
    if Instant::now() >= deadline || crate::server::script_runtime::memory_exceeded(lua) {
        return Err("item visual execution or memory limit exceeded".into());
    }
    result.map_err(|error| error.to_string())
}
