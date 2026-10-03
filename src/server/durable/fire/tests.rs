use super::*;
use crate::server::durable::CommitAction;
use crate::server::journal::StateKey;
use crate::server::server_state;
use crate::world::{AIR, Chunk, GLOWSTONE, WOOD, world_to_chunk};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct BurnExtension;
struct BurnFuel;
impl bloxgloom_host_api::gameplay::Handler for BurnFuel {
    fn handle(
        &self,
        context: &mut bloxgloom_host_api::gameplay::Context<'_>,
        event: &bloxgloom_host_api::gameplay::Event,
    ) -> Result<(), bloxgloom_host_api::gameplay::Error> {
        use bloxgloom_host_api::gameplay::{Event, RemovalCause};
        let Event::BlockRemoved {
            cell,
            cause: RemovalCause::Burn,
            ..
        } = event
        else {
            return Err(bloxgloom_host_api::gameplay::Error::Invalid(
                "expected burned fuel".into(),
            ));
        };
        context.spawn_drop(cell.map(|v| v as f32 + 0.5), "bloxgloom:stick", 1, 250)
    }
}
impl bloxgloom_host_api::Extension for BurnExtension {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        use bloxgloom_host_api::content::{Block, BlockState, FaceTextures, Geometry, Material};
        registrar.block(Block {
            acoustics: None,
            key: "test:fire_fuel".into(),
            name: "FIRE FUEL".into(),
            swatch: [0.4, 0.6, 0.3, 1.0],
            textures: FaceTextures::uniform("bloxgloom:grass_top"),
            geometry: Geometry::Cube,
            material: Material::Opaque,
            solid: true,
            replaceable: false,
            supports_plant: true,
            flammable: true,
            emission: 0,
            sky_attenuation: 0,
            reflectance: [75, 170, 65],
            properties: Vec::new(),
            states: vec![BlockState::default()],
        })?;
        registrar.gameplay_handler(bloxgloom_host_api::gameplay::HandlerRegistration {
            key: "test:burn_fuel".into(),
            version: 1,
            event: bloxgloom_host_api::gameplay::EventKind::BlockRemoved,
            target: Some("test:fire_fuel".into()),
            handler: std::sync::Arc::new(BurnFuel),
        })
    }
}

#[test]
fn fire_burn_uses_public_removal_and_support_handlers_in_one_receipt() {
    use crate::world::RED_FLOWER;
    let path = temp_save();
    let startup = crate::server::startup::ServerStartup::new(std::sync::Arc::new(
        crate::content::Catalog::builtins(),
    ))
    .with_extension(&BurnExtension)
    .unwrap();
    let mut state = crate::server::server_state_with_startup(71, path.clone(), 8, startup).unwrap();
    let fuel = state
        .world
        .catalog()
        .state_by_key("test:fire_fuel")
        .unwrap();
    let (source, local) = world_to_chunk(8, 95, 8);
    let cell = Chunk::index(local).unwrap() as u16;
    let edits = state
        .world
        .prepare_edits(&[
            (8, 95, 8, GLOWSTONE),
            (9, 95, 8, fuel),
            (9, 96, 8, RED_FLOWER),
        ])
        .unwrap();
    let seed = state
        .fire
        .prepare_seed_from_edit(TickId::new(1), source, cell, GLOWSTONE)
        .unwrap()
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: edits,
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: Some(seed.clone()),
        clock_change: None,
        weather_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        sounds: Vec::new(),
        player_publication: None,
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .unwrap()
    );
    state.fire.mark_seed_submitted(&seed).unwrap();
    drain_wal(&mut state);
    run_delivery(&mut state, TickId::new(3)).unwrap();
    drain_wal(&mut state);
    state.durability.publish_queue.clear();
    run_source(
        &mut state,
        TickId::new(1 + crate::server::fire::SPREAD_DELAY_TICKS),
    )
    .unwrap();
    assert!(
        state.durability.publish_queue.is_empty(),
        "flames must wait for the WAL receipt"
    );
    assert_eq!(
        state.world.cached_block(9, 95, 8),
        Some(fuel),
        "burn must wait for WAL receipt"
    );
    drain_wal(&mut state);
    assert_eq!(state.world.cached_block(9, 95, 8), Some(AIR));
    assert_eq!(state.world.cached_block(9, 96, 8), Some(AIR));
    let effects = &state.durability.publish_queue;
    assert_eq!(
        effects.len(),
        1,
        "burn and support cleanup share one publication"
    );
    assert_eq!(
        effects[0].fire_bursts,
        vec![[9, 95, 8]],
        "only the burned fuel gets a flame, not its unsupported flower"
    );
    assert_eq!(effects[0].deltas.len(), 2);
    let drops = crate::server::drops::nearby(&state.entities, [9.5, 95.5, 8.5]);
    assert_eq!(
        drops
            .iter()
            .filter(|drop| drop.item == crate::items::STICK)
            .map(|drop| drop.count)
            .sum::<u16>(),
        1
    );
    assert_eq!(
        drops
            .iter()
            .filter(|drop| drop.item == crate::items::ItemId::new(RED_FLOWER.get()))
            .map(|drop| drop.count)
            .sum::<u16>(),
        1
    );
    drop(state);
    let startup = crate::server::startup::ServerStartup::new(std::sync::Arc::new(
        crate::content::Catalog::builtins(),
    ))
    .with_extension(&BurnExtension)
    .unwrap();
    let mut state = crate::server::server_state_with_startup(71, path.clone(), 8, startup).unwrap();
    assert_eq!(state.world.get_block(9, 95, 8).unwrap(), AIR);
    assert_eq!(state.world.get_block(9, 96, 8).unwrap(), AIR);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

fn temp_save() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bloxgloom-live-fire-{}-{nonce}",
        std::process::id()
    ))
}

fn drain_wal(state: &mut State) {
    for _ in 0..1_000 {
        super::super::receipt::poll_journal_receipts(state).unwrap();
        if state.durability.pending.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("fire WAL did not produce a receipt");
}

#[test]
fn seeded_fire_survives_restart_and_burns_only_after_wal_receipt() {
    // Repeated startup/replay calls use large State temporaries in debug builds.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(check_seeded_fire_restart)
        .unwrap()
        .join()
        .unwrap();
}

fn check_seeded_fire_restart() {
    let path = temp_save();
    let mut state = server_state(71, path.clone()).unwrap();
    let (source, local) = world_to_chunk(8, 96, 8);
    let cell = Chunk::index(local).unwrap() as u16;
    let edits = state
        .world
        .prepare_edits(&[(8, 96, 8, GLOWSTONE), (9, 96, 8, WOOD)])
        .unwrap();
    let seed = state
        .fire
        .prepare_seed_from_edit(TickId::new(1), source, cell, GLOWSTONE)
        .unwrap()
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: edits,
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: Some(seed.clone()),
        clock_change: None,
        weather_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        sounds: Vec::new(),
        player_publication: None,
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .unwrap()
    );
    state.fire.mark_seed_submitted(&seed).unwrap();
    drain_wal(&mut state);
    assert_eq!(state.world.cached_block(9, 96, 8), Some(WOOD));
    drop(state);

    state = server_state(71, path.clone()).unwrap();
    assert_eq!(state.world.cached_block(9, 96, 8), Some(WOOD));
    assert_eq!(
        state.recovered_tick, 1,
        "a future ignition must not advance the recovered clock"
    );
    assert!(
        state
            .fire
            .snapshot()
            .checkpoint_values()
            .iter()
            .any(|(key, value)| key.domain == "bloxgloom:fire_pending" && !value.is_empty())
    );
    run_delivery(&mut state, TickId::new(3)).unwrap();
    drain_wal(&mut state);
    assert_eq!(state.world.cached_block(9, 96, 8), Some(WOOD));
    drop(state);
    // Recover again with the ignition already delivered into a future frontier.
    state = server_state(71, path.clone()).unwrap();
    assert_eq!(
        state.recovered_tick, 3,
        "a future frontier must not advance the recovered clock"
    );
    run_source(
        &mut state,
        TickId::new(crate::server::fire::SPREAD_DELAY_TICKS),
    )
    .unwrap();
    drain_wal(&mut state);
    assert_eq!(
        state.world.cached_block(9, 96, 8),
        Some(WOOD),
        "restart must preserve the ignition deadline"
    );
    run_source(
        &mut state,
        TickId::new(1 + crate::server::fire::SPREAD_DELAY_TICKS),
    )
    .unwrap();
    drain_wal(&mut state);
    assert_eq!(state.world.cached_block(9, 96, 8), Some(AIR));
    drop(state);

    // The synced BGED/fire record, not the in-memory owner slot, must be
    // sufficient to restore the burn after a second process restart.
    state = server_state(71, path.clone()).unwrap();
    assert_eq!(state.world.cached_block(9, 96, 8), Some(AIR));
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn dirty_fire_keys_use_one_checkpoint_job_and_revision_fenced_receipt() {
    let path = temp_save();
    let mut state = server_state(73, path.clone()).unwrap();
    let (source, local) = world_to_chunk(8, 96, 8);
    let cell = Chunk::index(local).unwrap() as u16;
    let edits = state.world.prepare_edits(&[(8, 96, 8, GLOWSTONE)]).unwrap();
    let seed = state
        .fire
        .prepare_seed_from_edit(TickId::new(1), source, cell, GLOWSTONE)
        .unwrap()
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: edits,
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: Some(seed.clone()),
        clock_change: None,
        weather_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        sounds: Vec::new(),
        player_publication: None,
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .unwrap()
    );
    state.fire.mark_seed_submitted(&seed).unwrap();
    drain_wal(&mut state);

    let fire_count = state
        .durability
        .dirty_checkpoints
        .keys()
        .filter(|key| {
            matches!(
                key.domain.as_str(),
                "bloxgloom:fire_frontier" | "bloxgloom:fire_pending" | "bloxgloom:fire_cursor"
            )
        })
        .count();
    assert!(fire_count > 1);
    super::super::checkpoint::submit_dirty_checkpoints(&mut state);
    let batch = state.durability.fire_checkpoint_batch.as_ref().unwrap();
    assert_eq!(batch.covered.len(), fire_count);
    let revised_key = batch.covered[0].0.clone();
    let revised_snapshot = state.durability.dirty_checkpoints[&revised_key]
        .snapshot
        .clone();
    state
        .durability
        .remember_checkpoint(revised_key.clone(), revised_snapshot);
    assert!(
        state
            .durability
            .checkpoint_inflight
            .contains_key(&StateKey::new(
                "bloxgloom:fire_checkpoint_batch",
                Vec::new()
            ))
    );

    for _ in 0..1_000 {
        super::super::checkpoint::process_checkpoint_receipts(
            &mut state,
            std::time::Instant::now(),
        );
        if state.durability.fire_checkpoint_batch.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(state.durability.fire_checkpoint_batch.is_none());
    assert!(
        state
            .durability
            .dirty_checkpoints
            .contains_key(&revised_key)
    );
    assert_eq!(
        state
            .durability
            .dirty_checkpoints
            .keys()
            .filter(|key| matches!(
                key.domain.as_str(),
                "bloxgloom:fire_frontier" | "bloxgloom:fire_pending" | "bloxgloom:fire_cursor"
            ))
            .count(),
        1
    );
    super::super::checkpoint::submit_dirty_checkpoints(&mut state);
    assert_eq!(
        state
            .durability
            .fire_checkpoint_batch
            .as_ref()
            .unwrap()
            .covered,
        vec![(
            revised_key.clone(),
            state.durability.dirty_checkpoints[&revised_key].revision
        )]
    );
    for _ in 0..1_000 {
        super::super::checkpoint::process_checkpoint_receipts(
            &mut state,
            std::time::Instant::now(),
        );
        if state.durability.fire_checkpoint_batch.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(state.durability.fire_checkpoint_batch.is_none());
    assert!(
        !state
            .durability
            .dirty_checkpoints
            .contains_key(&revised_key)
    );
    assert!(path.join("fire/checkpoints.fire").exists());
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn checkpoint_pressure_admits_a_durable_owner_prefix_without_losing_the_remainder() {
    let path = temp_save();
    let mut state = server_state(72, path.clone()).unwrap();
    assert!(state.durability.dirty_checkpoints.is_empty());
    for index in 0..MAX_DIRTY_CHECKPOINT_KEYS - 2 {
        state.durability.remember_checkpoint(
            StateKey::new("test:pressure", (index as u32).to_le_bytes().to_vec()),
            vec![1],
        );
    }
    let owners: BTreeMap<_, _> = (0..32)
        .map(|x| (ChunkKey { x, y: 4, z: 0 }, vec![256]))
        .collect();
    let wave = state
        .fire
        .prepare_benchmark_frontier_wave(&owners, TickId::new(1))
        .unwrap();
    let candidates = wave.transactions.len();
    assert!(candidates > 1);
    stage_wave(&mut state, TickId::new(1), wave).unwrap();
    assert_eq!(state.durability.pending.len(), 1);
    assert_eq!(state.fire.load_metrics().admitted_transactions, 1);
    assert_eq!(
        state.fire.load_metrics().full_deferred_transactions,
        (candidates - 1) as u64
    );
    drain_wal(&mut state);
    assert_eq!(state.fire.load_metrics().frontier_cells, 1);
    drop(state);

    let state = server_state(72, path.clone()).unwrap();
    assert_eq!(state.fire.load_metrics().frontier_cells, 1);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
