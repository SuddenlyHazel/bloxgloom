use super::*;

#[test]
fn rejected_and_unconfirmed_owner_waves_publish_no_wakes_or_cursor() {
    let owners = [chunk_owner(0), chunk_owner(1), chunk_owner(2)];
    let runs = Arc::new(Mutex::new(Vec::new()));
    let mut harness = nudge_harness(
        1,
        1,
        8,
        4,
        false,
        [0, 0, 0],
        PatchUsage::default(),
        saturating_emitter(owners[0], owners[2], Arc::clone(&runs), 3),
    );
    let registered = harness
        .state
        .phase_plan
        .system(&harness.system)
        .unwrap()
        .clone();
    let kinds = Arc::clone(&harness.state.effect_kinds);
    let stage = |state: &mut crate::server::State, wave| {
        state.system_runtime.stage_registered_wave(
            &registered,
            TickId::new(2),
            wave,
            &kinds,
            &mut state.durability,
            &[],
        )
    };
    let next_id = harness.state.durability.next_id;
    harness.state.durability.rotation_requested = true;
    assert_eq!(
        stage(&mut harness.state, 0).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 0);
    assert_eq!(nudge_values(&harness), [0, 0, 0]);
    assert_eq!(harness.state.durability.next_id, next_id);
    assert!(harness.state.durability.reserved.is_empty());
    harness.state.durability.rotation_requested = false;
    let wave = stage(&mut harness.state, 1).unwrap().unwrap();
    assert_eq!(
        harness.state.system_runtime.pending_wake_count(),
        0,
        "unconfirmed hints must not escape"
    );
    assert_eq!(nudge_values(&harness), [0, 0, 0]);
    complete_barrier(&mut harness.state, wave.barrier()).unwrap();
    assert_eq!(nudge_values(&harness), [1, 0, 0]);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 1);
    runs.lock().unwrap().clear();
    // A later dispatch boundary in the SAME logical tick must not select the
    // hinted owner. The ordinary cursor advances to owner 1, not target 2.
    let same_tick = stage(&mut harness.state, 2).unwrap().unwrap();
    complete_barrier(&mut harness.state, same_tick.barrier()).unwrap();
    assert_eq!(*runs.lock().unwrap(), vec![owners[1]]);
    assert_eq!(harness.state.system_runtime.pending_wake_count(), 1);
}

#[test]
fn captured_owner_reads_share_reservations_and_fence_exclusive_waves() {
    let (_save, mut state, system, owners) = staged_harness();
    let third = chunk_owner(2);
    state
        .system_runtime
        .insert_owner(system.clone(), third, 30u64)
        .unwrap();
    let mut read = OwnerWrite::new(owners[0], 0, OwnerData::new(11u64));
    read.reads.push((owners[1], 0));
    let first = state
        .system_runtime
        .stage_test_wave(&system, vec![read], TickId::new(1), &mut state.durability)
        .unwrap();
    let mut read = OwnerWrite::new(third, 0, OwnerData::new(31u64));
    read.reads.push((owners[1], 0));
    let second = state
        .system_runtime
        .stage_test_wave(&system, vec![read], TickId::new(1), &mut state.durability)
        .unwrap();
    let exclusive = |state: &mut crate::server::State| {
        state.system_runtime.stage_test_wave(
            &system,
            vec![OwnerWrite::new(owners[1], 0, OwnerData::new(21u64))],
            TickId::new(1),
            &mut state.durability,
        )
    };
    assert_eq!(
        exclusive(&mut state).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(poll_to_applied(&mut state, first), 1);
    assert_eq!(
        exclusive(&mut state).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(poll_to_applied(&mut state, second), 1);
    let writer = exclusive(&mut state).unwrap();
    assert_eq!(poll_to_applied(&mut state, writer), 1);
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owners[1]),
        Some((1, 21))
    );
    assert!(state.durability.reserved.is_empty());
}
