//! V32/V34/V35/V36 inert process machine descriptors. Luau source and durable private data
//! are excluded; the host and client reconstruct identical catalog identities.
use super::*;
use crate::server::script::startup::MachineDeclaration;
use bloxgloom_host_api::{
    FootprintCell, InventoryScreen, SlotGroup, StatusField, StatusFormat,
    machine::{self as api, ComponentMatch, ComponentOutput},
};
use std::sync::Arc;
#[path = "machine/components.rs"]
mod components;

#[derive(Clone, Copy)]
pub(super) struct Format {
    pub recipes: bool,
    pub ports: bool,
    pub footprints: bool,
    pub components: bool,
    pub active: bool,
}

pub(super) fn encode(
    writer: &mut Writer,
    owner: &str,
    declarations: &[MachineDeclaration],
    format: Format,
) -> Result<(), ScriptError> {
    let mut own = declarations
        .iter()
        .filter(|d| {
            d.machine
                .entity
                .split_once(':')
                .is_some_and(|(namespace, _)| namespace == owner)
        })
        .collect::<Vec<_>>();
    own.sort_by(|a, b| a.machine.entity.cmp(&b.machine.entity));
    writer.count(own.len())?;
    for declaration in own {
        let m = &declaration.machine;
        let screen = &declaration.screen;
        m.validate().map_err(|_| invalid())?;
        screen.validate().map_err(|_| invalid())?;
        let p = m.process.as_ref().ok_or_else(invalid)?;
        if m.item != m.block
            || m.slots != screen.slots
            || m.read_radius != u8::from(!m.ports.is_empty())
            || m.reads_neighbours == m.ports.is_empty()
            || m.variants.len() != 1
            || m.ports.len() > if format.ports { 8 } else { 0 }
            || p.recipes.is_empty()
            || p.recipes.len() > if format.recipes { 8 } else { 1 }
            || p.fuels.len() > 1
            || (m.variants[0].idle != m.variants[0].active && p.fuel.is_none())
            || p.input != if p.fuel.is_some() { 1 } else { 0 }
            || p.output != p.input + 1
            || m.variants[0].idle.len() > if format.footprints { 8 } else { 1 }
            || m.variants[0].idle.len() != m.variants[0].active.len()
            || m.variants[0]
                .idle
                .iter()
                .zip(&m.variants[0].active)
                .any(|(idle, active)| {
                    idle.offset != active.offset
                        || (!format.active && idle.state != active.state)
                        || idle.state != m.variants[0].placement_state
                        || (format.active && active.state != m.variants[0].active[0].state)
                        || idle.offset.iter().any(|axis| axis.unsigned_abs() > 2)
                })
            || screen.footprint
                != m.variants[0]
                    .idle
                    .iter()
                    .map(|cell| cell.offset)
                    .collect::<Vec<_>>()
            || screen.groups.len() != usize::from(m.slots)
            || screen.status.len() != 2
        {
            return Err(invalid());
        }
        if !format.components
            && (p.recipes.iter().any(|recipe| {
                !matches!(recipe.input_components, ComponentMatch::Empty)
                    || !matches!(recipe.output_components, ComponentOutput::Empty)
            }) || p
                .fuels
                .iter()
                .any(|f| !matches!(f.components, ComponentMatch::Empty)))
        {
            return Err(invalid());
        }
        writer.field(m.entity.as_bytes())?;
        writer.field(m.block.as_bytes())?;
        writer.field(&m.schema.to_le_bytes())?;
        writer.field(&m.interval.to_le_bytes())?;
        writer.field(m.variants[0].placement_state.as_bytes())?;
        if format.active {
            writer.field(m.variants[0].active[0].state.as_bytes())?;
        }
        writer.field(screen.title.as_bytes())?;
        writer.field(screen.hint.as_bytes())?;
        if format.recipes {
            writer.count(p.recipes.len())?;
        }
        for recipe in &p.recipes {
            writer.field(recipe.key.as_bytes())?;
            writer.field(recipe.input.as_bytes())?;
            writer.field(recipe.output.as_bytes())?;
            writer.field(&recipe.input_count.to_le_bytes())?;
            writer.field(&recipe.output_count.to_le_bytes())?;
            writer.field(&recipe.pulses.to_le_bytes())?;
            if format.components {
                components::encode_input(writer, &recipe.input_components)?;
                components::encode_output(writer, &recipe.output_components)?;
            }
        }
        writer.count(p.fuels.len())?;
        if let Some(fuel) = p.fuels.first() {
            writer.field(fuel.item.as_bytes())?;
            writer.field(&fuel.pulses.to_le_bytes())?;
            if format.components {
                components::encode_input(writer, &fuel.components)?;
            }
        }
        if format.ports {
            writer.count(m.ports.len())?;
            for port in &m.ports {
                writer.field(port.name.as_bytes())?;
                writer.count(port.faces.len())?;
                for face in &port.faces {
                    writer.field(&face.map(|axis| (axis + 1) as u8))?;
                }
                for slots in [&port.insert, &port.extract] {
                    writer.field(slots)?;
                }
            }
        }
        if format.footprints {
            writer.count(m.variants[0].idle.len())?;
            for cell in &m.variants[0].idle {
                writer.field(&cell.offset.map(|axis| (axis + 2) as u8))?;
            }
        }
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut Reader<'_>,
    owner: &str,
    requires: &[String],
    blocks: &[content::Block],
    format: Format,
) -> Result<Vec<MachineDeclaration>, ScriptError> {
    let mut result: Vec<MachineDeclaration> = Vec::new();
    for _ in 0..reader.count(8)? {
        if ![
            composition::CONTENT,
            composition::MACHINES,
            composition::INVENTORY_SCREENS,
        ]
        .into_iter()
        .all(|capability| requires.iter().any(|r| r == capability))
        {
            return Err(invalid());
        }
        let entity = reader.text(129)?;
        let block = reader.text(129)?;
        if [&entity, &block].into_iter().any(|key| {
            key.split_once(':')
                .is_none_or(|(namespace, local)| namespace != owner || !identifier(local))
        }) || result
            .last()
            .is_some_and(|old| old.machine.entity >= entity)
        {
            return Err(invalid());
        }
        let schema = u64::from_le_bytes(reader.field(8)?.try_into().map_err(|_| invalid())?);
        let interval = u32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
        let state = reader.text(129)?;
        let active_state = if format.active {
            reader.text(129)?
        } else {
            state.clone()
        };
        let title = reader.text(40)?;
        let hint = reader.text(80)?;
        let recipe_count = if format.recipes { reader.count(8)? } else { 1 };
        if recipe_count == 0 {
            return Err(invalid());
        }
        let mut recipes = Vec::with_capacity(recipe_count);
        for _ in 0..recipe_count {
            let key = reader.text(129)?;
            let input = reader.text(129)?;
            let output = reader.text(129)?;
            let input_count =
                u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
            let output_count =
                u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
            let pulses = u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
            let input_components = if format.components {
                components::decode_input(reader)?
            } else {
                ComponentMatch::Empty
            };
            let output_components = if format.components {
                components::decode_output(reader)?
            } else {
                ComponentOutput::Empty
            };
            recipes.push(api::Recipe {
                key,
                input,
                output,
                input_count,
                output_count,
                pulses,
                input_components,
                output_components,
            });
        }
        let fuel = if reader.count(1)? == 1 {
            let item = reader.text(129)?;
            let pulses = u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
            let components = if format.components {
                components::decode_input(reader)?
            } else {
                ComponentMatch::Empty
            };
            Some((item, pulses, components))
        } else {
            None
        };
        let mut ports = Vec::new();
        if format.ports {
            for _ in 0..reader.count(8)? {
                let name = reader.text(64)?;
                let mut faces = Vec::new();
                for _ in 0..reader.count(6)? {
                    let bytes = reader.field(3)?;
                    if bytes.len() != 3 || bytes.iter().any(|axis| *axis > 2) {
                        return Err(invalid());
                    }
                    let axes: [u8; 3] = bytes.try_into().map_err(|_| invalid())?;
                    faces.push(axes.map(|axis| i32::from(axis) - 1));
                }
                let insert = reader.field(54)?.to_vec();
                let extract = reader.field(54)?.to_vec();
                ports.push(api::Port {
                    name,
                    faces,
                    insert,
                    extract,
                });
            }
        }
        let mut offsets = vec![[0; 3]];
        if format.footprints {
            offsets.clear();
            for _ in 0..reader.count(8)? {
                let bytes = reader.field(3)?;
                if bytes.len() != 3 || bytes.iter().any(|axis| *axis > 4) {
                    return Err(invalid());
                }
                let axes: [u8; 3] = bytes.try_into().map_err(|_| invalid())?;
                offsets.push(axes.map(|axis| i32::from(axis) - 2));
            }
        }
        let definition = blocks
            .iter()
            .find(|old| old.key == block)
            .ok_or_else(invalid)?;
        if crate::server::script::startup::placement_state(definition) != state
            || !crate::server::script::startup::has_state(definition, &active_state)
            || recipes.iter().any(|recipe| {
                recipe
                    .key
                    .split_once(':')
                    .is_none_or(|(namespace, local)| namespace != owner || !identifier(local))
            })
        {
            return Err(invalid());
        }
        let slots = if fuel.is_some() { 3 } else { 2 };
        if active_state != state && fuel.is_none() {
            return Err(invalid());
        }
        let input_slot = if fuel.is_some() { 1 } else { 0 };
        let output_slot = input_slot + 1;
        let process = api::Process {
            input: input_slot,
            output: output_slot,
            fuel: fuel.as_ref().map(|_| 0),
            recipes: recipes.clone(),
            fuels: fuel
                .as_ref()
                .map(|(item, pulses, components)| api::Fuel {
                    item: item.clone(),
                    components: components.clone(),
                    pulses: *pulses,
                })
                .into_iter()
                .collect(),
        };
        let mut filters = Vec::new();
        if let Some((item, _, components)) = &fuel {
            filters.push(api::Filter {
                items: vec![item.clone()],
                components: components != &ComponentMatch::Empty,
            });
        }
        filters.push(api::Filter {
            items: recipes
                .iter()
                .map(|recipe| recipe.input.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            components: recipes
                .iter()
                .any(|r| r.input_components != ComponentMatch::Empty),
        });
        filters.push(api::Filter {
            items: recipes
                .iter()
                .map(|recipe| recipe.output.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            components: recipes
                .iter()
                .any(|r| r.output_components != ComponentOutput::Empty),
        });
        let cells = offsets
            .iter()
            .map(|offset| FootprintCell {
                offset: *offset,
                state: state.clone(),
            })
            .collect::<Vec<_>>();
        let active_cells = offsets
            .iter()
            .map(|offset| FootprintCell {
                offset: *offset,
                state: active_state.clone(),
            })
            .collect::<Vec<_>>();
        let machine = api::Machine {
            entity: entity.clone(),
            block: block.clone(),
            item: block.clone(),
            schema,
            slots,
            interval,
            read_radius: u8::from(!ports.is_empty()),
            reads_neighbours: !ports.is_empty(),
            variants: vec![api::Variant {
                placement_state: state,
                idle: cells.clone(),
                active: active_cells,
            }],
            filters,
            ports,
            process: Some(process),
            behavior: Arc::new(crate::server::script::machine::ScriptMachine::client()),
        };
        let mut screen = InventoryScreen::storage(&entity, &block, &title, slots, slots, offsets);
        screen.hint = hint;
        screen.groups = (0..slots)
            .map(|index| SlotGroup {
                label: if fuel.is_some() && index == 0 {
                    "FUEL"
                } else if index == input_slot {
                    "INPUT"
                } else {
                    "OUTPUT"
                }
                .into(),
                first: index,
                count: 1,
                insert: index != output_slot,
                extract: true,
            })
            .collect();
        screen.status = vec![
            StatusField {
                label: "FUEL".into(),
                format: StatusFormat::Milliseconds,
                maximum: 60_000,
            },
            StatusField {
                label: "PROGRESS".into(),
                format: StatusFormat::Progress,
                maximum: 1_000,
            },
        ];
        machine.validate().map_err(|_| invalid())?;
        screen.validate().map_err(|_| invalid())?;
        result.push(MachineDeclaration { machine, screen });
    }
    Ok(result)
}
