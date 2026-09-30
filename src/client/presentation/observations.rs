//! Immutable snapshots of installed client replicas, never gameplay authority.
use mlua::{Lua, Table};

#[derive(Clone, Debug, Default)]
pub(crate) struct Observations {
    pub(crate) inventory: Option<InventoryView>,
    pub(crate) blocks: Vec<BlockView>,
    pub(crate) blocks_truncated: bool,
    pub(crate) world: Option<WorldView>,
    pub(crate) actions: Vec<ActionView>,
}
#[derive(Clone, Debug)]
pub(crate) struct InventoryView {
    pub(crate) revision: u64,
    pub(crate) slots: Vec<SlotView>,
}
#[derive(Clone, Debug)]
pub(crate) struct SlotView {
    pub(crate) slot: u8,
    pub(crate) stack: Option<StackView>,
}
#[derive(Clone, Debug)]
pub(crate) struct StackView {
    pub(crate) item: String,
    pub(crate) count: u16,
    pub(crate) components: Option<ComponentView>,
}
#[derive(Clone, Debug)]
pub(crate) struct ComponentView {
    pub(crate) version: u16,
    pub(crate) bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct BlockView {
    pub(crate) position: [i32; 3],
    pub(crate) state: String,
    pub(crate) version: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct WorldView {
    pub(crate) elapsed_ms: u64,
    pub(crate) cycle_ms: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct ActionView {
    pub(crate) id: u128,
    pub(crate) key: Option<String>,
    pub(crate) accepted: bool,
    pub(crate) reason: String,
}

impl Observations {
    pub(crate) fn validate(&self) -> mlua::Result<()> {
        let bad = super::invalid;
        if self.blocks.len() > 64 || self.actions.len() > 16 {
            return Err(bad());
        }
        if let Some(inventory) = &self.inventory {
            if inventory.slots.len() != 36 {
                return Err(bad());
            }
            for (index, slot) in inventory.slots.iter().enumerate() {
                if usize::from(slot.slot) != index {
                    return Err(bad());
                }
                if let Some(stack) = &slot.stack
                    && (!key(&stack.item)
                        || !(1..=128).contains(&stack.count)
                        || stack.components.as_ref().is_some_and(|c| {
                            c.version == 0 || c.bytes.is_empty() || c.bytes.len() > 1024
                        }))
                {
                    return Err(bad());
                }
            }
        }
        let mut positions = std::collections::BTreeSet::new();
        for block in &self.blocks {
            if !state_key(&block.state) || !positions.insert(block.position) {
                return Err(bad());
            }
        }
        if self.world.as_ref().is_some_and(|world| {
            world.cycle_ms != crate::daylight::CYCLE_MS || world.elapsed_ms >= world.cycle_ms
        }) {
            return Err(bad());
        }
        let mut actions = std::collections::BTreeSet::new();
        for action in &self.actions {
            if (action.id >> 64) == 0
                || action.id as u64 == 0
                || !actions.insert(action.id)
                || action.key.as_ref().is_some_and(|value| !key(value))
                || action.reason.len() > 32
            {
                return Err(bad());
            }
        }
        Ok(())
    }
    pub(crate) fn for_owner(&self, owner: &str) -> Self {
        let mut result = self.clone();
        result.actions.retain(|action| {
            action.key.as_ref().is_some_and(|key| {
                key.split_once(':')
                    .is_some_and(|(namespace, _)| namespace == owner)
            })
        });
        result
    }
    pub(crate) fn lua(&self, lua: &Lua) -> mlua::Result<Table> {
        self.validate()?;
        let output = lua.create_table()?;
        if let Some(inventory) = &self.inventory {
            let view = lua.create_table()?;
            revision(lua, &view, "revision", inventory.revision)?;
            let slots = lua.create_table()?;
            for slot in &inventory.slots {
                let entry = lua.create_table()?;
                entry.raw_set("slot", slot.slot)?;
                if let Some(stack) = &slot.stack {
                    let item = lua.create_table()?;
                    item.raw_set("item", stack.item.as_str())?;
                    item.raw_set("count", stack.count)?;
                    if let Some(component) = &stack.components {
                        let value = lua.create_table()?;
                        value.raw_set("version", component.version)?;
                        value.raw_set("bytes", lua.create_string(&component.bytes)?)?;
                        value.set_readonly(true);
                        item.raw_set("components", value)?;
                    }
                    item.set_readonly(true);
                    entry.raw_set("stack", item)?;
                }
                entry.set_readonly(true);
                slots.raw_set(usize::from(slot.slot) + 1, entry)?;
            }
            slots.set_readonly(true);
            view.raw_set("slots", slots)?;
            view.set_readonly(true);
            output.raw_set("inventory", view)?;
        }
        let blocks = lua.create_table()?;
        for (index, block) in self.blocks.iter().enumerate() {
            let view = lua.create_table()?;
            let position = lua.create_table()?;
            for (name, value) in ["x", "y", "z"].into_iter().zip(block.position) {
                position.raw_set(name, value)?;
            }
            position.set_readonly(true);
            view.raw_set("position", position)?;
            view.raw_set("state", block.state.as_str())?;
            revision(lua, &view, "revision", block.version)?;
            view.set_readonly(true);
            blocks.raw_set(index + 1, view)?;
        }
        blocks.set_readonly(true);
        output.raw_set("blocks", blocks)?;
        output.raw_set("blocks_truncated", self.blocks_truncated)?;
        if let Some(world) = &self.world {
            let view = lua.create_table()?;
            view.raw_set("elapsed_ms", world.elapsed_ms)?;
            view.raw_set("cycle_ms", world.cycle_ms)?;
            view.set_readonly(true);
            output.raw_set("world", view)?;
        }
        let actions = lua.create_table()?;
        for (index, action) in self.actions.iter().enumerate() {
            let view = lua.create_table()?;
            view.raw_set("id", format!("{:032x}", action.id))?;
            view.raw_set("key", action.key.as_deref())?;
            view.raw_set("accepted", action.accepted)?;
            view.raw_set("reason", action.reason.as_str())?;
            view.set_readonly(true);
            actions.raw_set(index + 1, view)?;
        }
        actions.set_readonly(true);
        output.raw_set("actions", actions)?;
        output.set_readonly(true);
        Ok(output)
    }
}
fn revision(lua: &Lua, table: &Table, name: &str, value: u64) -> mlua::Result<()> {
    table.raw_set(name, crate::server::script_handles::revision(lua, value)?)?;
    table.raw_set(format!("{name}_lo"), value as u32)?;
    table.raw_set(format!("{name}_hi"), (value >> 32) as u32)?;
    Ok(())
}
fn key(value: &str) -> bool {
    value.len() <= 194
        && value.split_once(':').is_some_and(|(namespace, local)| {
            [namespace, local].iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-/.".contains(&c)
                    })
            })
        })
}
fn state_key(value: &str) -> bool {
    if value.len() > 1024 {
        return false;
    }
    if let Some((base, properties)) = value.split_once('[') {
        key(base)
            && properties.strip_suffix(']').is_some_and(|properties| {
                !properties.is_empty()
                    && properties.split(',').all(|pair| {
                        pair.split_once('=').is_some_and(|(name, value)| {
                            [name, value].iter().all(|s| {
                                !s.is_empty()
                                    && s.bytes().all(|c| {
                                        c.is_ascii_lowercase()
                                            || c.is_ascii_digit()
                                            || b"_-".contains(&c)
                                    })
                            })
                        })
                    })
            })
    } else {
        key(value)
    }
}

#[cfg(test)]
mod tests;

impl InventoryView {
    pub(crate) fn from_inventory(
        inventory: &crate::inventory::Inventory,
        catalog: &crate::content::Catalog,
    ) -> std::result::Result<Self, String> {
        let slots = inventory
            .slots
            .iter()
            .enumerate()
            .map(|(slot, stack)| {
                let stack = stack
                    .as_ref()
                    .map(|stack| {
                        if !stack.valid_in(catalog) {
                            return Err("invalid installed inventory stack".to_owned());
                        }
                        Ok(StackView {
                            item: catalog
                                .item(stack.item)
                                .ok_or("unknown inventory item")?
                                .key
                                .to_string(),
                            count: stack.count,
                            components: stack.components.as_ref().map(|c| ComponentView {
                                version: c.version,
                                bytes: c.bytes.to_vec(),
                            }),
                        })
                    })
                    .transpose()?;
                Ok(SlotView {
                    slot: slot as u8,
                    stack,
                })
            })
            .collect::<std::result::Result<Vec<_>, String>>()?;
        Ok(Self {
            revision: inventory.revision,
            slots,
        })
    }
}
