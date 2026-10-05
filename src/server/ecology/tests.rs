use super::*;
use crate::world::{AIR, BlockId, DIRT, GRASS, LEAVES, STONE, WOOD, WOOD_X, WOOD_Z, World};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT_SAVE: AtomicU64 = AtomicU64::new(0);
struct Save(PathBuf);
impl Save {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "bloxgloom-ecology-{}-{}",
            std::process::id(),
            NEXT_SAVE.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Save {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
const CELL: [i32; 3] = [8, 88, 8];
fn edit(world: &mut World, cell: [i32; 3], block: BlockId) {
    world.edit(cell[0], cell[1], cell[2], block).unwrap();
}
fn check(world: &mut World, cell: [i32; 3], sun: bool) -> io::Result<Option<Rule>> {
    rules::check(
        world,
        &mut TerrainReads::default(),
        &mut Vec::new(),
        cell,
        sun,
    )
}

#[test]
fn connected_leaves_find_all_log_orientations_within_six_steps() {
    let save = Save::new();
    let mut world = World::new(7, save.0.clone()).unwrap();
    for x in 3..=8 {
        edit(&mut world, [x, 88, 8], LEAVES);
    }
    for log in [WOOD, WOOD_X, WOOD_Z] {
        edit(&mut world, [2, 88, 8], log);
        assert_eq!(check(&mut world, CELL, true).unwrap(), None);
    }
    edit(&mut world, [2, 88, 8], AIR);
    assert_eq!(
        check(&mut world, CELL, true).unwrap(),
        Some(Rule::LeafDecay)
    );
    edit(&mut world, [2, 88, 8], LEAVES);
    edit(&mut world, [1, 88, 8], WOOD);
    assert_eq!(
        check(&mut world, CELL, true).unwrap(),
        Some(Rule::LeafDecay)
    );
}

#[test]
fn named_leaves_accept_oriented_wood_but_not_directional_stone() {
    let save = Save::new();
    let mut world = World::new(7, save.0.clone()).unwrap();
    let state = |key: &str| crate::content::catalog().state_by_key(key).unwrap();
    edit(&mut world, CELL, state("bloxgloom:cherry_leaves"));
    for axis in ["x", "y", "z"] {
        edit(
            &mut world,
            [7, 88, 8],
            state(&format!("bloxgloom:cherry_log[axis={axis}]")),
        );
        assert_eq!(check(&mut world, CELL, true).unwrap(), None);
    }
    edit(&mut world, [7, 88, 8], state("bloxgloom:basalt[axis=y]"));
    assert_eq!(
        check(&mut world, CELL, true).unwrap(),
        Some(Rule::LeafDecay)
    );
}

#[test]
fn missing_seam_never_counts_as_absent_log_support() {
    let save = Save::new();
    let mut world = World::new(7, save.0.clone()).unwrap();
    let cell = [0, 88, 8];
    edit(&mut world, cell, LEAVES);
    let mut missing = Vec::new();
    assert_eq!(
        rules::check(
            &mut world,
            &mut TerrainReads::default(),
            &mut missing,
            cell,
            true
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(missing.contains(&ChunkKey { x: -1, y: 5, z: 0 }));
    assert_eq!(world.cached_block(-1, 88, 8), None);
    edit(&mut world, [-1, 88, 8], WOOD_Z);
    assert_eq!(check(&mut world, cell, true).unwrap(), None);
}

#[test]
fn grass_dies_under_solid_cover_but_plants_leave_it_alive() {
    let save = Save::new();
    let mut world = World::new(7, save.0.clone()).unwrap();
    edit(&mut world, CELL, GRASS);
    for cover in [STONE, WOOD, LEAVES] {
        edit(&mut world, [8, 89, 8], cover);
        assert_eq!(
            check(&mut world, CELL, false).unwrap(),
            Some(Rule::GrassDeath)
        );
    }
    for cover in [AIR, world::FERN, world::TALL_GRASS, world::RED_FLOWER] {
        edit(&mut world, [8, 89, 8], cover);
        assert_eq!(check(&mut world, CELL, true).unwrap(), None);
    }
}

#[test]
fn dirt_needs_daylight_and_a_clear_column_without_adjacent_grass() {
    let save = Save::new();
    let mut world = World::new(7, save.0.clone()).unwrap();
    edit(&mut world, CELL, DIRT);
    assert_eq!(
        check(&mut world, CELL, true).unwrap(),
        Some(Rule::GrassGrowth)
    );
    assert_eq!(check(&mut world, CELL, false).unwrap(), None);
    edit(&mut world, [8, 92, 8], STONE);
    assert_eq!(check(&mut world, CELL, true).unwrap(), None);
    edit(&mut world, [8, 92, 8], AIR);
    assert_eq!(
        check(&mut world, CELL, true).unwrap(),
        Some(Rule::GrassGrowth)
    );
    assert!(rules::daylight(crate::daylight::INITIAL_MS));
    assert!(!rules::daylight(crate::daylight::CYCLE_MS * 3 / 4));
}

#[test]
fn unloaded_saved_roof_above_generated_height_blocks_growth_after_restart() {
    let save = Save::new();
    {
        let mut world = World::new(7, save.0.clone()).unwrap();
        edit(&mut world, CELL, DIRT);
        edit(&mut world, [8, 120, 8], STONE);
    }
    let mut world = World::new(7, save.0.clone()).unwrap();
    world.get_block(CELL[0], CELL[1], CELL[2]).unwrap();
    assert_eq!(world.sky_scan_top(8, 8), Some(127));
    assert_eq!(
        check(&mut world, CELL, true).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    world.get_chunk(ChunkKey { x: 0, y: 6, z: 0 }).unwrap();
    assert_eq!(
        check(&mut world, CELL, true).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    world.get_chunk(ChunkKey { x: 0, y: 7, z: 0 }).unwrap();
    assert_eq!(check(&mut world, CELL, true).unwrap(), None);
    edit(&mut world, [8, 120, 8], AIR);
    assert_eq!(
        check(&mut world, CELL, true).unwrap(),
        Some(Rule::GrassGrowth)
    );
}

#[test]
fn rotating_scan_visits_every_voxel_and_bounds_each_tick() {
    let mut runtime = Runtime::default();
    runtime.refresh(BTreeSet::from([ChunkKey { x: -1, y: 5, z: -2 }]));
    let mut cells = BTreeSet::new();
    for _ in 0..128 {
        let samples = runtime.samples();
        assert_eq!(samples.len(), 32);
        cells.extend(samples);
    }
    assert_eq!(cells.len(), world::CHUNK_VOLUME);
    assert!(cells.contains(&[-16, 80, -32]));
    assert!(cells.contains(&[-1, 95, -17]));
}

#[test]
fn delay_is_stable_cancelable_and_queued_work_is_bounded() {
    let mut runtime = Runtime::default();
    runtime.observe(CELL, Some(Rule::GrassDeath), 1, 7);
    runtime.observe(CELL, Some(Rule::GrassDeath), 499, 7);
    assert_eq!(runtime.take_due(500), None);
    runtime.observe(CELL, None, 500, 7);
    assert_eq!(runtime.take_due(10_000), None);
    for x in 0..=schedule::MAX_PENDING as i32 {
        runtime.observe([x, 88, 8], Some(Rule::LeafDecay), 1, 7);
    }
    let served: Vec<_> = (0..schedule::MAX_QUEUED)
        .map(|_| runtime.take_due(10_000).unwrap().0)
        .collect();
    assert_eq!(runtime.take_due(10_000), None);
    for cell in served {
        runtime.finished(cell);
    }
    let mut served = schedule::MAX_QUEUED;
    while let Some((cell, _)) = runtime.take_due(10_000) {
        served += 1;
        runtime.finished(cell);
    }
    assert_eq!(served, schedule::MAX_PENDING);
    // Existing pending cells never acquire a second timer while queued.
    let mut runtime = Runtime::default();
    runtime.observe(CELL, Some(Rule::LeafDecay), 1, 7);
    let due = 1 + Rule::LeafDecay.delay(7, CELL);
    assert_eq!(runtime.take_due(due - 1), None);
    assert_eq!(runtime.take_due(due), Some((CELL, Rule::LeafDecay)));
    runtime.observe(CELL, Some(Rule::LeafDecay), due, 7);
    assert_eq!(runtime.take_due(due + 10_000), None);
    runtime.finished(CELL);
    runtime.observe(CELL, Some(Rule::LeafDecay), due, 7);
    assert!(runtime.take_due(due + 10_000).is_some());
}

#[test]
fn conversions_commit_after_receipt_and_recover_without_harvest_drops() {
    let save = Save::new();
    let mut state = crate::server::server_state(7, save.0.clone()).unwrap();
    edit(&mut state.world, CELL, GRASS);
    edit(&mut state.world, [8, 89, 8], STONE);
    edit(&mut state.world, [9, 88, 8], DIRT);
    let action = crate::server::durable::actions::plan_durable_request(
        &mut state,
        &DurableRequest::Ecology {
            cell: CELL,
            rule: Rule::GrassDeath,
        },
        TickId::new(2_000),
    )
    .unwrap()
    .unwrap();
    assert!(
        action.entities.is_none(),
        "conversion must not reward the replaced grass"
    );
    assert!(
        state
            .durability
            .try_stage(TickId::new(2_000), &action, None)
            .unwrap()
    );
    assert_eq!(state.world.cached_block(8, 88, 8), Some(GRASS));
    for tick in 2_001..3_001 {
        crate::server::durable::process_durable_actions(
            &mut state,
            TickId::new(tick),
            Instant::now(),
        )
        .unwrap();
        if state.world.cached_block(8, 88, 8) == Some(DIRT) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(state.world.cached_block(8, 88, 8), Some(DIRT));
    let grow = crate::server::durable::actions::plan_durable_request(
        &mut state,
        &DurableRequest::Ecology {
            cell: [9, 88, 8],
            rule: Rule::GrassGrowth,
        },
        TickId::new(3_001),
    )
    .unwrap()
    .unwrap();
    assert!(grow.entities.is_none(), "regrowth must not harvest dirt");
    assert!(
        state
            .durability
            .try_stage(TickId::new(3_001), &grow, None)
            .unwrap()
    );
    for tick in 3_002..4_002 {
        crate::server::durable::process_durable_actions(
            &mut state,
            TickId::new(tick),
            Instant::now(),
        )
        .unwrap();
        if state.world.cached_block(9, 88, 8) == Some(GRASS) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(state.world.cached_block(9, 88, 8), Some(GRASS));
    drop(state);
    let mut restarted = crate::server::server_state(7, save.0.clone()).unwrap();
    assert_eq!(restarted.world.get_block(8, 88, 8).unwrap(), DIRT);
    assert_eq!(restarted.world.get_block(9, 88, 8).unwrap(), GRASS);
}

#[test]
fn registered_tick_adapter_drives_delayed_conversion_and_finishes_its_request() {
    let save = Save::new();
    let mut state = crate::server::server_state(7, save.0.clone()).unwrap();
    edit(&mut state.world, CELL, GRASS);
    edit(&mut state.world, [8, 89, 8], STONE);
    state.ecology.changed([8, 89, 8], STONE);
    crate::server::runtime::tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    let due = 1 + Rule::GrassDeath.delay(7, CELL);
    crate::server::runtime::tick_once(&mut state, TickId::new(due - 1), Instant::now()).unwrap();
    assert_eq!(state.world.cached_block(8, 88, 8), Some(GRASS));
    assert!(
        !state
            .durability
            .queued
            .iter()
            .any(|r| matches!(r, DurableRequest::Ecology { .. }))
    );
    crate::server::runtime::tick_once(&mut state, TickId::new(due), Instant::now()).unwrap();
    assert!(
        state
            .durability
            .queued
            .iter()
            .any(|r| matches!(r, DurableRequest::Ecology { cell, .. } if *cell == CELL))
    );
    for tick in due + 1..due + 1_001 {
        crate::server::runtime::tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
        if state.world.cached_block(8, 88, 8) == Some(DIRT) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(state.world.cached_block(8, 88, 8), Some(DIRT));
    assert!(
        !state
            .durability
            .queued
            .iter()
            .any(|r| matches!(r, DurableRequest::Ecology { cell, .. } if *cell == CELL))
    );
}

#[test]
fn queued_decay_rechecks_rescued_leaves_and_fences_absent_support() {
    let save = Save::new();
    let mut state = crate::server::server_state(7, save.0.clone()).unwrap();
    edit(&mut state.world, CELL, LEAVES);
    let request = DurableRequest::Ecology {
        cell: CELL,
        rule: Rule::LeafDecay,
    };
    let action = crate::server::durable::actions::plan_durable_request(
        &mut state,
        &request,
        TickId::new(1_000),
    )
    .unwrap()
    .unwrap();
    assert_eq!(action.deltas[0].block, AIR);
    edit(&mut state.world, [8, 87, 8], WOOD_X);
    assert!(!action.terrain_reads.is_current());
    assert!(
        crate::server::durable::actions::plan_durable_request(
            &mut state,
            &request,
            TickId::new(1_001)
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn sky_ceiling_tracks_owner_worker_roof_installation_and_removal() {
    let save = Save::new();
    let mut world = World::new(7, save.0.clone()).unwrap();
    edit(&mut world, CELL, DIRT);
    world.get_block(8, 120, 8).unwrap();
    world.get_chunk(ChunkKey { x: 0, y: 6, z: 0 }).unwrap();
    for (block, expected_top, expected_rule) in
        [(STONE, 127, None), (AIR, 95, Some(Rule::GrassGrowth))]
    {
        let prepared = world.prepare_edits(&[(8, 120, 8, block)]).unwrap();
        let tasks = world.prepare_owner_apply_batch(prepared).unwrap();
        let receipts = tasks.into_iter().map(|task| task.run().unwrap()).collect();
        world.finish_owner_apply_batch(receipts).unwrap();
        assert_eq!(world.sky_scan_top(8, 8), Some(expected_top));
        assert_eq!(check(&mut world, CELL, true).unwrap(), expected_rule);
    }
}
