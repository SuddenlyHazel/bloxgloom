use super::*;

fn command(block: BlockId, y: i32) -> BlockEditCommand {
    BlockEditCommand {
        id: 1,
        profile: 17,
        action_id: 1,
        receipt_value: vec![1],
        x: 0,
        y,
        z: 0,
        block,
        slot: 0,
    }
}

#[test]
fn tall_plant_place_debits_one_and_break_either_half_clears_both() {
    let path = temp_save_dir("tall-plant-pair");
    let mut state = server_state(19, path.clone()).unwrap();
    let catalog = state.world.catalog_arc();
    let lower = catalog
        .state_by_key("bloxgloom:sunflower[half=lower]")
        .unwrap();
    let upper = catalog
        .state_by_key("bloxgloom:sunflower[half=upper]")
        .unwrap();
    let item = catalog.item_by_key("bloxgloom:sunflower").unwrap();
    // Cross the vertical chunk seam and exercise fenced reads on both cells.
    state.world.edit(0, 94, 0, GRASS).unwrap();
    state.world.edit(0, 95, 0, AIR).unwrap();
    state.world.edit(0, 96, 0, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(item, 2));
    let _peer = add_test_client(&mut state, [0.5, 95.0, 0.5], inventory);
    let plan = plan_block_edit(&mut state, TickId::new(1), command(lower, 95)).unwrap();
    assert_eq!(
        plan.inventory.as_ref().unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    assert_eq!(plan.deltas.len(), 2);
    assert!(plan.deltas.iter().any(|d| d.block == lower));
    assert!(plan.deltas.iter().any(|d| d.block == upper));
    assert_eq!(
        state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
        2
    );
    assert_eq!(
        state.world.get_block(0, 95, 0).unwrap(),
        AIR,
        "planning cannot publish terrain"
    );
    state.world.edit(0, 95, 0, lower).unwrap();
    state.world.edit(0, 96, 0, upper).unwrap();
    let mut reads = TerrainReads::default();
    let inventory = state.clients[&1].inventory.clone();
    let harvest = plan_gameplay_removals(
        &mut state,
        &mut reads,
        &[(0, 95, 0, AIR), (0, 96, 0, AIR)],
        &[
            (lower, [0, 95, 0], RemovalCause::Break),
            (upper, [0, 96, 0], RemovalCause::Break),
        ],
        (17, &inventory, inventory.revision),
        [0.5, 95.0, 0.5],
        1,
    )
    .unwrap();
    assert_eq!(harvest.drops.len(), 1);
    assert_eq!(harvest.drops[0].1.item, item);
    assert_eq!(
        harvest.drops[0].1.count, 1,
        "two halves harvest one finite item"
    );
    let support = plan_gameplay_removals(
        &mut state,
        &mut TerrainReads::default(),
        &[(0, 94, 0, AIR)],
        &[(GRASS, [0, 94, 0], RemovalCause::Break)],
        (17, &inventory, inventory.revision),
        [0.5, 95.0, 0.5],
        1,
    )
    .unwrap();
    assert!(support.edits.contains(&(0, 95, 0, AIR)));
    assert!(support.edits.contains(&(0, 96, 0, AIR)));
    assert_eq!(
        support
            .drops
            .iter()
            .filter(|(_, stack, _)| stack.item == item)
            .map(|(_, stack, _)| stack.count)
            .sum::<u16>(),
        1
    );
    for y in [95, 96] {
        let plan = plan_block_edit(&mut state, TickId::new(1), command(AIR, y)).unwrap();
        assert_eq!(plan.deltas.len(), 2);
        assert!(plan.deltas.iter().all(|d| d.block == AIR));
    }
    state.world.edit(0, 95, 0, AIR).unwrap();
    state.world.edit(0, 96, 0, crate::world::STONE).unwrap();
    assert_eq!(
        plan_block_edit(&mut state, TickId::new(1), command(lower, 95))
            .err()
            .unwrap()
            .kind(),
        ErrorKind::PermissionDenied
    );
    assert!(plan_block_edit(&mut state, TickId::new(1), command(upper, 95)).is_err());
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
