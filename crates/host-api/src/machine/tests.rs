use super::*;

struct NoWork;

impl Behavior for NoWork {
    fn plan(&self, context: &Context<'_>) -> Result<Plan, RegistrationError> {
        Ok(Plan {
            data: context.data.to_vec(),
            next_tick: context.due + 1,
            work: vec![],
        })
    }
}

#[test]
fn placement_variants_choose_one_identity_and_reject_duplicate_states() {
    let variant = |state: &str| Variant {
        placement_state: state.into(),
        idle: vec![FootprintCell {
            offset: [0; 3],
            state: state.into(),
        }],
        active: vec![FootprintCell {
            offset: [0; 3],
            state: state.into(),
        }],
    };
    let mut machine = Machine {
        entity: "test:machine".into(),
        block: "test:block".into(),
        item: "test:block".into(),
        schema: 1,
        slots: 1,
        interval: 1,
        read_radius: 0,
        reads_neighbours: false,
        variants: vec![
            variant("test:block[face=north]"),
            variant("test:block[face=south]"),
        ],
        filters: vec![Filter::any()],
        ports: vec![],
        process: None,
        behavior: Arc::new(NoWork),
    };
    machine.validate().unwrap();
    let plan = machine
        .plan_place([2, 80, -3], "test:block[face=south]")
        .unwrap();
    assert_eq!(plan.variant, 1);
    assert_eq!(
        plan.cells,
        vec![([2, 80, -3], "test:block[face=south]".into())]
    );
    machine.variants[1].placement_state = machine.variants[0].placement_state.clone();
    assert!(machine.validate().is_err());
}
