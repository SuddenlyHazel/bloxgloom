//! Competing real clients and native readonly reservation fences.
use super::*;

#[test]
fn concurrent_profile_inventory_transfers_retry_without_lost_items() {
    let fixture = fixture(true);
    let mut state = Box::new(fixture.open().unwrap());
    state.admission_limit = 3;
    seeded(&state, 10, 124);
    let second = PROFILE + 2;
    let source = state.inventory_store.load(PROFILE).unwrap();
    state.inventory_store.save(second, &source).unwrap();
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut first = Peer::connect(address, Arc::clone(&catalog));
        let mut second = Peer::connect_profile(address, Arc::clone(&catalog), second);
        thread::scope(|scope| {
            for peer in [&mut first, &mut second] {
                scope.spawn(move || {
                    for remaining in (2..=9).rev() {
                        let request = peer.request(12);
                        let (accepted, reason) = peer.send(&request);
                        assert!(accepted, "{reason}");
                        slot(peer, 0, remaining);
                        assert!(peer.send(&request).0, "replay failed");
                    }
                });
            }
        });
        let mut target = Peer::connect_profile(address, catalog, TARGET);
        slot(&mut target, 2, 16);
    });
    let state = fixture.open().unwrap();
    for profile in [PROFILE, second] {
        assert_eq!(
            state.inventory_store.load(profile).unwrap().slots[0]
                .as_ref()
                .unwrap()
                .count,
            2
        );
    }
    assert_eq!(
        state.inventory_store.load(TARGET).unwrap().slots[2]
            .as_ref()
            .unwrap()
            .count,
        16
    );
}

#[test]
fn readonly_inventory_reservations_fence_writers_and_detect_stale_revisions() {
    use crate::server::{
        durable::{CommitAction, StageError, TerrainReads},
        simulation::TickId,
    };
    let fixture = fixture(true);
    let mut state = fixture.open().unwrap();
    seeded(&state, 10, 124);
    let before = state.inventory_store.load(TARGET).unwrap();
    let mut reads = TerrainReads::default();
    reads.inventory(TARGET, before.revision).unwrap();
    assert!(reads.inventories_current(&state));
    let action = |reads, changes| CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: vec![],
        terrain_reads: reads,
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entities: None,
        entity_wakes: vec![],
        player_publication: None,
        owner_changes: changes,
    };
    let store = state.inventory_store.clone();
    let catalog = state.world.catalog_arc();
    let marker = |profile| {
        let before = store.load(profile).unwrap();
        let mut after = before.clone();
        after.revision += 1;
        let slot = after.slots[0]
            .get_or_insert_with(|| Stack::new(catalog.item_by_key("bloxgloom:stick").unwrap(), 0));
        slot.count += 1;
        crate::server::journal::Change::new(
            crate::server::durable::inventory_state_key(profile),
            crate::inventory::InventoryStore::encode_snapshot_with_catalog(&before, &catalog)
                .unwrap(),
            crate::inventory::InventoryStore::encode_snapshot_with_catalog(&after, &catalog)
                .unwrap(),
        )
    };
    // Distinct write participants allow both read reservations to coexist.
    for profile in [PROFILE, PROFILE + 2] {
        let reader = action(reads.clone(), vec![marker(profile)]);
        assert!(
            state
                .durability
                .try_stage(TickId::new(1), &reader, None)
                .unwrap()
        );
    }
    assert!(state.durability.profile_reserved(TARGET));
    let writer = action(TerrainReads::default(), vec![marker(TARGET)]);
    assert!(matches!(
        state.durability.try_stage(TickId::new(1), &writer, None),
        Err(StageError::Conflict)
    ));
    let mut after = before;
    after.revision += 1;
    state.durability.inventory_overlay.insert(TARGET, after);
    assert!(!reads.inventories_current(&state));
    assert!(reads.inventory(TARGET, 1).is_err());
}
