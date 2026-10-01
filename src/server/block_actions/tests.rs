use super::*;
use crate::server::durable::CommitAction;
use crate::server::durable::actions::BlockEditCommand;
use crate::server::effects::CellCoord;
use crate::server::server_state;
use crate::server::simulation::TickId;
use crate::world::{BlockId, world_to_chunk};
use std::fs;
use std::io::{self, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_save_dir(label: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("bloxgloom-{label}-{}-{stamp}", std::process::id()))
}

/// Minimal lifecycle hook: stages one edit through the builder and nothing
/// else. As a free `fn` it captures no environment, so it provably cannot
/// close over `&mut State`; the `BlockEditHook` binding below pins that.
fn probe_place(
    _context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    _tick: TickId,
    command: BlockEditCommand,
    _previous: BlockId,
) -> io::Result<CommitAction> {
    let coords = vec![(command.x, command.y, command.z, command.block)];
    let world_edits = builder.prepare_edits(&coords)?;
    Ok(CommitAction {
        client_id: Some(command.id),
        profile: Some(command.profile),
        action_id: Some(command.action_id),
        receipt_value: Some(command.receipt_value),
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits,
        deltas: Vec::new(),
        changed_cells: vec![CellCoord::new(command.x, command.y, command.z)],
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        sounds: Vec::new(),
        player_publication: None,
        entities: None,
    })
}

fn probe_deferred(
    _context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    _tick: TickId,
    command: BlockEditCommand,
    _previous: BlockId,
) -> io::Result<CommitAction> {
    let _ = builder.cached_block_or_request(
        command.x,
        command.y,
        command.z,
        "probe chunk is not resident",
    )?;
    unreachable!("missing chunk must defer the edit");
}

/// Hook that plans successfully while also recording a prefetch request for
/// a far cell. The prefetch miss is swallowed here; its recorded request
/// drains through `invoke_hook` without affecting this commit.
fn probe_ok_with_prefetch(
    context: &BlockActionContext,
    builder: &mut BlockCommitBuilder,
    tick: TickId,
    command: BlockEditCommand,
    previous: BlockId,
) -> io::Result<CommitAction> {
    let _ = builder.cached_block_or_request(command.x + 16, command.y, command.z, "probe prefetch");
    probe_place(context, builder, tick, command, previous)
}

fn edit_command(block: BlockId, x: i32, y: i32, z: i32) -> BlockEditCommand {
    BlockEditCommand {
        id: 1,
        profile: 17,
        action_id: 3,
        receipt_value: vec![7],
        x,
        y,
        z,
        block,
        slot: 0,
    }
}

#[test]
fn hook_stages_edits_through_builder_without_touching_live_world() {
    let path = temp_save_dir("hook-builder-commit");
    let mut state = server_state(11, path.clone()).unwrap();
    let (key, _) = world_to_chunk(0, 80, 0);
    state.world.get_chunk(key).unwrap();
    let before = state.world.cached_block(0, 80, 0).unwrap();
    let block = if before == crate::world::AIR {
        crate::world::STONE
    } else {
        crate::world::AIR
    };

    let hook: BlockEditHook = probe_place;
    let action = invoke_hook(
        hook,
        &mut state,
        TickId::new(1),
        edit_command(block, 0, 80, 0),
        before,
    )
    .unwrap();
    assert_eq!(action.world_edits.len(), 1);
    assert_eq!(action.changed_cells.len(), 1);
    // Staging prepares versioned edits but stays invisible until the WAL
    // receipt applies: the live world still serves the old block.
    assert_eq!(state.world.cached_block(0, 80, 0), Some(before));

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn hook_chunk_requests_drain_even_when_planning_defers() {
    let path = temp_save_dir("hook-builder-defer");
    let mut state = server_state(13, path.clone()).unwrap();
    state.world.reset_cache_for_test(1);
    let (near, _) = world_to_chunk(0, 80, 0);
    let (far, _) = world_to_chunk(16, 80, 0);
    assert_ne!(near, far);
    state.world.get_chunk(near).unwrap();
    let before = state.world.cached_block(0, 80, 0).unwrap();

    let hook: BlockEditHook = probe_deferred;
    let error = invoke_hook(
        hook,
        &mut state,
        TickId::new(1),
        edit_command(before, 16, 80, 0),
        before,
    )
    .err()
    .expect("missing chunk must defer the edit");
    assert_eq!(error.kind(), ErrorKind::WouldBlock);
    assert!(state.loader.is_pending(far));

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn ok_hook_survives_failing_chunk_request() {
    let path = temp_save_dir("hook-prefetch-failure");
    let mut state = server_state(17, path.clone()).unwrap();
    let (near, _) = world_to_chunk(0, 80, 0);
    let (far, _) = world_to_chunk(16, 80, 0);
    assert_ne!(near, far);
    state.world.get_chunk(near).unwrap();
    let before = state.world.cached_block(0, 80, 0).unwrap();
    let block = if before == crate::world::AIR {
        crate::world::STONE
    } else {
        crate::world::AIR
    };
    // Fail every recorded request closed: the prefetch for the far chunk can
    // never be queued, but the planned commit must still come back.
    state.loader.stop_for_test();

    let hook: BlockEditHook = probe_ok_with_prefetch;
    let action = invoke_hook(
        hook,
        &mut state,
        TickId::new(1),
        edit_command(block, 0, 80, 0),
        before,
    )
    .expect("failing prefetch must not discard the planned commit");
    assert_eq!(action.world_edits.len(), 1);
    assert!(!state.loader.is_pending(far));

    drop(state);
    fs::remove_dir_all(path).unwrap();
}
