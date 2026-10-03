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

pub(in crate::server::script) fn motion(
    lua: &Lua,
    motion: &bloxgloom_host_api::motion::Motion,
) -> mlua::Result<Table> {
    let value = lua.create_table()?;
    value.set("position", triple(lua, motion.position)?)?;
    value.set("velocity", triple(lua, motion.velocity)?)?;
    value.set("acceleration", triple(lua, motion.acceleration)?)?;
    value.set("angular_velocity", triple(lua, motion.angular_velocity)?)?;
    let orientation = lua.create_sequence_from(motion.orientation)?;
    orientation.set_readonly(true);
    value.set("orientation", orientation)?;
    value.set(
        "revision",
        crate::server::script::handles::revision(lua, motion.revision)?,
    )?;
    value.set("grounded", motion.grounded)?;
    value.set_readonly(true);
    Ok(value)
}

fn moving_target(lua: &Lua, target: &bloxgloom_host_api::motion::Target) -> mlua::Result<Table> {
    let value = lua.create_table()?;
    match target {
        bloxgloom_host_api::motion::Target::Terrain { cell, state } => {
            value.set("kind", "Terrain")?;
            value.set("cell", triple(lua, *cell)?)?;
            value.set("state", state.as_str())?;
        }
        bloxgloom_host_api::motion::Target::Entity { id, revision } => {
            value.set("kind", "Entity")?;
            value.set("entity", crate::server::script::handles::entity(lua, *id)?)?;
            value.set(
                "revision",
                crate::server::script::handles::revision(lua, *revision)?,
            )?;
        }
    }
    value.set_readonly(true);
    Ok(value)
}
pub(in crate::server::script) fn motion_contact(
    lua: &Lua,
    contact: &bloxgloom_host_api::motion::MotionContact,
) -> mlua::Result<Table> {
    let value = lua.create_table()?;
    value.set(
        "motion_revision",
        crate::server::script::handles::revision(lua, contact.motion_revision)?,
    )?;
    value.set(
        "tick",
        crate::server::script::handles::tick(lua, contact.tick)?,
    )?;
    value.set("target", moving_target(lua, &contact.target)?)?;
    value.set("normal", triple(lua, contact.normal)?)?;
    value.set_readonly(true);
    Ok(value)
}

#[cfg(test)]
pub(super) fn fields(lua: &Lua, event: &Event) -> mlua::Result<Table> {
    fields_with_command(lua, event, None)
}
pub(super) fn fields_with_command(
    lua: &Lua,
    event: &Event,
    command: Option<&bloxgloom_host_api::actions::Command>,
) -> mlua::Result<Table> {
    let fields = lua.create_table()?;
    match event {
        Event::MovingTick {
            entity,
            tick,
            motion: pose,
        } => {
            fields.set("kind", "MovingTick")?;
            fields.set(
                "entity",
                crate::server::script::handles::entity(lua, *entity)?,
            )?;
            fields.set("tick", crate::server::script::handles::tick(lua, *tick)?)?;
            fields.set("motion", motion(lua, pose)?)?;
        }
        Event::MovingImpact { impact } => {
            fields.set("kind", "MovingImpact")?;
            fields.set(
                "entity",
                crate::server::script::handles::entity(lua, impact.entity)?,
            )?;
            fields.set(
                "motion_revision",
                crate::server::script::handles::revision(lua, impact.motion_revision)?,
            )?;
            fields.set(
                "tick",
                crate::server::script::handles::tick(lua, impact.tick)?,
            )?;
            fields.set("position", triple(lua, impact.position)?)?;
            fields.set("normal", triple(lua, impact.normal)?)?;
            fields.set("incoming_velocity", triple(lua, impact.incoming_velocity)?)?;
            fields.set("blocked", impact.blocked)?;
            fields.set("target", moving_target(lua, &impact.target)?)?;
        }
        Event::MovingExpiry {
            entity,
            tick,
            motion_revision,
            reason,
        } => {
            fields.set("kind", "MovingExpiry")?;
            fields.set(
                "entity",
                crate::server::script::handles::entity(lua, *entity)?,
            )?;
            fields.set("tick", crate::server::script::handles::tick(lua, *tick)?)?;
            fields.set(
                "motion_revision",
                crate::server::script::handles::revision(lua, *motion_revision)?,
            )?;
            fields.set(
                "reason",
                match reason {
                    bloxgloom_host_api::motion::ExpiryReason::Lifetime => "Lifetime",
                    bloxgloom_host_api::motion::ExpiryReason::WorldBoundary => "WorldBoundary",
                },
            )?;
        }
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
                fields.set(
                    "entity",
                    crate::server::script::handles::entity(lua, *entity)?,
                )?;
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
                    RemovalCause::Transformation => "Transformation",
                    RemovalCause::Replacement => "Replacement",
                    RemovalCause::SupportLoss => "SupportLoss",
                    RemovalCause::WorldEdit => "WorldEdit",
                    RemovalCause::Burn => "Burn",
                    RemovalCause::AnchoredBreak => "AnchoredBreak",
                },
            )?;
            fields.set(
                "random",
                crate::server::script::handles::unit_random(*random),
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
            fields.set(
                "entity",
                crate::server::script::handles::entity(lua, *entity)?,
            )?;
            fields.set("entity_lo", *entity as u32)?;
            fields.set("entity_hi", (*entity >> 32) as u32)?;
            fields.set("position", triple(lua, *position)?)?;
            fields.set("tick", crate::server::script::handles::tick(lua, *tick)?)?;
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
                candidate.set("id", crate::server::script::handles::entity(lua, id)?)?;
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
    if let (Some(command), Event::ActionRequested { arguments, .. }) = (command, event) {
        let values = command
            .decode_arguments(arguments)
            .ok_or_else(|| mlua::Error::RuntimeError("invalid typed command arguments".into()))?;
        let list = lua.create_table()?;
        for (index, value) in values.into_iter().enumerate() {
            use bloxgloom_host_api::actions::CommandValue;
            match value {
                CommandValue::Player { profile, session } => list.raw_set(
                    index + 1,
                    crate::server::script::handles::session(lua, profile, session)?,
                )?,
                CommandValue::ItemKey(key) | CommandValue::EntityKey(key) => {
                    list.raw_set(index + 1, key)?
                }
                CommandValue::Count(count) => list.raw_set(index + 1, count)?,
                CommandValue::Text(value) => list.raw_set(index + 1, value)?,
                CommandValue::Integer(value) => list.raw_set(index + 1, value)?,
                CommandValue::Number(value) => list.raw_set(index + 1, value.get())?,
            }
        }
        list.set_readonly(true);
        fields.set("command_arguments", list)?;
    }
    fields.set_readonly(true);
    Ok(fields)
}

#[cfg(test)]
mod tests;
