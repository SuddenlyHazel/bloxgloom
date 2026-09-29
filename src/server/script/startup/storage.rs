//! Single-cell host-owned storage with a shared, negotiated inventory screen.
use super::*;
use crate::server::script::values::integer;
use bloxgloom_host_api::{
    InventoryScreen, SlotGroup,
    lifecycle::{FootprintCell, StorageBlockEntity},
};

#[derive(Clone, Debug)]
pub(in crate::server::script) struct Declaration {
    pub(in crate::server::script) storage: StorageBlockEntity,
    pub(in crate::server::script) screen: InventoryScreen,
}

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(
        move |_,
              (entity, block, title, slots, columns, options): (
            Value,
            Value,
            Value,
            Value,
            Value,
            Value,
        )| {
            let mut pending = pending.borrow_mut();
            let result = (|| {
                if let Some(error) = pending.error {
                    return Err(error);
                }
                if !snapshot.permits_storage_screens(&namespace) {
                    return Err(
                        "register_storage requires content, storage and inventory_screens/v1",
                    );
                }
                if pending.storage.len() >= 8 {
                    return Err("storage declaration limit exceeded");
                }
                let entity = text(entity)?;
                let block = text(block)?;
                for key in [&entity, &block] {
                    if key.split_once(':').is_none_or(|(owner, local)| {
                        owner != namespace || !super::super::package::manifest::identifier(local)
                    }) {
                        return Err("storage keys must belong to the package");
                    }
                }
                if pending
                    .storage
                    .iter()
                    .any(|old| old.storage.entity == entity || old.storage.block == block)
                {
                    return Err("duplicate storage entity or block");
                }
                let block_def = pending
                    .blocks
                    .iter()
                    .find(|definition| definition.key == block)
                    .ok_or("storage needs a previously registered block")?;
                let state = super::placement_state(block_def);
                let title = text(title)?;
                let slots = integer(slots, 1, 54)? as u8;
                let columns = integer(columns, 1, 9)? as u8;
                let storage = StorageBlockEntity {
                    entity: entity.clone(),
                    block: block.clone(),
                    placement_item: block.clone(),
                    anchor_state: state.clone(),
                    footprint: vec![FootprintCell {
                        offset: [0; 3],
                        state,
                    }],
                    slots: usize::from(slots),
                    automation_faces: None,
                };
                let mut screen =
                    InventoryScreen::storage(&entity, &block, &title, slots, columns, vec![[0; 3]]);
                if !options.is_nil() {
                    let Value::Table(options) = options else {
                        return Err("storage screen options must be a table");
                    };
                    let hint: Value = options.raw_get("hint").map_err(|_| "invalid screen hint")?;
                    if !hint.is_nil() {
                        screen.hint = text(hint)?;
                    }
                    let groups: Value = options
                        .raw_get("groups")
                        .map_err(|_| "invalid screen groups")?;
                    if !groups.is_nil() {
                        let Value::Table(groups) = groups else {
                            return Err("storage groups must be a sequence");
                        };
                        let count = groups.raw_len();
                        if count == 0 || count > usize::from(slots) {
                            return Err("invalid storage group count");
                        }
                        let mut seen = 0;
                        for pair in groups.clone().pairs::<Value, Value>().take(count + 1) {
                            let (key, _) = pair.map_err(|_| "invalid storage groups")?;
                            seen += 1;
                            if !matches!(key, Value::Integer(i) if i > 0 && i as usize <= count) {
                                return Err("storage groups must be a dense sequence");
                            }
                        }
                        if seen != count {
                            return Err("storage groups must be a dense sequence");
                        }
                        let mut first = 0u8;
                        let mut parsed = Vec::with_capacity(count);
                        for index in 1..=count {
                            let group: mlua::Table =
                                groups.raw_get(index).map_err(|_| "invalid storage group")?;
                            let label: Value = group
                                .raw_get("label")
                                .map_err(|_| "invalid storage group label")?;
                            let label = text(label)?;
                            let size: Value = group
                                .raw_get("count")
                                .map_err(|_| "invalid storage group size")?;
                            let size = integer(size, 1, i64::from(slots))? as u8;
                            first = first.checked_add(size).ok_or("storage group overflow")?;
                            if first > slots {
                                return Err("storage groups exceed slots");
                            }
                            parsed.push(SlotGroup {
                                label,
                                first: first - size,
                                count: size,
                                insert: true,
                                extract: true,
                            });
                        }
                        if first != slots {
                            return Err("storage groups must cover every slot");
                        }
                        screen.groups = parsed;
                    }
                }
                storage
                    .validate()
                    .map_err(|_| "invalid storage declaration")?;
                screen.validate().map_err(|_| "invalid storage screen")?;
                pending.storage.push(Declaration { storage, screen });
                Ok(())
            })();
            result.map_err(|error| {
                pending.error.get_or_insert(error);
                mlua::Error::RuntimeError(error.into())
            })
        },
    )
}
