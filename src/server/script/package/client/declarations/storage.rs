//! Declarative storage and inventory screen reconstruction through V38.
use super::*;
use crate::server::script::startup::StorageDeclaration;
use bloxgloom_host_api::{
    InventoryScreen, SlotGroup,
    lifecycle::{FootprintCell, StorageBlockEntity},
};

pub(super) fn encode(
    writer: &mut Writer,
    owner: &str,
    declarations: &[StorageDeclaration],
    screen_layout: bool,
    footprints: bool,
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
            || storage.footprint.is_empty()
            || storage.footprint.len() > 8
            || storage.footprint.iter().any(|cell| {
                cell.state != storage.anchor_state
                    || cell.offset.iter().any(|axis| !(-2..=2).contains(axis))
            })
            || (!footprints
                && (storage.footprint.len() != 1 || storage.footprint[0].offset != [0; 3]))
            || storage.automation_faces.is_some()
            || screen.footprint
                != storage
                    .footprint
                    .iter()
                    .map(|cell| cell.offset)
                    .collect::<Vec<_>>()
            || screen.slots as usize != storage.slots
            || !screen.status.is_empty()
            || screen
                .groups
                .iter()
                .any(|group| !group.insert || !group.extract)
        {
            return Err(invalid());
        }
        screen.validate().map_err(|_| invalid())?;
        writer.field(storage.entity.as_bytes())?;
        writer.field(storage.block.as_bytes())?;
        writer.field(screen.title.as_bytes())?;
        writer.field(&[screen.slots, screen.columns])?;
        if footprints {
            writer.count(storage.footprint.len())?;
            for cell in &storage.footprint {
                writer.field(&cell.offset.map(|axis| axis as i8 as u8))?;
            }
        }
        if screen_layout {
            writer.field(screen.hint.as_bytes())?;
            writer.count(screen.groups.len())?;
            for group in &screen.groups {
                writer.field(group.label.as_bytes())?;
                writer.field(&[group.count])?;
            }
        } else if !screen.hint.is_empty()
            || screen.groups.len() != 1
            || screen.groups[0].first != 0
            || screen.groups[0].count != screen.slots
            || screen.groups[0].label != "STORAGE"
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut Reader<'_>,
    owner: &str,
    requires: &[String],
    blocks: &[content::Block],
    screen_layout: bool,
    footprints: bool,
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
        let cells = if footprints {
            let mut cells = Vec::new();
            for _ in 0..reader.count(8)? {
                let [x, y, z] = reader.field(3)? else {
                    return Err(invalid());
                };
                let offset = [*x as i8 as i32, *y as i8 as i32, *z as i8 as i32];
                if offset.iter().any(|axis| !(-2..=2).contains(axis))
                    || cells.iter().any(|old: &FootprintCell| old.offset == offset)
                {
                    return Err(invalid());
                }
                cells.push(FootprintCell {
                    offset,
                    state: state.clone(),
                });
            }
            cells
        } else {
            vec![FootprintCell {
                offset: [0; 3],
                state: state.clone(),
            }]
        };
        let storage = StorageBlockEntity {
            entity: entity.clone(),
            block: block.clone(),
            placement_item: block.clone(),
            footprint: cells.clone(),
            anchor_state: state,
            slots: usize::from(*slots),
            automation_faces: None,
        };
        let mut screen = InventoryScreen::storage(
            &entity,
            &block,
            &title,
            *slots,
            *columns,
            cells.iter().map(|cell| cell.offset).collect(),
        );
        if screen_layout {
            screen.hint = reader.text(80)?;
            let mut first = 0u8;
            let mut groups = Vec::new();
            for _ in 0..reader.count(usize::from(*slots))? {
                let label = reader.text(20)?;
                let [count] = reader.field(1)? else {
                    return Err(invalid());
                };
                if *count == 0 || first.checked_add(*count).is_none_or(|end| end > *slots) {
                    return Err(invalid());
                }
                groups.push(SlotGroup {
                    label,
                    first,
                    count: *count,
                    insert: true,
                    extract: true,
                });
                first += *count;
            }
            if first != *slots {
                return Err(invalid());
            }
            screen.groups = groups;
        }
        storage.validate().map_err(|_| invalid())?;
        screen.validate().map_err(|_| invalid())?;
        result.push(StorageDeclaration { storage, screen });
    }
    Ok(result)
}
