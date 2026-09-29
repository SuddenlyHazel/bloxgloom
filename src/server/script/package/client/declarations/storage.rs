//! V29 declarative single-cell storage and inventory screen reconstruction.
use super::*;
use crate::server::script::startup::StorageDeclaration;
use bloxgloom_host_api::{
    InventoryScreen,
    lifecycle::{FootprintCell, StorageBlockEntity},
};

pub(super) fn encode(
    writer: &mut Writer,
    owner: &str,
    declarations: &[StorageDeclaration],
) -> Result<(), ScriptError> {
    let mut own = declarations
        .iter()
        .filter(|d| {
            d.storage
                .entity
                .split_once(':')
                .is_some_and(|(package, _)| package == owner)
        })
        .collect::<Vec<_>>();
    own.sort_by(|a, b| a.storage.entity.cmp(&b.storage.entity));
    if own.len() > 8 {
        return Err(invalid());
    }
    writer.count(own.len())?;
    for declaration in own {
        let storage = &declaration.storage;
        let screen = &declaration.screen;
        if storage.block != screen.block
            || storage.entity != screen.entity
            || storage.placement_item != storage.block
            || storage.footprint.len() != 1
            || storage.footprint[0].offset != [0; 3]
            || storage.footprint[0].state != storage.anchor_state
            || storage.automation_faces.is_some()
            || screen.footprint != [[0; 3]]
            || screen.slots as usize != storage.slots
            || !screen.hint.is_empty()
            || !screen.status.is_empty()
            || screen.groups.len() != 1
            || screen.groups[0].first != 0
            || screen.groups[0].count != screen.slots
            || screen.groups[0].label != "STORAGE"
            || !screen.groups[0].insert
            || !screen.groups[0].extract
        {
            return Err(invalid());
        }
        writer.field(storage.entity.as_bytes())?;
        writer.field(storage.block.as_bytes())?;
        writer.field(screen.title.as_bytes())?;
        writer.field(&[screen.slots, screen.columns])?;
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut Reader<'_>,
    owner: &str,
    requires: &[String],
    blocks: &[content::Block],
) -> Result<Vec<StorageDeclaration>, ScriptError> {
    let mut result: Vec<StorageDeclaration> = Vec::new();
    for _ in 0..reader.count(8)? {
        if ![
            composition::CONTENT,
            composition::STORAGE,
            composition::INVENTORY_SCREENS,
        ]
        .into_iter()
        .all(|capability| requires.iter().any(|r| r == capability))
        {
            return Err(invalid());
        }
        let entity = reader.text(129)?;
        let block = reader.text(129)?;
        let title = reader.text(255)?;
        let slots = reader.field(2)?;
        let [slots, columns] = slots else {
            return Err(invalid());
        };
        if entity
            .split_once(':')
            .is_none_or(|(package, local)| package != owner || !identifier(local))
            || block
                .split_once(':')
                .is_none_or(|(package, local)| package != owner || !identifier(local))
            || result
                .last()
                .is_some_and(|last| last.storage.entity >= entity)
            || result.iter().any(|last| last.storage.block == block)
        {
            return Err(invalid());
        }
        let definition = blocks
            .iter()
            .find(|definition| definition.key == block)
            .ok_or_else(invalid)?;
        let state = crate::server::script::startup::placement_state(definition);
        let storage = StorageBlockEntity {
            entity: entity.clone(),
            block: block.clone(),
            placement_item: block.clone(),
            anchor_state: state.clone(),
            footprint: vec![FootprintCell {
                offset: [0; 3],
                state,
            }],
            slots: usize::from(*slots),
            automation_faces: None,
        };
        let screen =
            InventoryScreen::storage(&entity, &block, &title, *slots, *columns, vec![[0; 3]]);
        storage.validate().map_err(|_| invalid())?;
        screen.validate().map_err(|_| invalid())?;
        result.push(StorageDeclaration { storage, screen });
    }
    Ok(result)
}
