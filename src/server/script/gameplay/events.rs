//! Owned readonly copies only. Never round u64 host identities through a double.
use bloxgloom_host_api::gameplay::{Block, Event, RemovalCause};
use mlua::{Lua, Table};

pub(super) fn block(lua: &Lua, block: &Block) -> mlua::Result<Table> {
    let value = lua.create_table()?;
    value.set("state", block.state.as_str())?;
    value.set("block_type", block.block_type.as_str())?;
    value.set("primary_item", block.primary_item.as_deref())?;
    value.set("plant", block.plant)?;
    value.set("supports_plant", block.supports_plant)?;
    value.set_readonly(true);
    Ok(value)
}

fn triple<T: mlua::IntoLua>(lua: &Lua, values: [T; 3]) -> mlua::Result<Table> {
    let value = lua.create_sequence_from(values)?;
    value.set_readonly(true);
    Ok(value)
}

pub(super) fn fields(lua: &Lua, event: &Event) -> mlua::Result<Table> {
    let fields = lua.create_table()?;
    match event {
        Event::ActionRequested {
            action,
            position,
            cell,
            entity,
            slot,
            arguments,
        } => {
            fields.set("kind", "ActionRequested")?;
            fields.set("action", action.as_str())?;
            fields.set("position", triple(lua, *position)?)?;
            fields.set("slot", *slot)?;
            fields.set("arguments", lua.create_string(arguments)?)?;
            if let Some(cell) = cell {
                fields.set("cell", triple(lua, *cell)?)?;
            }
            if let Some(entity) = entity {
                fields.set("entity_lo", *entity as u32)?;
                fields.set("entity_hi", (*entity >> 32) as u32)?;
            }
        }
        Event::BlockRemoved {
            cell,
            previous,
            cause,
            random,
        } => {
            fields.set("kind", "BlockRemoved")?;
            fields.set("cell", triple(lua, *cell)?)?;
            fields.set("previous", block(lua, previous)?)?;
            fields.set(
                "cause",
                match cause {
                    RemovalCause::Break => "Break",
                    RemovalCause::Replacement => "Replacement",
                    RemovalCause::SupportLoss => "SupportLoss",
                    RemovalCause::WorldEdit => "WorldEdit",
                    RemovalCause::Burn => "Burn",
                    RemovalCause::AnchoredBreak => "AnchoredBreak",
                },
            )?;
            fields.set("random_lo", *random as u32)?;
            fields.set("random_hi", (*random >> 32) as u32)?;
        }
        Event::BlockPlaced {
            cell,
            previous,
            placed,
        } => {
            fields.set("kind", "BlockPlaced")?;
            fields.set("cell", triple(lua, *cell)?)?;
            fields.set("previous", block(lua, previous)?)?;
            fields.set("placed", block(lua, placed)?)?;
        }
        Event::NeighborChanged {
            cell,
            changed,
            previous,
            current,
        } => {
            fields.set("kind", "NeighborChanged")?;
            fields.set("cell", triple(lua, *cell)?)?;
            fields.set("changed", triple(lua, *changed)?)?;
            fields.set("previous", block(lua, previous)?)?;
            fields.set("current", block(lua, current)?)?;
        }
        Event::EntityTick {
            entity,
            position,
            tick,
        } => {
            fields.set("kind", "EntityTick")?;
            fields.set("entity_lo", *entity as u32)?;
            fields.set("entity_hi", (*entity >> 32) as u32)?;
            fields.set("position", triple(lua, *position)?)?;
            fields.set("tick_lo", *tick as u32)?;
            fields.set("tick_hi", (*tick >> 32) as u32)?;
        }
        Event::PickupRequested { position, drops } => {
            // Production candidate admission already limits this to 32. Check
            // before allocating VM copies as well, rather than taking a prefix.
            if drops.len() > 32 {
                return Err(mlua::Error::RuntimeError(
                    "pickup candidate limit exceeded".into(),
                ));
            }
            fields.set("kind", "PickupRequested")?;
            fields.set("position", triple(lua, *position)?)?;
            let candidates = lua.create_table()?;
            for (index, &(id, count)) in drops.iter().enumerate() {
                let candidate = lua.create_table()?;
                candidate.set("entity_lo", id as u32)?;
                candidate.set("entity_hi", (id >> 32) as u32)?;
                candidate.set("count", count)?;
                candidate.set_readonly(true);
                candidates.raw_set(index + 1, candidate)?;
            }
            candidates.set_readonly(true);
            fields.set("drops", candidates)?;
        }
    }
    fields.set_readonly(true);
    Ok(fields)
}
