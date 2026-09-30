//! Owned public copies: no inventory contents, entity-private bytes or writer.
use bloxgloom_host_api::gameplay::{Committed, CommittedEntity, Entity};
use mlua::{Lua, Table};

fn entity(lua: &Lua, entity: &Entity, kind: &str) -> mlua::Result<Table> {
    let value = lua.create_table()?;
    value.set("kind", kind)?;
    value.set("entity", super::super::handles::entity(lua, entity.id)?)?;
    value.set("entity_type", entity.entity_type.as_str())?;
    let position = lua.create_sequence_from(entity.position)?;
    position.set_readonly(true);
    value.set("position", position)?;
    if let Some(anchor) = entity.anchor {
        let anchor = lua.create_sequence_from(anchor)?;
        anchor.set_readonly(true);
        value.set("anchor", anchor)?;
    }
    value.set("data", lua.create_string(&entity.data)?)?;
    value.set_readonly(true);
    Ok(value)
}

pub(super) fn present(lua: &Lua, event: &Committed) -> mlua::Result<Table> {
    let fields = lua.create_table()?;
    fields.set("kind", "Committed")?;
    let blocks = lua.create_table()?;
    for (index, block) in event.blocks.iter().enumerate() {
        let value = lua.create_table()?;
        let cell = lua.create_sequence_from(block.cell)?;
        cell.set_readonly(true);
        value.set("cell", cell)?;
        value.set("state", block.state.as_str())?;
        value.set_readonly(true);
        blocks.raw_set(index + 1, value)?;
    }
    blocks.set_readonly(true);
    fields.set("blocks", blocks)?;
    let entities = lua.create_table()?;
    for (index, change) in event.entities.iter().enumerate() {
        let value = match change {
            CommittedEntity::Spawned(view) => entity(lua, view, "Spawned")?,
            CommittedEntity::Updated(view) => entity(lua, view, "Updated")?,
            CommittedEntity::Removed { id, key } => {
                let value = lua.create_table()?;
                value.set("kind", "Removed")?;
                value.set("entity", super::super::handles::entity(lua, *id)?)?;
                value.set("entity_type", key.as_str())?;
                value.set_readonly(true);
                value
            }
        };
        entities.raw_set(index + 1, value)?;
    }
    entities.set_readonly(true);
    fields.set("entities", entities)?;
    if let Some((profile, revision)) = event.inventory {
        let value = lua.create_table()?;
        value.set("profile", super::super::handles::profile(lua, profile)?)?;
        value.set("revision", super::super::handles::revision(lua, revision)?)?;
        value.set_readonly(true);
        fields.set("inventory", value)?;
    }
    fields.set_readonly(true);
    Ok(fields)
}

pub(super) fn seed(event: &Committed) -> u64 {
    let mut seed = super::super::runtime::Seed::new().word(event.blocks.len() as u64);
    for block in &event.blocks {
        for axis in block.cell {
            seed = seed.bytes(&axis.to_le_bytes());
        }
        seed = seed.bytes(block.state.as_bytes());
    }
    seed = seed.word(event.entities.len() as u64);
    for entity in &event.entities {
        match entity {
            CommittedEntity::Spawned(e) | CommittedEntity::Updated(e) => {
                seed = seed
                    .word(e.id)
                    .bytes(e.entity_type.as_bytes())
                    .bytes(&e.data);
                for axis in e.position {
                    seed = seed.bytes(&axis.to_le_bytes());
                }
            }
            CommittedEntity::Removed { id, key } => {
                seed = seed.word(*id).bytes(key.as_bytes());
            }
        }
    }
    if let Some((profile, revision)) = event.inventory {
        seed = seed.bytes(&profile.to_le_bytes()).word(revision);
    }
    seed.finish()
}
