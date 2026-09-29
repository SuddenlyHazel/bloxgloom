//! Luau process machines with host-owned recipes, footprints, and screens.
use super::*;
use crate::server::script::values::integer;
use bloxgloom_host_api::{
    FootprintCell, InventoryScreen, SlotGroup, StatusField, StatusFormat,
    machine::{self as api, ComponentMatch, ComponentOutput},
};
#[path = "machine/footprint.rs"]
mod footprint;
#[path = "machine/ports.rs"]
mod ports;

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
            let cells = footprint::parse(field(&d, "footprint")?, &state)?;
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
            let recipes = match (field(&d, "recipe")?, field(&d, "recipes")?) {
                (Value::Table(recipe), Value::Nil) => vec![parse_recipe(&recipe, &namespace)?],
                (Value::Nil, Value::Table(list)) if list.metatable().is_none() => {
                    let count = list.raw_len();
                    if !(1..=8).contains(&count)
                        || list.clone().pairs::<Value, Value>().take(9).count() != count
                    {
                        return Err("machine recipes must be a dense list of 1..=8 entries");
                    }
                    let mut recipes = Vec::with_capacity(count);
                    for index in 1..=count {
                        let recipe: mlua::Table =
                            list.raw_get(index).map_err(|_| "invalid machine recipe")?;
                        recipes.push(parse_recipe(&recipe, &namespace)?);
                    }
                    recipes
                }
                _ => return Err("machine requires recipe or recipes"),
            };
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
            let ports = ports::parse(field(&d, "ports")?, slots)?;
            let process = api::Process {
                input: input_slot,
                output: output_slot,
                fuel: fuel.as_ref().map(|_| 0),
                recipes: recipes.clone(),
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
                items: recipes
                    .iter()
                    .map(|recipe| recipe.input.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                components: false,
            });
            filters.push(api::Filter {
                items: recipes
                    .iter()
                    .map(|recipe| recipe.output.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                components: false,
            });
            let machine = api::Machine {
                entity: entity.clone(),
                block: block.clone(),
                item: block.clone(),
                schema: snapshot.machine_schema(&module, schema, revision),
                slots,
                interval,
                read_radius: u8::from(!ports.is_empty()),
                reads_neighbours: !ports.is_empty(),
                variants: vec![api::Variant {
                    placement_state: state,
                    idle: cells.clone(),
                    active: cells.clone(),
                }],
                filters,
                ports: ports.clone(),
                process: Some(process),
                behavior: Arc::new(crate::server::script::machine::ScriptMachine::server(
                    Arc::clone(&snapshot),
                    module,
                    ports.iter().map(|port| port.name.clone()).collect(),
                )),
            };
            let mut screen = InventoryScreen::storage(
                &entity,
                &block,
                &title,
                slots,
                slots,
                cells.iter().map(|cell| cell.offset).collect(),
            );
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

fn parse_recipe(recipe: &mlua::Table, namespace: &str) -> Result<api::Recipe, &'static str> {
    if recipe.metatable().is_some() {
        return Err("machine recipe cannot have a metatable");
    }
    let key = text(field(recipe, "key")?)?;
    if key.split_once(':').is_none_or(|(owner, local)| {
        owner != namespace || !super::super::package::manifest::identifier(local)
    }) {
        return Err("machine recipe key must belong to the package");
    }
    Ok(api::Recipe {
        key,
        input: text(field(recipe, "input")?)?,
        input_count: integer(field(recipe, "input_count")?, 1, 128)? as u16,
        input_components: ComponentMatch::Empty,
        output: text(field(recipe, "output")?)?,
        output_count: integer(field(recipe, "output_count")?, 1, 128)? as u16,
        output_components: ComponentOutput::Empty,
        pulses: integer(field(recipe, "pulses")?, 1, 60000)? as u16,
    })
}
