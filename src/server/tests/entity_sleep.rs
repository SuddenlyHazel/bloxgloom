//! Sleeping dependency checks through real worker dispatch and WAL recovery.
use super::*;
use crate::server::durable::{CommitAction, process_durable_actions, queue_interaction_actions};
use crate::server::entities::{EntityId, EntityLocation};

const STONE_ITEM: crate::items::ItemId = crate::items::ItemId::new(crate::world::STONE.get());

#[test]
fn terrain_edit_interrupts_idle_mossbun_on_the_next_tick_across_a_seam() {
    use crate::server::entities::{EntityPayload, EntitySpawn, mossbun::Mossbun};
    let save = TestSave::new("idle-mossbun-support-wake");
    let mut state = state_for(&save, 7);
    let y = crate::world::MAX_GENERATED_HEIGHT + 32;
    let rest = [16.05, y as f32, 0.5];
    reside_neighbourhood(&mut state, rest);
    for x in [15, 16] {
        edit(&mut state, (x, y - 1, 0), crate::world::STONE, 1);
        receive(&mut state, true);
    }
    let mut spawn = empty_action();
    spawn.entities = Some(
        state
            .entities
            .prepare_spawn(EntitySpawn::Mobile {
                entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
                position: rest,
                payload: EntityPayload::new(Mossbun::default()),
                spawn_tick: 1,
            })
            .unwrap(),
    );
    let id = spawn.entities.as_ref().unwrap().entity_id();
    stage(&mut state, &spawn, 1);
    receive(&mut state, true);
    let mut idle = empty_action();
    idle.entities = Some(
        state
            .entities
            .prepare_update(
                id,
                state.entities.snapshot(id).unwrap().revision,
                crate::server::entities::EntityPatch {
                    next_tick: Some(Some(100)),
                    ..Default::default()
                },
            )
            .unwrap(),
    );
    stage(&mut state, &idle, 2);
    receive(&mut state, true);
    // Process harmless placement hints first: they must not advance idle AI.
    tick_once(&mut state, TickId::new(2), Instant::now()).unwrap();
    tick_once(&mut state, TickId::new(3), Instant::now()).unwrap();
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, Some(100));
    for x in [15, 16] {
        edit(&mut state, (x, y - 1, 0), AIR, 4);
        receive(&mut state, true);
    }
    tick_once(&mut state, TickId::new(4), Instant::now()).unwrap();
    assert_eq!(
        position(&state, id),
        rest,
        "wake cannot cascade in its producing tick"
    );
    tick_once(&mut state, TickId::new(5), Instant::now()).unwrap();
    let first = position(&state, id);
    assert!(
        first[1] < rest[1],
        "support loss must interrupt the idle deadline"
    );
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, Some(6));
    tick_once(&mut state, TickId::new(6), Instant::now()).unwrap();
    tick_once(&mut state, TickId::new(7), Instant::now()).unwrap();
    let second = position(&state, id);
    assert!(
        first[1] - second[1] > rest[1] - first[1],
        "gravity accelerates"
    );
    assert!(!state.durability.failed);
}

fn empty_action() -> CommitAction {
    CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: None,
        entity_wakes: Vec::new(),
    }
}

fn stage(state: &mut State, action: &CommitAction, tick: u64) {
    let permit = action.entities.as_ref().map(|_| {
        state
            .durability
            .entity_mirror
            .try_reserve_durable()
            .unwrap()
            .unwrap()
    });
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), action, permit)
            .unwrap()
    );
}

// Wait for precisely the submitted receipts, not more simulation iterations.
// Forward them back through the real receipt/apply path with a bounded timeout.
fn receive(state: &mut State, apply: bool) {
    for pending in &mut state.durability.pending {
        let receipt = pending
            .receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let (sender, receiver) = mpsc::channel();
        sender.send(receipt).unwrap();
        pending.receiver = receiver;
    }
    if apply {
        crate::server::durable::poll_journal_receipts(state).unwrap();
        assert!(state.durability.pending.is_empty());
    }
}

fn edit(state: &mut State, cell: (i32, i32, i32), block: crate::world::BlockId, tick: u64) {
    let mut action = empty_action();
    action.world_edits = state
        .world
        .prepare_edits(&[(cell.0, cell.1, cell.2, block)])
        .unwrap();
    action
        .changed_cells
        .push(crate::server::effects::CellCoord {
            x: cell.0,
            y: cell.1,
            z: cell.2,
        });
    stage(state, &action, tick);
}

fn dispatch(state: &mut State, barrier: u64) {
    queue_interaction_actions(state, TickId::new(barrier));
    process_durable_actions(state, TickId::new(barrier + 1), Instant::now()).unwrap();
    receive(state, true);
}

fn position(state: &State, id: EntityId) -> [f32; 3] {
    let EntityLocation::Mobile { position } = state.entities.snapshot(id).unwrap().location else {
        panic!("drop must remain mobile")
    };
    position
}

fn settled(state: &mut State, x: f32) -> (EntityId, (i32, i32, i32)) {
    settled_at(state, x, 1)
}

fn settled_at(state: &mut State, x: f32, tick: u64) -> (EntityId, (i32, i32, i32)) {
    let y = crate::world::MAX_GENERATED_HEIGHT + 32;
    let at = [x, y as f32, 0.5];
    reside_neighbourhood(state, at);
    let cell = (
        (x - crate::server::drops::DROP_RADIUS).floor() as i32,
        y - 1,
        0,
    );
    edit(state, cell, crate::world::STONE, tick);
    receive(state, true);
    let mut action = empty_action();
    action.entities = crate::server::drops::plan_spawns(
        &state.entities,
        state.world.catalog(),
        &[(at, STONE_ITEM, 7, Duration::ZERO)],
        tick,
        crate::server::drops::unix_ms(),
    )
    .unwrap();
    let id = action.entities.as_ref().unwrap().entity_id();
    stage(state, &action, tick);
    receive(state, true);
    dispatch(state, tick + 1);
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, None);
    assert_eq!(position(state, id)[1], y as f32 + 0.18);
    (id, cell)
}

#[test]
fn sleeping_drop_rechecks_support_without_hints_and_coalesces_duplicates() {
    let save = TestSave::new("sleep-support");
    let mut state = state_for(&save, 7);
    let (id, cell) = settled(&mut state, 0.5);
    let before = state.entities.snapshot(id).unwrap();
    // Cover an enclosed resting plane as well: unchanged support must not
    // rerun the inclusive active sweep and climb through the column above it.
    edit(
        &mut state,
        (cell.0, cell.1 + 1, cell.2),
        crate::world::STONE,
        3,
    );
    receive(&mut state, true);
    dispatch(&mut state, 3);
    assert_eq!(
        state.entities.snapshot(id).unwrap().revision,
        before.revision
    );
    assert_eq!(
        state.entities.snapshot(id).unwrap().motion_revision,
        before.motion_revision
    );
    edit(
        &mut state,
        (cell.0, cell.1 + 1, cell.2),
        crate::world::AIR,
        4,
    );
    receive(&mut state, true);
    edit(&mut state, cell, crate::world::AIR, 4);
    receive(&mut state, true);
    let rest = position(&state, id);
    // Lose all block effects, then inject duplicate hints: neither is authority.
    state.pending_block_changes.clear();
    for _ in 0..512 {
        state.durability.hint_entity_wake(id);
    }
    assert_eq!(state.durability.pending_wakes, vec![id]);
    queue_interaction_actions(&mut state, TickId::new(4));
    assert_eq!(state.durability.queued.len(), 1);
    assert_eq!(position(&state, id), rest, "admission is not motion");
    process_durable_actions(&mut state, TickId::new(5), Instant::now()).unwrap();
    let after = state.entities.snapshot(id).unwrap();
    assert!(position(&state, id)[1] < rest[1]);
    assert_eq!(after.motion_revision, before.motion_revision + 1);
    assert_eq!(after.next_tick, Some(6));
    assert_eq!(drop_stack(&state, id).unwrap().count, 7);
    assert_eq!(state.entities.len(), 1);
}

#[test]
fn sleeping_drop_recovers_after_durable_support_removal_before_apply_or_wake() {
    let save = TestSave::new("sleep-edit-crash");
    let mut state = state_for(&save, 7);
    // The footprint crosses a chunk boundary; recovery must not depend on
    // the edited cell having the same chunk owner as the suspended entity.
    let (id, cell) = settled(&mut state, 16.05);
    let rest = position(&state, id);
    edit(&mut state, cell, crate::world::AIR, 4);
    receive(&mut state, false); // fsynced, but deliberately not applied
    assert_eq!(
        state.world.cached_block(cell.0, cell.1, cell.2),
        Some(crate::world::STONE)
    );
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, None);
    drop(state);

    let mut recovered = state_for(&save, 7);
    assert_eq!(recovered.entities.snapshot(id).unwrap().next_tick, None);
    assert!(recovered.durability.pending_wakes.is_empty());
    assert!(recovered.pending_block_changes.is_empty());
    // No resident capture: one rejected attempt must not erase eligibility.
    let unavailable =
        crate::server::durable::actions::entity::capture_tick_input(&mut recovered, id, 6, true)
            .err()
            .expect("the recovered edit may be resident, but not the whole read radius");
    assert_eq!(unavailable.kind(), io::ErrorKind::WouldBlock);
    dispatch(&mut recovered, 5);
    assert_eq!(position(&recovered, id), rest);
    assert_eq!(
        recovered.entities.suspended_tick_after(None),
        Some((id, id))
    );
    // Explicitly deliver availability; no sleeps or loader-timing assumptions.
    reside_neighbourhood(&mut recovered, rest);
    assert_eq!(
        recovered.world.cached_block(cell.0, cell.1, cell.2),
        Some(crate::world::AIR)
    );
    dispatch(&mut recovered, 6);
    assert!(position(&recovered, id)[1] < rest[1]);
    assert_eq!(drop_stack(&recovered, id).unwrap().count, 7);
}

#[test]
fn sleeping_drop_survives_full_admission_and_deleted_work_is_retired() {
    let save = TestSave::new("sleep-full-admission");
    let mut state = state_for(&save, 7);
    let (id, cell) = settled(&mut state, 0.5);
    let rest = position(&state, id);
    edit(&mut state, cell, crate::world::AIR, 4);
    receive(&mut state, true);
    state.pending_block_changes.clear();
    for _ in 0..crate::server::durable::MAX_DEFERRED_DURABLE_ACTIONS {
        state
            .durability
            .queued
            .push_back(DurableRequest::Pickup { id: u64::MAX });
    }
    queue_interaction_actions(&mut state, TickId::new(4));
    assert_eq!(state.durability.entity_sleep_cursor, None);
    process_durable_actions(&mut state, TickId::new(5), Instant::now()).unwrap();
    assert_eq!(position(&state, id), rest);
    dispatch(&mut state, 5);
    assert!(position(&state, id)[1] < rest[1]);
    // Removal uses the same atomic item take path as pickup. Stale hint and
    // recheck state may not retain a dead entity or resurrect its seven items.
    let mut action = empty_action();
    action.entities = crate::server::drops::plan_take(&state.entities, &[(id, 7)]).unwrap();
    stage(&mut state, &action, 7);
    receive(&mut state, true);
    state.durability.hint_entity_wake(id);
    dispatch(&mut state, 7);
    assert!(state.entities.snapshot(id).is_none());
    assert!(state.entities.suspended_tick_after(None).is_none());
    assert!(state.durability.queued.is_empty());
    assert!(state.durability.pending_wakes.is_empty());
    assert!(state.durability.pending.is_empty());
}

#[test]
fn sleeping_lane_rotates_past_unavailable_work_under_sustained_due_and_hint_pressure() {
    let save = TestSave::new("sleep-scarce-turns");
    let mut state = state_for(&save, 7);
    let (blocked, blocked_cell) = settled(&mut state, 0.5);
    let (ready, ready_cell) = settled(&mut state, 64.5);
    let rest = position(&state, ready);
    edit(&mut state, blocked_cell, crate::world::AIR, 4);
    receive(&mut state, true);
    edit(&mut state, ready_cell, crate::world::AIR, 4);
    receive(&mut state, true);
    drop(state);
    let mut state = state_for(&save, 7);
    reside_neighbourhood(&mut state, rest);
    let mut action = empty_action();
    action.entities = crate::server::drops::plan_spawns(
        &state.entities,
        state.world.catalog(),
        &[([66.5, rest[1] + 2.0, 0.5], STONE_ITEM, 1, Duration::ZERO)],
        1,
        crate::server::drops::unix_ms(),
    )
    .unwrap();
    let airborne = action.entities.as_ref().unwrap().entity_id();
    stage(&mut state, &action, 1);
    receive(&mut state, true);
    let airborne_revision = state.entities.snapshot(airborne).unwrap().motion_revision;
    // Six admission opportunities, exactly one slot each: due, hint, asleep,
    // due, hint, asleep. The unavailable first sleeper cannot pin the cursor.
    for turn in 0..6 {
        for raw in 10_000..10_512 {
            state
                .durability
                .hint_entity_wake(EntityId::new(raw).unwrap());
        }
        assert_eq!(state.durability.pending_wakes.len(), 256);
        for _ in 0..128 {
            state
                .durability
                .queued
                .push_back(DurableRequest::Pickup { id: u64::MAX });
        }
        for _ in 0..127 {
            state.durability.queued.push_back(DurableRequest::Expire);
        }
        dispatch(&mut state, 10 + turn);
    }
    assert_eq!(
        state.entities.snapshot(airborne).unwrap().motion_revision,
        airborne_revision + 2
    );
    assert_eq!(state.entities.snapshot(blocked).unwrap().next_tick, None);
    assert!(position(&state, ready)[1] < rest[1]);
    assert_eq!(drop_stack(&state, ready).unwrap().count, 7);
    assert_eq!(state.durability.entity_sleep_cursor, Some((ready, ready)));
}

#[test]
fn sleeping_expiry_removes_recheck_membership_and_stale_admitted_hint() {
    let save = TestSave::new("sleep-expiry");
    let mut state = state_for(&save, 7);
    let (id, _) = settled(&mut state, 0.5);
    queue_interaction_actions(&mut state, TickId::new(4));
    assert_eq!(state.durability.queued.len(), 1);
    let mut action = empty_action();
    action.entities = crate::server::drops::plan_expired(
        &state.entities,
        crate::server::drops::unix_ms() + crate::server::drops::LIFETIME.as_millis() as u64,
        256,
    )
    .unwrap();
    stage(&mut state, &action, 4);
    receive(&mut state, true);
    assert!(state.entities.suspended_tick_after(None).is_none());
    process_durable_actions(&mut state, TickId::new(5), Instant::now()).unwrap();
    assert_eq!(state.entities.len(), 0);
    assert!(state.durability.queued.is_empty());
    assert!(state.durability.pending_wakes.is_empty());
    drop(state);
    let recovered = state_for(&save, 7);
    assert!(recovered.entities.suspended_tick_after(None).is_none());
    assert!(recovered.entities.snapshot(id).is_none());
}

#[test]
fn live_harvest_receipt_invalidates_sleeping_support_without_notification_delivery() {
    let save = TestSave::new("sleep-live-harvest");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 99);
    let (id, cell) = settled(&mut state, 0.5);
    let rest = position(&state, id);
    // In reach, but outside pickup range; authorize the terrain snapshot as
    // a streaming client would. The command itself follows normal admission.
    let client = state.clients.get_mut(&session.id).unwrap();
    client.movement =
        crate::server::movement::MovementState::new([rest[0] + 3.0, rest[1], rest[2]], 0);
    client.sent.insert(world_to_chunk(cell.0, cell.1, cell.2).0);
    client.center = world_to_chunk(cell.0, cell.1, cell.2).0;
    crate::server::durable::handle_live_message(
        &mut state,
        session.id,
        ClientMessage::Edit {
            action_id: session.action_id(1),
            x: cell.0,
            y: cell.1,
            z: cell.2,
            block: AIR,
            slot: 0,
        },
        TickId::new(100),
    )
    .unwrap();
    process_durable_actions(&mut state, TickId::new(100), Instant::now()).unwrap();
    receive(&mut state, false);
    assert_eq!(
        state.world.cached_block(cell.0, cell.1, cell.2),
        Some(crate::world::STONE)
    );
    assert_eq!(
        position(&state, id),
        rest,
        "receipt gates ownership and terrain"
    );
    receive(&mut state, true);
    assert_eq!(state.world.cached_block(cell.0, cell.1, cell.2), Some(AIR));
    assert!(!state.pending_block_changes.is_empty());
    state.pending_block_changes.clear();
    state.durability.pending_wakes.clear();
    dispatch(&mut state, 101);
    assert!(position(&state, id)[1] < rest[1]);
    assert_eq!(
        drop_nearby(&state, rest)
            .iter()
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        8
    );
}

#[test]
fn new_sleepers_cannot_extend_the_current_recheck_pass() {
    let save = TestSave::new("sleep-pass-boundary");
    let mut state = state_for(&save, 7);
    let (first, support) = settled(&mut state, 0.5);
    let (second, _) = settled_at(&mut state, 64.5, 4);
    state.durability.entity_sleep_cursor = None;
    dispatch(&mut state, 7);
    assert_eq!(state.durability.entity_sleep_cursor, Some((first, second)));
    // This real spawn/settle dispatch visits the old pass's final sleeper
    // while adding a newer ID. New births cannot push that pass's end ahead.
    let (newest, _) = settled_at(&mut state, 128.5, 9);
    assert_eq!(state.durability.entity_sleep_cursor, Some((second, second)));
    let rest = position(&state, first);
    edit(&mut state, support, AIR, 12);
    receive(&mut state, true);
    state.pending_block_changes.clear();
    state.durability.pending_wakes.clear();
    dispatch(&mut state, 12);
    assert_eq!(state.durability.entity_sleep_cursor, Some((first, newest)));
    assert!(position(&state, first)[1] < rest[1]);
    assert_eq!(state.entities.snapshot(newest).unwrap().next_tick, None);
}
