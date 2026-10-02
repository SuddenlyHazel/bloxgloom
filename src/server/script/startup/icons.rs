//! Startup-owned bitmap art using the same native item-icon contract.
use super::*;
pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    permitted: bool,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (item, value): (Value, Value)| {
        let mut pending = pending.borrow_mut();
        let declaration = format!("register_item_icon {}", declaration_key(&item));
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !permitted {
                return Err("register_item_icon requires bloxgloom:content/v1");
            }
            let item = text(item)?;
            if item.split_once(':').is_none_or(|(owner, local)| {
                owner != namespace || !super::super::package::manifest::identifier(local)
            }) {
                return Err("item icon must belong to the startup package");
            }
            if pending.icons.len() >= 32 || pending.icons.iter().any(|icon| icon.item == item) {
                return Err("duplicate item icon or limit exceeded (32 per package)");
            }
            let icon = crate::content::icons::script::decode(item, value)
                .map_err(|_| "invalid item icon")?;
            pending.reserve_content(1, 2048, &icon.item)?;
            pending.icons.push(icon);
            Ok(())
        })();
        result.map_err(|error| pending.reject(error, &declaration))
    })
}

#[cfg(test)]
mod tests;
