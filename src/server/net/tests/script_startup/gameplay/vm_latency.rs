//! Opt-in end-to-end evidence; no machine-dependent timing assertions.
use super::*;

const CALLBACK: &str = r#"
local calls = 0
local lookup = {}
for i=1,128 do lookup[i] = i * i end
return function(c,e)
    calls += 1
    assert(calls == 1) -- Authoritative exports still initialize on every attempt.
    local total = 0
    for i=1,1000 do total += lookup[(i % 128) + 1] end
    assert(total > 0)
    local previous = c.block(2,80,0).state
    c.set_block(2,80,0,if previous == 'bloxgloom:glowstone' then 'bloxgloom:sand' else 'bloxgloom:glowstone')
end
"#;

fn report(label: &str, mut times: Vec<Duration>) {
    times.sort_unstable();
    let percentile = |p: usize| times[(times.len() - 1) * p / 100].as_secs_f64() * 1_000.0;
    eprintln!(
        "{label}: samples={} p50={:.3}ms p95={:.3}ms p99={:.3}ms",
        times.len(),
        percentile(50),
        percentile(95),
        percentile(99)
    );
}

#[test]
#[ignore = "opt-in real listener movement/action latency measurement; run alone with --nocapture"]
fn vm_lifetime_mixed_listener_latency() {
    const SAMPLES: u64 = 120;
    let fixture = Fixture::new();
    fixture.action(REGISTER, CALLBACK);
    let mut state = Box::new(fixture.open().unwrap());
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=2 {
        state.world.edit(x, 79, 0, crate::world::STONE).unwrap();
        state.world.edit(x, 80, 0, AIR).unwrap();
        state.world.edit(x, 81, 0, AIR).unwrap();
    }
    let catalog = state.world.catalog_arc();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(
        catalog.item_by_key("bloxgloom:stick").unwrap(),
        1,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();

    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        peer.stream.set_nodelay(true).unwrap();
        let mut movement_only = Vec::new();
        let mut mixed_movement = Vec::new();
        let mut actions = Vec::new();
        for seq in 1..=SAMPLES * 2 {
            let started = Instant::now();
            protocol::write_client(
                &mut peer.stream,
                &ClientMessage::Move {
                    seq,
                    dx: if seq % 2 == 0 { -0.01 } else { 0.01 },
                    dy: 0.0,
                    dz: 0.0,
                },
            )
            .unwrap();
            let action = (seq > SAMPLES).then(|| peer.request(0));
            let action_started = Instant::now();
            let expected_action = action.as_ref().map(|request| {
                protocol::write_client_with_catalog(&mut peer.stream, request, &peer.catalog)
                    .unwrap();
                match request {
                    ClientMessage::EntityInteract { action_id, .. } => *action_id,
                    _ => unreachable!(),
                }
            });
            let mut movement_done = false;
            let mut action_done = expected_action.is_none();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !movement_done || !action_done {
                match peer.read(deadline) {
                    ServerMessage::Position { ack_seq, x, y, z } if ack_seq == seq => {
                        assert!((x - 0.5).abs() <= 0.02 && y == 80.0 && z == 0.5);
                        let elapsed = started.elapsed();
                        if expected_action.is_some() {
                            mixed_movement.push(elapsed);
                        } else {
                            movement_only.push(elapsed);
                        }
                        movement_done = true;
                    }
                    ServerMessage::ActionResult {
                        action_id,
                        accepted,
                        reason,
                    } if Some(action_id) == expected_action => {
                        assert!(accepted, "mixed-load action rejected: {reason}");
                        actions.push(action_started.elapsed());
                        action_done = true;
                    }
                    _ => {}
                }
            }
        }
        eprintln!(
            "mixed_listener_first_action: {:.3}ms (includes network/coordinator/WAL; not isolated VM startup)",
            actions[0].as_secs_f64() * 1_000.0
        );
        actions.remove(0);
        report("listener_movement_only", movement_only);
        report("listener_movement_with_luau_action", mixed_movement);
        report("listener_warm_durable_luau_action", actions);
    });
    // The entire action path reached WAL durability, not just a Lua evaluator.
    let mut recovered = fixture.open().unwrap();
    assert_eq!(recovered.world.get_block(2, 80, 0).unwrap(), SAND);
    assert_eq!(
        recovered.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}
