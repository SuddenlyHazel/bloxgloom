//! A fueled stone crusher authored entirely through public registrations.
use bloxgloom_host_api::{
    CubeBlock, FootprintCell, InventoryScreen, Registrar, RegistrationError, SlotGroup,
    StatusField, StatusFormat, machine::*,
};
use std::sync::Arc;
pub const KEY: &str = "fixture:crusher";
pub const MARKED_INPUT: &[u8] = b"crusher:marked";
pub const REFINED_INPUT: &[u8] = b"crusher:refined";
pub fn register(r: &mut dyn Registrar) -> Result<(), RegistrationError> {
    r.cube_block(CubeBlock {
        key: KEY.into(),
        name: "CRUSHER".into(),
        texture: "bloxgloom:chest_side".into(),
    })?;
    let mut screen = InventoryScreen::storage(KEY, KEY, "CRUSHER", 3, 3, vec![[0; 3]]);
    screen.groups = [("FUEL", true), ("STONE", true), ("GRAVEL", false)]
        .into_iter()
        .enumerate()
        .map(|(i, (name, insert))| SlotGroup {
            label: name.into(),
            first: i as u8,
            count: 1,
            insert,
            extract: true,
        })
        .collect();
    screen.hint = "ONE STONE > TWO GRAVEL / STICKS / IN ABOVE, OUT BELOW".into();
    screen.status = vec![
        StatusField {
            label: "FUEL".into(),
            format: StatusFormat::Milliseconds,
            maximum: 96000,
        },
        StatusField {
            label: "CRUSHING".into(),
            format: StatusFormat::Progress,
            maximum: 1000,
        },
    ];
    r.inventory_screen(screen)?;
    let cells = vec![FootprintCell {
        offset: [0; 3],
        state: KEY.into(),
    }];
    r.machine(Machine {
        entity: KEY.into(),
        block: KEY.into(),
        item: KEY.into(),
        schema: 0x4352_5553_4800_0001,
        slots: 3,
        interval: 20,
        read_radius: 0,
        reads_neighbours: false,
        variants: vec![Variant {
            placement_state: KEY.into(),
            idle: cells.clone(),
            active: cells,
        }],
        filters: vec![
            Filter {
                items: vec!["bloxgloom:stick".into()],
                components: false,
            },
            Filter {
                items: vec!["bloxgloom:stone".into()],
                components: true,
            },
            Filter {
                items: vec!["bloxgloom:gravel".into()],
                components: true,
            },
        ],
        ports: vec![
            Port {
                name: "feed".into(),
                faces: vec![[0, 1, 0]],
                insert: vec![0, 1],
                extract: vec![],
            },
            Port {
                name: "product".into(),
                faces: vec![[0, -1, 0]],
                insert: vec![],
                extract: vec![2],
            },
        ],
        process: Some(Process {
            input: 1,
            output: 2,
            fuel: Some(0),
            recipes: vec![
                Recipe {
                    key: "fixture:crush_stone".into(),
                    input: "bloxgloom:stone".into(),
                    input_count: 1,
                    input_components: ComponentMatch::Empty,
                    output: "bloxgloom:gravel".into(),
                    output_count: 2,
                    output_components: ComponentOutput::Empty,
                    pulses: 3,
                },
                Recipe {
                    key: "fixture:crush_marked_stone".into(),
                    input: "bloxgloom:stone".into(),
                    input_count: 1,
                    input_components: ComponentMatch::Exact(ComponentValue {
                        version: 1,
                        bytes: MARKED_INPUT.to_vec(),
                    }),
                    output: "bloxgloom:gravel".into(),
                    output_count: 2,
                    output_components: ComponentOutput::PreserveInput,
                    pulses: 3,
                },
                Recipe {
                    key: "fixture:crush_refined_stone".into(),
                    input: "bloxgloom:stone".into(),
                    input_count: 2,
                    input_components: ComponentMatch::Exact(ComponentValue {
                        version: 2,
                        bytes: REFINED_INPUT.to_vec(),
                    }),
                    output: "bloxgloom:gravel".into(),
                    output_count: 1,
                    output_components: ComponentOutput::Exact(ComponentValue {
                        version: 1,
                        bytes: MARKED_INPUT.to_vec(),
                    }),
                    pulses: 2,
                },
            ],
            fuels: vec![Fuel {
                item: "bloxgloom:stick".into(),
                components: ComponentMatch::Empty,
                pulses: 30,
            }],
        }),
        behavior: Arc::new(Crush),
    })
}
struct Crush;
impl Behavior for Crush {
    fn plan(&self, c: &Context<'_>) -> Result<Plan, RegistrationError> {
        let pulses = if c.data.is_empty() {
            0
        } else {
            u64::from_le_bytes(
                c.data
                    .try_into()
                    .map_err(|_| RegistrationError("invalid crusher counter".into()))?,
            )
        };
        Ok(Plan {
            data: pulses.wrapping_add(1).to_le_bytes().to_vec(),
            next_tick: c
                .due
                .checked_add(20)
                .ok_or_else(|| RegistrationError("tick exhausted".into()))?,
            work: vec![Work::Process],
        })
    }
}
