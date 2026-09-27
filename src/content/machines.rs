use super::*;
use bloxgloom_host_api::{FootprintCell, RegistrationError as ApiError, machine::*};
use std::sync::Arc;
impl Catalog {
    /// Recompile the builtin's declarative flammability-derived fuels after all
    /// extension items exist, not only during Catalog::builtins().
    pub(crate) fn refresh_builtin_fuels(&mut self) -> Result<(), ApiError> {
        let Some(id) = self.entity_type_id_by_key("bloxgloom:kiln") else {
            return Ok(());
        };
        let Some(machine) = self.machine(id) else {
            return Ok(());
        };
        let mut machine = (**machine).clone();
        let fuels = self.kiln_fuels();
        machine.filters[0].items = fuels.iter().map(|f| f.item.clone()).collect();
        machine
            .process
            .as_mut()
            .expect("builtin kiln process")
            .fuels = fuels;
        machine.validate()?;
        self.machines[id.0 as usize] = Some(Arc::new(machine));
        Ok(())
    }

    fn kiln_fuels(&self) -> Vec<Fuel> {
        let mut fuels = self
            .items()
            .filter_map(|i| {
                if !self.valid_item_components(i.id, None) {
                    return None;
                }
                let pulses = if i.key == "bloxgloom:stick" {
                    40
                } else if i.key == "bloxgloom:sapling" {
                    80
                } else {
                    let block = self.block_type(self.state(i.placeable?)?.block_type)?;
                    if !block.flammable {
                        return None;
                    }
                    if block.key == "bloxgloom:wood" {
                        240
                    } else {
                        60
                    }
                };
                Some(Fuel {
                    item: i.key.to_string(),
                    components: ComponentMatch::Empty,
                    pulses,
                })
            })
            .collect::<Vec<_>>();
        fuels.sort_by(|a, b| a.item.cmp(&b.item));
        fuels
    }
    pub(crate) fn machine(&self, id: EntityTypeId) -> Option<&Arc<Machine>> {
        self.machines.get(id.0 as usize)?.as_ref()
    }
    pub(crate) fn machines(&self) -> impl Iterator<Item = (EntityTypeId, &Arc<Machine>)> {
        self.machines
            .iter()
            .enumerate()
            .filter_map(|(id, m)| m.as_ref().map(|m| (EntityTypeId(id as u32), m)))
    }
    pub(crate) fn register_machine_identity(&mut self, m: &Machine) -> Result<(), ApiError> {
        m.validate()?;
        let id = EntityTypeId(self.entities.len() as u32);
        self.register_entity_type(EntityTypeDef {
            id,
            key: m.entity.clone().into(),
            schema_version: 1,
            schema_fingerprint: m.schema,
        })
        .map_err(|e| ApiError(format!("machine identity: {e:?}")))?;
        Ok(())
    }
    pub(crate) fn bind_machine(
        &mut self,
        id: EntityTypeId,
        m: Arc<Machine>,
    ) -> Result<(), ApiError> {
        let mut definition = (*m).clone();
        for filter in &mut definition.filters {
            filter.items = self.expand_item_filter(&filter.items)?;
        }
        let m = Arc::new(definition);
        m.validate()?;
        let bad = || ApiError("unresolved or incompatible machine declaration".into());
        if self.entity_type(id).is_none_or(|e| e.key != m.entity)
            || self.machines().any(|(_, old)| old.block == m.block)
        {
            return Err(bad());
        }
        let block = self.block_by_key(&m.block).ok_or_else(bad)?;
        if !self.items().any(|i| {
            i.key == m.item
                && i.placeable.is_some_and(|s| {
                    m.variants
                        .iter()
                        .any(|v| self.state_by_key(&v.placement_state) == Some(s))
                })
        }) {
            return Err(bad());
        }
        let screen = self.inventory_screen(id).ok_or_else(bad)?;
        if screen.slots != m.slots
            || screen.block != m.block
            || screen.status.len() != if m.process.is_some() { 2 } else { 0 }
            || screen
                .footprint
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != m.variants[0].idle.iter().map(|c| c.offset).collect()
        {
            return Err(bad());
        }
        for v in &m.variants {
            for c in v.idle.iter().chain(&v.active) {
                if self
                    .state_by_key(&c.state)
                    .and_then(|s| self.state(s))
                    .is_none_or(|s| s.block_type != block)
                {
                    return Err(bad());
                }
            }
        }
        let item = |s: &str| self.item_keys.contains(s);
        if m.filters.iter().flat_map(|f| &f.items).any(|s| !item(s)) {
            return Err(bad());
        }
        if let Some(p) = &m.process {
            let valid_components = |key: &str, policy: &ComponentMatch| {
                self.items()
                    .find(|i| i.key == key)
                    .is_some_and(|i| match policy {
                        ComponentMatch::Empty => self.valid_item_components(i.id, None),
                        ComponentMatch::Exact(value) => {
                            self.valid_item_components(i.id, Some((value.version, &value.bytes)))
                        }
                        ComponentMatch::Present => !matches!(
                            self.item_components.get(key),
                            Some(bloxgloom_host_api::content::Components::None)
                        ),
                    })
            };
            let accepts = |slot: u8, key: &String| {
                m.filters[slot as usize].items.is_empty()
                    || m.filters[slot as usize].items.contains(key)
            };
            if p.recipes.iter().any(|r| {
                !valid_key(&r.key)
                    || !item(&r.input)
                    || !item(&r.output)
                    || !valid_components(&r.input, &r.input_components)
                    || !valid_components(
                        &r.output,
                        &match &r.output_components {
                            ComponentOutput::Empty => ComponentMatch::Empty,
                            ComponentOutput::Exact(value) => ComponentMatch::Exact(value.clone()),
                            ComponentOutput::PreserveInput => r.input_components.clone(),
                        },
                    )
                    || !accepts(p.input, &r.input)
                    || !accepts(p.output, &r.output)
                    || (r.input_components != ComponentMatch::Empty
                        && !m.filters[p.input as usize].components)
                    || (match &r.output_components {
                        ComponentOutput::Empty => false,
                        ComponentOutput::PreserveInput => {
                            r.input_components != ComponentMatch::Empty
                        }
                        ComponentOutput::Exact(_) => true,
                    } && !m.filters[p.output as usize].components)
            }) || p.fuels.iter().any(|f| {
                !item(&f.item)
                    || !valid_components(&f.item, &f.components)
                    || p.fuel.is_none_or(|slot| {
                        !accepts(slot, &f.item)
                            || (f.components != ComponentMatch::Empty
                                && !m.filters[slot as usize].components)
                    })
            }) {
                return Err(bad());
            }
            if screen.status[0].maximum
                < u32::from(p.fuels.iter().map(|f| f.pulses).max().unwrap_or(0)) * m.interval * 20
                || screen.status[1].maximum < 1000
            {
                return Err(bad());
            }
        }
        self.machines.resize_with(self.entities.len(), || None);
        let slot = &mut self.machines[id.0 as usize];
        if slot.is_some() {
            return Err(bad());
        }
        *slot = Some(m);
        Ok(())
    }
    pub(super) fn builtin_machines(&mut self) {
        let state = |facing: &str, half: &str, lit: bool| {
            let mut s = KILN_DEFAULT_STATE;
            for (k, v) in [
                ("facing", facing),
                ("half", half),
                ("lit", if lit { "true" } else { "false" }),
            ] {
                s = self.state_with_property(s, k, v).unwrap();
            }
            self.state(s).unwrap().key.to_string()
        };
        let variants = ["north", "east", "south", "west"]
            .into_iter()
            .map(|f| Variant {
                placement_state: state(f, "lower", false),
                idle: vec![
                    FootprintCell {
                        offset: [0; 3],
                        state: state(f, "lower", false),
                    },
                    FootprintCell {
                        offset: [0, 1, 0],
                        state: state(f, "upper", false),
                    },
                ],
                active: vec![
                    FootprintCell {
                        offset: [0; 3],
                        state: state(f, "lower", true),
                    },
                    FootprintCell {
                        offset: [0, 1, 0],
                        state: state(f, "upper", true),
                    },
                ],
            })
            .collect();
        let fuels = self.kiln_fuels();
        let kiln = Machine {
            entity: "bloxgloom:kiln".into(),
            block: "bloxgloom:kiln".into(),
            item: "bloxgloom:kiln".into(),
            schema: 1,
            slots: 3,
            interval: 20,
            read_radius: 0,
            reads_neighbours: false,
            variants,
            filters: vec![
                Filter {
                    items: fuels.iter().map(|f| f.item.clone()).collect(),
                    components: false,
                },
                Filter::any(),
                Filter::any(),
            ],
            ports: vec![
                Port {
                    name: "input".into(),
                    faces: FACES.to_vec(),
                    insert: vec![0, 1],
                    extract: vec![],
                },
                Port {
                    name: "output".into(),
                    faces: FACES.to_vec(),
                    insert: vec![],
                    extract: vec![2],
                },
            ],
            process: Some(Process {
                input: 1,
                output: 2,
                fuel: Some(0),
                recipes: vec![Recipe {
                    key: "bloxgloom:smelt_gravel".into(),
                    input: "bloxgloom:gravel".into(),
                    input_count: 1,
                    input_components: ComponentMatch::Empty,
                    output: "bloxgloom:stone".into(),
                    output_count: 1,
                    output_components: ComponentOutput::Empty,
                    pulses: 4,
                }],
                fuels,
            }),
            behavior: Arc::new(Processor),
        };
        self.bind_machine(KILN_ENTITY_TYPE, Arc::new(kiln))
            .expect("builtin kiln machine");
        let cells = vec![FootprintCell {
            offset: [0; 3],
            state: "bloxgloom:hopper".into(),
        }];
        self.bind_machine(
            HOPPER_ENTITY_TYPE,
            Arc::new(Machine {
                entity: "bloxgloom:hopper".into(),
                block: "bloxgloom:hopper".into(),
                item: "bloxgloom:hopper".into(),
                schema: 1,
                slots: 3,
                interval: 20,
                read_radius: 1,
                reads_neighbours: true,
                variants: vec![Variant {
                    placement_state: "bloxgloom:hopper".into(),
                    idle: cells.clone(),
                    active: cells,
                }],
                filters: vec![Filter::any(); 3],
                ports: vec![Port {
                    name: "storage".into(),
                    faces: FACES.to_vec(),
                    insert: vec![0, 1, 2],
                    extract: vec![0, 1, 2],
                }],
                process: None,
                behavior: Arc::new(DownwardFlow),
            }),
        )
        .expect("builtin hopper machine");
    }
}
