use super::*;

#[test]
fn every_public_removal_cause_preserves_its_exact_luau_context() {
    let lua = Lua::new();
    let previous = Block {
        state: "demo:lit".into(),
        block_type: "demo:block".into(),
        primary_item: Some("demo:item".into()),
        plant: false,
        supports_plant: true,
    };
    for (cause, expected) in [
        (RemovalCause::Break, "Break"),
        (RemovalCause::Replacement, "Replacement"),
        (RemovalCause::SupportLoss, "SupportLoss"),
        (RemovalCause::WorldEdit, "WorldEdit"),
        (RemovalCause::Burn, "Burn"),
        (RemovalCause::AnchoredBreak, "AnchoredBreak"),
    ] {
        let event = Event::BlockRemoved {
            cell: [-1, 80, 2],
            previous: previous.clone(),
            cause,
            random: (9u64 << 32) | 7,
        };
        let fields = fields(&lua, &event).unwrap();
        assert_eq!(fields.get::<String>("kind").unwrap(), "BlockRemoved");
        assert_eq!(fields.get::<String>("cause").unwrap(), expected);
        assert_eq!(fields.get::<u32>("random_lo").unwrap(), 7);
        assert_eq!(fields.get::<u32>("random_hi").unwrap(), 9);
        let cell = fields.get::<Table>("cell").unwrap();
        assert_eq!(
            [
                cell.raw_get::<i32>(1).unwrap(),
                cell.raw_get::<i32>(2).unwrap(),
                cell.raw_get::<i32>(3).unwrap()
            ],
            [-1, 80, 2]
        );
        let block = fields.get::<Table>("previous").unwrap();
        assert_eq!(block.get::<String>("state").unwrap(), "demo:lit");
        assert_eq!(block.get::<String>("block_type").unwrap(), "demo:block");
        assert_eq!(block.get::<String>("primary_item").unwrap(), "demo:item");
        assert!(block.get::<bool>("supports_plant").unwrap());
        assert!(fields.is_readonly() && cell.is_readonly() && block.is_readonly());
    }
}
