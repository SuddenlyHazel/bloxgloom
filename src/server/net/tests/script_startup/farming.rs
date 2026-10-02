//! Representative package composition through production download and WAL paths.
use super::*;
use crate::client::PackageActionProbe;
use crate::inventory::Stack;
use crate::server::{
    durable::complete_barrier,
    parallel::OwnerKey,
    registry::SystemId,
    runtime::systems::{RegisteredWaveInputs, RegisteredWorldInputs},
    simulation::TickId,
};
use crate::world::{AIR, ChunkKey, STONE};
use std::time::Instant;

const PROFILE: u128 = 0xfab012;
const PLANT: [i32; 3] = [2, 81, 1];
const HARVEST: [i32; 3] = [3, 81, 0];

fn open(fixture: &Fixture) -> State {
    let started = Instant::now();
    let packages = std::env::var_os("BLOXGLOOM_FARMING_PRESSURE_PACKAGES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/farming-scale/packages")
        });
    let packages = std::fs::canonicalize(packages).unwrap();
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&packages)
        .unwrap();
    eprintln!("farming discovery/startup: {:?}", started.elapsed());
    crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap()
}

fn stage(state: &mut State, key: &str, tick: u64) {
    let id = SystemId::new(key).unwrap();
    let registered = state.phase_plan.system(&id).unwrap().clone();
    let mut missing = Vec::new();
    let pending = state
        .system_runtime
        .stage_registered_wave_with_world(
            &registered,
            TickId::new(tick),
            0,
            RegisteredWaveInputs {
                effects: &state.effect_kinds,
                durability: &mut state.durability,
                in_flight: &[],
                world: RegisteredWorldInputs {
                    environment: None,
                    world: Some(&mut state.world),
                    entities: Some(&state.entities),
                    lifecycles: Some(&state.lifecycles),
                    players: &[],
                    seed: state.seed,
                    missing: &mut missing,
                },
            },
        )
        .unwrap()
        .expect("fixture owner is due");
    assert!(missing.is_empty());
    complete_barrier(state, pending.barrier()).unwrap();
}

#[test]
fn farming_scale_downloads_plants_harvests_and_recovers_three_systems() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut state = Box::new(open(&fixture));
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=6 {
        for z in -1..=2 {
            state.world.edit(x, 79, z, STONE).unwrap();
            for y in 80..=83 {
                state.world.edit(x, y, z, AIR).unwrap();
            }
        }
    }
    let catalog = state.world.catalog_arc();
    assert_eq!(
        catalog
            .items()
            .filter(|item| item.key.starts_with("farm:"))
            .count(),
        192
    );
    let seedling = catalog.state_by_key("farm:barley_seedling").unwrap();
    let ripe = catalog.state_by_key("farm:barley_ripe").unwrap();
    let seed = catalog.item_by_key("farm:barley_seed").unwrap();
    let produce = catalog.item_by_key("farm:barley_produce").unwrap();
    state
        .world
        .edit(PLANT[0], PLANT[1], PLANT[2], STONE)
        .unwrap();
    state
        .world
        .edit(HARVEST[0], HARVEST[1], HARVEST[2], seedling)
        .unwrap();
    // Irrigation's committed wake schedules growth sooner than its own deadline.
    for tick in [1, 101, 201] {
        stage(&mut state, "farm:irrigation", tick);
        stage(&mut state, "farm:growth", tick + 1);
    }
    stage(&mut state, "farm:seasons", 203);
    assert_eq!(
        state
            .world
            .get_block(HARVEST[0], HARVEST[1], HARVEST[2])
            .unwrap(),
        ripe
    );
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(seed, 128));
    inventory.slots[1] = Some(Stack::new(produce, 127));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    gameplay::serve(state, |address| {
        let started = Instant::now();
        let mut client = PackageActionProbe::connect_for(
            &address.to_string(),
            PROFILE,
            fixture.0.join("client"),
            "farm:plant",
        );
        eprintln!("farming cold join: {:?}", started.elapsed());
        client.ready(PLANT, STONE, 0);
        assert!(client.title().contains("32 crops"));
        let request = client.click(PLANT, PLANT);
        let (accepted, reason) = client.result(&request);
        assert!(accepted, "{reason}");
        client.ready(PLANT, seedling, 1);
        assert_eq!(client.inventory_count(0), 127);
        for _ in 0..16 {
            if client.session_mut().event() == Some("farm:harvest") {
                break;
            }
            client.session_mut().tab(false);
        }
        assert_eq!(client.session_mut().event(), Some("farm:harvest"));
        let request = client.click(HARVEST, HARVEST);
        let (accepted, reason) = client.result(&request);
        assert!(accepted, "{reason}");
        client.ready(HARVEST, STONE, 2);
        assert_eq!(client.inventory_count(0), 127);
        assert_eq!(client.inventory_count(1), 128);
    });
    let mut recovered = open(&fixture);
    assert_eq!(
        recovered
            .world
            .get_block(PLANT[0], PLANT[1], PLANT[2])
            .unwrap(),
        seedling
    );
    assert_eq!(
        recovered
            .world
            .get_block(HARVEST[0], HARVEST[1], HARVEST[2])
            .unwrap(),
        STONE
    );
    let inventory = recovered.inventory_store.load(PROFILE).unwrap();
    assert_eq!(inventory.slots[0], Some(Stack::new(seed, 127)));
    assert_eq!(inventory.slots[1], Some(Stack::new(produce, 128)));
    for (key, minimum) in [
        ("farm:irrigation", 3),
        ("farm:growth", 3),
        ("farm:seasons", 1),
    ] {
        let (revision, bytes) = recovered
            .system_runtime
            .owner_value::<Vec<u8>>(
                &SystemId::new(key).unwrap(),
                OwnerKey::Chunk(ChunkKey { x: 0, y: 5, z: 0 }),
            )
            .unwrap();
        assert!(revision >= minimum, "{key} lost its independent state");
        assert!(!bytes.is_empty());
    }
}

const GROW: [i32; 3] = [3, 81, 0];
#[path = "farming/join.rs"]
mod join;
#[path = "farming/mixed.rs"]
mod mixed;
#[path = "farming/pressure.rs"]
mod pressure;
