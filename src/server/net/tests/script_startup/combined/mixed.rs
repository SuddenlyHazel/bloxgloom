//! Sustained authored gameplay and finite inventory work during fresh downloads.
use super::*;
use crate::server::net::tests::package_load::Downloads;
use std::time::{Duration, Instant};

const ROUNDS: u16 = 32;
const TARGETS: [[i32; 3]; 2] = [[4, 81, 0], [4, 81, 2]];

#[test]
fn combined_mod_mixed_load_preserves_response_progress_and_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut state = Box::new(open(&fixture));
    state.admission_limit = 8;
    state.admin_profile = Some(PROFILE);
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=6 {
        for z in -1..=3 {
            state.world.edit(x, 79, z, STONE).unwrap();
            for y in 80..=83 {
                state.world.edit(x, y, z, AIR).unwrap();
            }
        }
    }
    for [x, y, z] in TARGETS.into_iter().chain([GROW]) {
        state.world.edit(x, y, z, STONE).unwrap();
    }
    let catalog = state.world.catalog_arc();
    let jade = catalog.state_by_key("verdant:jade").unwrap();
    let stick = catalog.item_by_key("bloxgloom:stick").unwrap();
    for profile in [PROFILE, PROFILE + 1] {
        let mut inventory = Inventory::default();
        inventory.slots.fill(None);
        inventory.slots[0] = Some(Stack::new(stick, 64));
        inventory.slots[1] = Some(Stack::new(crate::items::ItemId(STONE.0), 128));
        state.inventory_store.save(profile, &inventory).unwrap();
    }

    gameplay::serve(state, |address| {
        let mut downloads = Downloads::start(address);
        let mut clients: Vec<_> = TARGETS
            .iter()
            .enumerate()
            .map(|(index, target)| {
                let mut client = PackageActionProbe::connect_for(
                    &address.to_string(),
                    PROFILE + index as u128,
                    fixture.0.join(format!("mixed-client-{index}")),
                    "verdant:plant",
                );
                client.ready(*target, STONE, 0);
                assert!(client.has_downloaded_material_and_startup("verdant"));
                client
            })
            .collect();
        let command = clients[0].mixed_command("verdant:noon");
        let (accepted, reason) = clients[0].result(&command);
        assert!(accepted, "authored clock command failed: {reason}");
        downloads.resume();
        let started = Instant::now();
        let workers: Vec<_> = clients
            .drain(..)
            .zip(TARGETS)
            .map(|(mut client, target)| {
                thread::spawn(move || {
                    let mut actions = Vec::new();
                    let mut moves = Vec::new();
                    let mut denied = 0;
                    for round in 0..ROUNDS {
                        moves.push(client.mixed_move(
                            u64::from(round) + 1,
                            if round % 2 == 0 { 0.05 } else { -0.05 },
                        ));
                        let start = Instant::now();
                        for attempt in 0..20 {
                            client.mixed_drain();
                            let request = client.click(target, target);
                            let (accepted, reason) = client.result(&request);
                            if accepted {
                                break;
                            }
                            // Both targets share a chunk: a concurrent committed
                            // edit may invalidate the captured observation. Retry
                            // a fresh observation, never bypass the terrain fence.
                            assert!(
                                reason.starts_with("block action target changed"),
                                "authored action failed: {reason}"
                            );
                            assert_eq!(client.count(), 64 - round);
                            assert!(attempt < 19, "authored action made no progress");
                            denied += 1;
                            thread::sleep(Duration::from_millis(20));
                        }
                        actions.push(start.elapsed());
                        client.mixed_wait_block(target, jade);
                        client.mixed_wait_slot_count(0, 63 - round);
                        assert_eq!(client.count(), 63 - round);
                        for (from, to) in [(0, 2), (2, 0)] {
                            let start = Instant::now();
                            let request = client.mixed_transfer(from, to);
                            let (accepted, reason) = client.result(&request);
                            assert!(accepted, "finite transfer failed: {reason}");
                            actions.push(start.elapsed());
                            client.mixed_wait_slot_count(
                                0,
                                if from == 0 { 62 - round } else { 63 - round },
                            );
                        }
                        for (block, slot) in [(AIR, 0), (STONE, 1)] {
                            let start = Instant::now();
                            let request = client.edit(target, block, slot);
                            let (accepted, reason) = client.result(&request);
                            assert!(accepted, "edit failed: {reason}");
                            actions.push(start.elapsed());
                            client.mixed_wait_block(target, block);
                            if block == STONE {
                                client.mixed_wait_slot_count(1, 127 - round);
                            }
                        }
                        // Representative paced play while downloads and deadlines run.
                        thread::sleep(Duration::from_millis(50));
                    }
                    (actions, moves, denied)
                })
            })
            .collect();
        let mut actions = Vec::new();
        let mut moves = Vec::new();
        let mut denied = 0;
        for worker in workers {
            let (mut worker_actions, mut worker_moves, worker_denied) = worker.join().unwrap();
            actions.append(&mut worker_actions);
            moves.append(&mut worker_moves);
            denied += worker_denied;
        }
        downloads.finish();
        assert_eq!(actions.len(), usize::from(ROUNDS) * 10);
        assert_eq!(moves.len(), usize::from(ROUNDS) * 2);
        report("authored actions/edits/transfers", &mut actions);
        report("movement acknowledgements", &mut moves);
        eprintln!("combined mixed workload duration: {:?}", started.elapsed());
        eprintln!("combined safe stale-observation denials/retries: {denied}");
    });

    let mut recovered = open(&fixture);
    assert_eq!(
        recovered
            .world
            .get_block(GROW[0], GROW[1], GROW[2])
            .unwrap(),
        jade
    );
    let (revision, value) = recovered
        .system_runtime
        .owner_value::<Vec<u8>>(
            &SystemId::new("verdant:growth").unwrap(),
            OwnerKey::Chunk(ChunkKey { x: 0, y: 5, z: 0 }),
        )
        .unwrap();
    assert!(
        revision >= 2,
        "scheduled owner stopped progressing during load"
    );
    assert_eq!(value, b"1");
    for (index, [x, y, z]) in TARGETS.into_iter().enumerate() {
        assert_eq!(recovered.world.get_block(x, y, z).unwrap(), STONE);
        let inventory = recovered
            .inventory_store
            .load(PROFILE + index as u128)
            .unwrap();
        assert_eq!(inventory.slots[0], Some(Stack::new(stick, 64 - ROUNDS)));
        assert_eq!(
            inventory.slots[1],
            Some(Stack::new(crate::items::ItemId(STONE.0), 128 - ROUNDS))
        );
        assert_eq!(inventory.slots[2], None);
    }
}

fn report(label: &str, samples: &mut [Duration]) {
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p95 = samples[(samples.len() - 1) * 95 / 100];
    let maximum = *samples.last().unwrap();
    eprintln!(
        "combined {label}: {} samples, median={median:?}, p95={p95:?}, max={maximum:?}",
        samples.len()
    );
    assert!(
        p95 < Duration::from_millis(500),
        "{label} p95 regressed: {p95:?}"
    );
    assert!(
        maximum < Duration::from_secs(2),
        "{label} stalled: {maximum:?}"
    );
}
