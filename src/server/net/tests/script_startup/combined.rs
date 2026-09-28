//! One authored package spanning content, durable gameplay, scheduled work,
//! downloaded UI startup and client-side shader resources over the real listener.
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

const PROFILE: u128 = 0x9234;
const AIM: [i32; 3] = [2, 81, 0];
const GROW: [i32; 3] = [3, 80, 0];

fn open(fixture: &Fixture) -> State {
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/combined-mod/packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&packages)
        .unwrap();
    crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap()
}

fn grow_once(state: &mut State) {
    let system = SystemId::new("verdant:growth").unwrap();
    let registered = state.phase_plan.system(&system).unwrap().clone();
    let mut missing = Vec::new();
    let pending = state
        .system_runtime
        .stage_registered_wave_with_world(
            &registered,
            TickId::new(1),
            0,
            RegisteredWaveInputs {
                effects: &state.effect_kinds,
                durability: &mut state.durability,
                in_flight: &[],
                world: RegisteredWorldInputs {
                    world: Some(&mut state.world),
                    entities: Some(&state.entities),
                    players: &[],
                    seed: state.seed,
                    missing: &mut missing,
                },
            },
        )
        .unwrap()
        .expect("seeded growth owner is due");
    assert!(missing.is_empty());
    complete_barrier(state, pending.barrier()).unwrap();
    assert_eq!(
        state
            .system_runtime
            .owner_value::<Vec<u8>>(&system, OwnerKey::Chunk(ChunkKey { x: 0, y: 5, z: 0 }),),
        Some((1, b"1".to_vec()))
    );
}

#[test]
fn combined_mod_downloads_acts_grows_and_recovers() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut state = Box::new(open(&fixture));
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=4 {
        for z in -1..=2 {
            state.world.edit(x, 79, z, STONE).unwrap();
            for y in 80..=83 {
                state.world.edit(x, y, z, AIR).unwrap();
            }
        }
    }
    state.world.edit(AIM[0], AIM[1], AIM[2], STONE).unwrap();
    state.world.edit(GROW[0], GROW[1], GROW[2], STONE).unwrap();
    let catalog = state.world.catalog_arc();
    let jade = catalog.state_by_key("verdant:jade").unwrap();
    let jade_item = catalog.item_by_key("verdant:jade").unwrap();
    assert_eq!(catalog.item(jade_item).unwrap().placeable, Some(jade));
    let texture = catalog
        .texture(catalog.block(jade).unwrap().textures.side)
        .unwrap();
    assert_eq!(texture.key.as_ref(), "verdant:tile");
    assert_eq!(
        texture.png.as_ref(),
        std::fs::read("fixtures/combined-mod/packages/verdant/assets/textures/jade.png").unwrap()
    );
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(
        catalog.item_by_key("bloxgloom:stick").unwrap(),
        2,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    grow_once(&mut state);
    assert_eq!(
        state.world.get_block(GROW[0], GROW[1], GROW[2]).unwrap(),
        jade
    );

    gameplay::serve(state, |address| {
        let mut client = PackageActionProbe::connect_for(
            &address.to_string(),
            PROFILE,
            fixture.0.join("client-config"),
            "verdant:plant",
        );
        assert!(client.has_downloaded_material_and_startup("verdant"));
        assert_eq!(
            client.selected_material_layer(),
            catalog.block(jade).unwrap().textures.side.get()
        );
        client.ready(AIM, STONE, 0);
        assert_eq!(client.title(), "Jade garden (downloaded mod)");
        let request = client.click(AIM, AIM);
        let (accepted, reason) = client.result(&request);
        assert!(accepted, "{reason}");
        client.ready(AIM, jade, 1);
        assert_eq!(client.count(), 1);
    });
    let mut recovered = open(&fixture);
    assert_eq!(
        recovered.world.get_block(AIM[0], AIM[1], AIM[2]).unwrap(),
        jade
    );
    assert_eq!(
        recovered
            .world
            .get_block(GROW[0], GROW[1], GROW[2])
            .unwrap(),
        jade
    );
    assert_eq!(
        recovered.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    assert_eq!(
        recovered.system_runtime.owner_value::<Vec<u8>>(
            &SystemId::new("verdant:growth").unwrap(),
            OwnerKey::Chunk(ChunkKey { x: 0, y: 5, z: 0 }),
        ),
        Some((1, b"1".to_vec()))
    );
}

#[test]
fn combined_mod_two_profiles_act_independently_and_recover() {
    const OTHER_PROFILE: u128 = 0x9235;
    const OTHER_AIM: [i32; 3] = [2, 81, 2];

    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut state = Box::new(open(&fixture));
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=4 {
        for z in -1..=2 {
            state.world.edit(x, 79, z, STONE).unwrap();
            for y in 80..=83 {
                state.world.edit(x, y, z, AIR).unwrap();
            }
        }
    }
    for [x, y, z] in [AIM, OTHER_AIM, GROW] {
        state.world.edit(x, y, z, STONE).unwrap();
    }
    let catalog = state.world.catalog_arc();
    let jade = catalog.state_by_key("verdant:jade").unwrap();
    let stick = catalog.item_by_key("bloxgloom:stick").unwrap();
    for (profile, count) in [(PROFILE, 2), (OTHER_PROFILE, 3)] {
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(stick, count));
        state.inventory_store.save(profile, &inventory).unwrap();
    }
    grow_once(&mut state);
    assert_eq!(
        state.world.get_block(GROW[0], GROW[1], GROW[2]).unwrap(),
        jade
    );

    gameplay::serve(state, |address| {
        let mut first = PackageActionProbe::connect_for(
            &address.to_string(),
            PROFILE,
            fixture.0.join("first-client-config"),
            "verdant:plant",
        );
        let mut second = PackageActionProbe::connect_for(
            &address.to_string(),
            OTHER_PROFILE,
            fixture.0.join("second-client-config"),
            "verdant:plant",
        );
        let layer = catalog.block(jade).unwrap().textures.side.get();
        for client in [&first, &second] {
            assert!(client.has_downloaded_material_and_startup("verdant"));
            assert_eq!(client.selected_material_layer(), layer);
        }
        first.ready(AIM, STONE, 0);
        second.ready(OTHER_AIM, STONE, 0);
        assert_eq!(first.title(), "Jade garden (downloaded mod)");
        assert_eq!(second.title(), "Jade garden (downloaded mod)");
        assert_eq!(first.count(), 2);
        assert_eq!(second.count(), 3);

        let first_observed = first.observe(AIM);
        let second_observed = second.observe(OTHER_AIM);
        let request = first.click(AIM, AIM);
        let (accepted, reason) = first.result(&request);
        assert!(accepted, "{reason}");
        first.ready(AIM, jade, 1);
        second.ready(AIM, jade, 0);
        assert_eq!(first.count(), 1);
        assert_eq!(second.count(), 3);

        let request = second.click(OTHER_AIM, OTHER_AIM);
        let (accepted, reason) = second.result(&request);
        assert!(accepted, "{reason}");
        second.ready(OTHER_AIM, jade, 1);
        first.ready(OTHER_AIM, jade, 1);
        assert_eq!(first.count(), 1);
        assert_eq!(second.count(), 2);

        for (client, observed, count) in [
            (&mut first, &first_observed, 1),
            (&mut second, &second_observed, 2),
        ] {
            let stale = client.submit_observed(observed);
            let (accepted, reason) = client.result(&stale);
            assert!(!accepted, "stale action applied");
            assert!(reason.contains("target changed"), "{reason}");
            assert_eq!(client.count(), count, "denial changed stick inventory");
        }
    });

    let mut recovered = open(&fixture);
    for [x, y, z] in [AIM, OTHER_AIM, GROW] {
        assert_eq!(recovered.world.get_block(x, y, z).unwrap(), jade);
    }
    for (profile, count) in [(PROFILE, 1), (OTHER_PROFILE, 2)] {
        assert_eq!(
            recovered.inventory_store.load(profile).unwrap().slots[0]
                .as_ref()
                .unwrap()
                .count,
            count
        );
    }
    assert_eq!(
        recovered.system_runtime.owner_value::<Vec<u8>>(
            &SystemId::new("verdant:growth").unwrap(),
            OwnerKey::Chunk(ChunkKey { x: 0, y: 5, z: 0 }),
        ),
        Some((1, b"1".to_vec()))
    );
}
