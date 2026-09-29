//! Single-cell Luau process machines with host-owned recipes and screens.
use super::*;
use crate::server::script::values::integer;
use bloxgloom_host_api::{
    FootprintCell, InventoryScreen, SlotGroup, StatusField, StatusFormat,
    machine::{self as api, ComponentMatch, ComponentOutput},
};

#[derive(Clone, Debug)]
pub(in crate::server::script) struct Declaration {
    pub(in crate::server::script) machine: api::Machine,
    pub(in crate::server::script) screen: InventoryScreen,
}

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, value: Value| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !snapshot.permits_machines(&namespace) {
                return Err("register_machine requires content, machines and inventory_screens/v1");
            }
            if pending.machines.len() >= 8 {
                return Err("machine declaration limit exceeded (8)");
            }
            let Value::Table(d) = value else {
                return Err("machine declaration must be a table");
            };
            let entity = text(field(&d, "entity")?)?;
            let block = text(field(&d, "block")?)?;
            if [&entity, &block].into_iter().any(|key| {
                key.split_once(':').is_none_or(|(owner, local)| {
                    owner != namespace || !super::super::package::manifest::identifier(local)
                })
            }) {
                return Err("machine keys must belong to the package");
            }
            if pending
                .machines
                .iter()
                .any(|old| old.machine.entity == entity || old.machine.block == block)
                || pending
                    .storage
                    .iter()
                    .any(|old| old.storage.entity == entity || old.storage.block == block)
                || pending.creatures.iter().any(|old| old.key == entity)
                || pending.entities.iter().any(|old| old.key == entity)
            {
                return Err("duplicate machine identity or block");
            }
            let block_def = pending
                .blocks
                .iter()
                .find(|old| old.key == block)
                .ok_or("machine needs a previously registered block")?;
            let state = super::placement_state(block_def);
            let module = text(field(&d, "module")?)?;
            if module.split_once(':').map(|v| v.0) != Some(namespace.as_str())
                || snapshot.source(&module).is_none()
            {
                return Err("machine module must be a declared package source");
            }
            let schema = integer(field(&d, "schema")?, 1, u16::MAX.into())? as u16;
            let revision = integer(field(&d, "revision")?, 1, u16::MAX.into())? as u16;
            let interval = integer(field(&d, "interval")?, 1, 60000)? as u32;
            let title = text(field(&d, "title")?)?;
            let hint = match field(&d, "hint")? {
                Value::Nil => String::new(),
                value => text(value)?,
            };
            let Value::Table(recipe) = field(&d, "recipe")? else {
                return Err("machine recipe must be a table");
            };
            let recipe_key = text(field(&recipe, "key")?)?;
            if recipe_key.split_once(':').is_none_or(|(owner, local)| {
                owner != namespace || !super::super::package::manifest::identifier(local)
            }) {
                return Err("machine recipe key must belong to the package");
            }
            let input = text(field(&recipe, "input")?)?;
            let output = text(field(&recipe, "output")?)?;
            let input_count = integer(field(&recipe, "input_count")?, 1, 128)? as u16;
            let output_count = integer(field(&recipe, "output_count")?, 1, 128)? as u16;
            let pulses = integer(field(&recipe, "pulses")?, 1, 60000)? as u16;
            let fuel = match field(&d, "fuel")? {
                Value::Nil => None,
                Value::Table(fuel) => Some((
                    text(field(&fuel, "item")?)?,
                    integer(field(&fuel, "pulses")?, 1, 240)? as u16,
                )),
                _ => return Err("machine fuel must be a table"),
            };
            let slots = if fuel.is_some() { 3 } else { 2 };
            let input_slot = if fuel.is_some() { 1 } else { 0 };
            let output_slot = input_slot + 1;
            let process = api::Process {
                input: input_slot,
                output: output_slot,
                fuel: fuel.as_ref().map(|_| 0),
                recipes: vec![api::Recipe {
                    key: recipe_key,
                    input: input.clone(),
                    input_count,
                    input_components: ComponentMatch::Empty,
                    output: output.clone(),
                    output_count,
                    output_components: ComponentOutput::Empty,
                    pulses,
                }],
                fuels: fuel
                    .as_ref()
                    .map(|(item, pulses)| api::Fuel {
                        item: item.clone(),
                        components: ComponentMatch::Empty,
                        pulses: *pulses,
                    })
                    .into_iter()
                    .collect(),
            };
            let mut filters = Vec::new();
            if let Some((item, _)) = &fuel {
                filters.push(api::Filter {
                    items: vec![item.clone()],
                    components: false,
                });
            }
            filters.push(api::Filter {
                items: vec![input],
                components: false,
            });
            filters.push(api::Filter {
                items: vec![output],
                components: false,
            });
            let cell = FootprintCell {
                offset: [0; 3],
                state: state.clone(),
            };
            let machine = api::Machine {
                entity: entity.clone(),
                block: block.clone(),
                item: block.clone(),
                schema: snapshot.machine_schema(&module, schema, revision),
                slots,
                interval,
                read_radius: 0,
                reads_neighbours: false,
                variants: vec![api::Variant {
                    placement_state: state,
                    idle: vec![cell.clone()],
                    active: vec![cell],
                }],
                filters,
                ports: vec![],
                process: Some(process),
                behavior: Arc::new(crate::server::script::machine::ScriptMachine::server(
                    Arc::clone(&snapshot),
                    module,
                )),
            };
            let mut screen =
                InventoryScreen::storage(&entity, &block, &title, slots, slots, vec![[0; 3]]);
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
            machine
                .validate()
                .map_err(|_| "invalid machine declaration")?;
            screen.validate().map_err(|_| "invalid machine screen")?;
            pending.machines.push(Declaration { machine, screen });
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}

fn field(table: &mlua::Table, key: &str) -> Result<Value, &'static str> {
    table.raw_get(key).map_err(|_| "invalid machine field")
}
