//! Opt-in real-listener capacity measurement; every count gets an isolated save.
use super::*;
use bloxgloom_host_api::actions::Request;

const REGISTER: &str = r#"return function(h)
    h.register_moving_entity{key='demo:projectile',module='demo:behavior',schema=1,revision=1,
        max_state_bytes=2,max_public_bytes=2,interval=1,lifetime_ticks=10000,
        body={half_extents={0.05,0.05,0.05},max_speed=16,max_acceleration=32,
            gravity_scale=0,response='stop',collisions={terrain=true}},
        model={{min={-0.05,-0.05,-0.05},max={0.05,0.05,0.05},color={0.3,0.8,0.2}}}}
    h.register_action('demo:shift',1,'Launch load','item','bloxgloom:stick','demo:action')
end"#;
const ACTION: &str = r#"return function(c,e)
    local n,lo,hi,totalLo,totalHi,cluster=e.arguments:byte(1,6)
    local base=lo+256*hi
    local total=totalLo+256*totalHi
    assert(c.take('player',e.slot,n) ~= nil)
    for offset=0,n-1 do
        local index=base+offset
        local tile=math.floor(index/64)
        local x=8.2+(tile%2)*8+(index%8)*0.9
        local z=8.2+math.floor(tile/2)*8+(math.floor(index/8)%8)*0.9
        local speed=0.25
        if index == total-1 then x=15.8+(tile%2)*8; speed=8 end
        if cluster == 1 then x=8.2;z=8.2;speed=0 end
        c.spawn_moving_entity('demo:projectile',{position={x,83.5,z},velocity={speed,0,0},state=string.char(index%256,math.floor(index/256))})
    end
end"#;

fn package(fixture: &Fixture) {
    fixture.package("demo", "requires bloxgloom:content/v1\nrequires bloxgloom:actions/v1\nrequires bloxgloom:moving_entities/v1\nmodule action action.luau\nmodule behavior behavior.luau", REGISTER);
    let dir = fixture.0.join("packages/demo");
    std::fs::write(dir.join("action.luau"), ACTION).unwrap();
    std::fs::write(dir.join("behavior.luau"),"return function(c,e) assert(e.kind == 'MovingTick'); local m=c.motion(e.entity); assert(m ~= nil) end").unwrap();
}
fn request(
    peer: &mut gameplay::Peer,
    base: usize,
    count: usize,
    total: usize,
    cluster: bool,
) -> ClientMessage {
    let mut message = peer.request(0);
    if let ClientMessage::EntityInteract { payload, .. } = &mut message {
        let mut request = Request::decode(payload).unwrap();
        request.slot = (base / 128).min(3) as u8;
        request.arguments = vec![
            count as u8,
            base as u8,
            (base >> 8) as u8,
            total as u8,
            (total >> 8) as u8,
            u8::from(cluster),
        ];
        *payload = request.encode().unwrap();
    }
    message
}
fn count(state: &State) -> Vec<(EntityId, Record)> {
    let kind = state
        .world
        .catalog()
        .entity_type_id_by_key("demo:projectile")
        .unwrap();
    state
        .entities
        .query_mobile_aabb([-2.0, 79.0, -2.0], [48.0, 90.0, 48.0])
        .unwrap()
        .into_iter()
        .filter_map(|id| {
            let snapshot = state.entities.snapshot(id).unwrap();
            (snapshot.entity_type == kind).then(|| {
                (
                    id,
                    Record::decode(snapshot.private_payload.downcast_ref::<Vec<u8>>().unwrap())
                        .unwrap(),
                )
            })
        })
        .collect()
}
fn rank(samples: &[Duration], percentile: usize) -> Duration {
    let mut sorted = samples.to_vec();
    sorted.sort();
    sorted[(sorted.len() * percentile).div_ceil(100).saturating_sub(1)]
}

#[test]
#[ignore = "explicit real-listener moving-body capacity and latency measurement"]
fn moving_real_listener_capacity_measurements() {
    let counts = std::env::var("BLOXGLOOM_MOTION_LOAD_COUNT")
        .ok()
        .map(|count| vec![count.parse::<usize>().unwrap()])
        .unwrap_or_else(|| vec![1, 64, 256]);
    for bodies in counts {
        assert!([1, 64, 256].contains(&bodies));
        let fixture = Fixture::new();
        package(&fixture);
        let started = Instant::now();
        let mut state = Box::new(fixture.open().unwrap());
        let discovery = started.elapsed();
        // Launch points occupy four chunks around their common corner, all
        // within the authoritative eight-cell interaction reach.
        state.spawn_anchor = [16.0, 80.0, 16.0];
        for x in -1..=34 {
            for z in -1..=34 {
                state.world.edit(x, 79, z, crate::world::STONE).unwrap();
                for y in 80..=85 {
                    state.world.edit(x, y, z, crate::world::AIR).unwrap();
                }
            }
        }
        // The live spawn selector uses origin X/Z; a saved profile pose selects
        // the intended four-chunk admission point authoritatively.
        state
            .position_store
            .save(PROFILE, [16.0, 80.0, 16.0])
            .unwrap();
        let catalog = state.world.catalog_arc();
        let kind = catalog.entity_type_id_by_key("demo:projectile").unwrap();
        let stick = catalog.item_by_key("bloxgloom:stick").unwrap();
        let mut inventory = Inventory::default();
        for slot in 0..4 {
            inventory.slots[slot] = Some(Stack::new(stick, 128));
        }
        state.inventory_store.save(PROFILE, &inventory).unwrap();
        let mut latencies = Vec::new();
        let mut entity_bytes = 0usize;
        let mut updates = 0usize;
        let (sample_tx, sample_rx) = mpsc::sync_channel(1024);
        state.tick_observer = Some(sample_tx);
        let (motion_tx, motion_rx) = mpsc::sync_channel(1024);
        state.motion_observer = Some(motion_tx);
        let mut observation = None;
        gameplay::serve(state, |address| {
            eprintln!("motion-load bodies={bodies} connecting");
            let mut peer = gameplay::Peer::connect(address, Arc::clone(&catalog));
            eprintln!("motion-load bodies={bodies} connected");
            for base in (0..bodies).step_by(32) {
                let request = request(&mut peer, base, (bodies - base).min(32), bodies, false);
                let before = Instant::now();
                let (accepted, reason) = peer.send(&request);
                latencies.push(before.elapsed());
                assert!(accepted, "{bodies} bodies launch at {base}: {reason}");
                let accepted_count = base + (bodies - base).min(32);
                // Terminal action receipts can precede the finite inventory
                // update. The next request must use its new captured revision.
                let deadline = Instant::now() + Duration::from_secs(10);
                while peer
                    .inventory
                    .slots
                    .iter()
                    .flatten()
                    .map(|s| u32::from(s.count))
                    .sum::<u32>()
                    != 512 - accepted_count as u32
                {
                    peer.read(deadline);
                }
            }
            if bodies == 64 {
                // The last body can already have left the original 64-body
                // chunk: two staged bodies still exceed its independent cap.
                let extra = request(&mut peer, bodies, 2, bodies + 2, true);
                let (accepted, reason) = peer.send(&extra);
                assert!(!accepted, "per-chunk overload accepted: {reason}");
            }
            if bodies == 256 {
                for cluster in [false, true] {
                    let extra = request(&mut peer, bodies, 1, bodies + 1, cluster);
                    let (accepted, reason) = peer.send(&extra);
                    assert!(!accepted, "overload accepted: {reason}");
                }
            }
            // Separate admission/loading from the steady population window.
            while sample_rx.try_recv().is_ok() {}
            while motion_rx.try_recv().is_ok() {}
            let start = Instant::now();
            let deadline = start + Duration::from_secs(15);
            let mut seam = false;
            let mut moving = std::collections::BTreeSet::new();
            while start.elapsed() < Duration::from_secs(2) || !seam || moving.len() < bodies {
                if Instant::now() >= deadline {
                    break;
                }
                let message = peer.read(deadline);
                let mut observe = |entity: &protocol::PublicEntity| {
                    if entity.entity_type != kind {
                        return;
                    }
                    let pose =
                        bloxgloom_host_api::motion::Projection::decode(&entity.payload).unwrap();
                    if pose.motion.revision > 0 {
                        moving.insert(entity.id);
                    }
                    if pose.data[0] == (bodies - 1) as u8
                        && pose.motion.position[0] >= if bodies > 64 { 32.0 } else { 16.0 }
                    {
                        seam = true;
                    }
                    updates += 1;
                    if updates <= 4 {
                        eprintln!(
                            "motion-load pose {:?} rev={} stopped={}",
                            pose.motion.position, pose.motion.revision, pose.stopped
                        );
                    }
                };
                match &message {
                    ServerMessage::WorldCommitPart(part) => {
                        entity_bytes += protocol::server_wire_len(&message);
                        for change in &part.entities {
                            if let protocol::PublicEntityChange::Upsert(entity) = change {
                                observe(entity);
                            }
                        }
                    }
                    ServerMessage::EntitySnapshotPage(page) => {
                        entity_bytes += protocol::server_wire_len(&message);
                        for entity in &page.entities {
                            observe(entity);
                        }
                    }
                    _ => {}
                }
            }
            observation = Some((seam, moving.len()));
        });
        let samples: Vec<_> = sample_rx.try_iter().collect();
        assert!(
            !samples.is_empty(),
            "real listener produced no tick observations"
        );
        let ticks: Vec<_> = samples.iter().map(|sample| sample.tick_total).collect();
        let durable: Vec<_> = samples.iter().map(|sample| sample.phases[1]).collect();
        let commits: Vec<_> = samples.iter().map(|sample| sample.phases[3]).collect();
        let motion: Vec<_> = motion_rx.try_iter().collect();
        assert!(!motion.is_empty());
        let capture: Vec<_> = motion.iter().map(|s| s.capture).collect();
        let solve: Vec<_> = motion.iter().map(|s| s.solve).collect();
        let attempts: u64 = motion.iter().map(|s| s.attempts).sum();
        let simulated_steps: u64 = motion.iter().map(|s| s.steps).sum();
        if bodies == 1 {
            assert!(
                simulated_steps >= samples.len() as u64 / 3,
                "frequent callbacks slowed nominal physics: {simulated_steps} steps over {} ticks",
                samples.len()
            );
        }
        let deferred: u64 = motion.iter().map(|s| s.deferred).sum();
        let failed: u64 = motion.iter().map(|s| s.failed).sum();
        let pending = samples
            .iter()
            .map(|sample| sample.pending_durable_actions)
            .max()
            .unwrap();
        let resident = samples
            .iter()
            .map(|sample| sample.resident_chunks)
            .max()
            .unwrap();
        let state = fixture.open().unwrap();
        let saved = count(&state);
        assert_eq!(saved.len(), bodies, "committed bodies/restart");
        eprintln!(
            "motion-load observed={observation:?} saved={:?}",
            saved
                .iter()
                .map(|(_, r)| (
                    &r.motion,
                    r.next_behavior_tick,
                    r.simulation_tick,
                    &r.pending
                ))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "motion-load debug attempts={attempts} deferred={deferred} failed={failed} next_ticks={:?}",
            saved
                .iter()
                .map(|(id, _)| state.entities.snapshot(*id).unwrap().next_tick)
                .collect::<Vec<_>>()
        );
        assert_eq!(observation, Some((true, bodies)));
        let record_bytes: usize = saved.iter().map(|(_, r)| r.encode().unwrap().len()).sum();
        assert!(
            saved
                .iter()
                .all(|(_, record)| record.pending.is_none() && record.motion.revision > 0)
        );
        let inventory = state.inventory_store.load(PROFILE).unwrap();
        let remaining: u32 = inventory
            .slots
            .iter()
            .flatten()
            .map(|s| u32::from(s.count))
            .sum();
        assert_eq!(
            remaining,
            512 - bodies as u32,
            "launch conservation and overload rollback"
        );
        assert!(crate::server::drops::nearby(&state.entities, [16.0, 83.5, 16.0]).is_empty());
        eprintln!(
            "motion-load bodies={bodies} discovery_ms={:.3} launch_count={} launch_p50_ms={:.3} launch_p95_ms={:.3} launch_p99_ms={:.3} replica_bytes={entity_bytes} entity_updates={updates} remaining_seeds={remaining}",
            discovery.as_secs_f64() * 1000.0,
            latencies.len(),
            rank(&latencies, 50).as_secs_f64() * 1000.0,
            rank(&latencies, 95).as_secs_f64() * 1000.0,
            rank(&latencies, 99).as_secs_f64() * 1000.0
        );
        eprintln!(
            "motion-load-physics bodies={bodies} attempts={attempts} fixed_steps={simulated_steps} deferred={deferred} failed={failed} capture_tick_p50_ms={:.3} capture_tick_p95_ms={:.3} solve_tick_p50_ms={:.3} solve_tick_p95_ms={:.3} encoded_record_bytes={record_bytes}",
            rank(&capture, 50).as_secs_f64() * 1000.0,
            rank(&capture, 95).as_secs_f64() * 1000.0,
            rank(&solve, 50).as_secs_f64() * 1000.0,
            rank(&solve, 95).as_secs_f64() * 1000.0
        );
        eprintln!(
            "motion-load-server bodies={bodies} tick_samples={} tick_p50_ms={:.3} tick_p95_ms={:.3} tick_p99_ms={:.3} durable_phase_p50_ms={:.3} durable_phase_p95_ms={:.3} commit_phase_p50_ms={:.3} commit_phase_p95_ms={:.3} pending_actions_max={pending} resident_chunks_max={resident}",
            samples.len(),
            rank(&ticks, 50).as_secs_f64() * 1000.0,
            rank(&ticks, 95).as_secs_f64() * 1000.0,
            rank(&ticks, 99).as_secs_f64() * 1000.0,
            rank(&durable, 50).as_secs_f64() * 1000.0,
            rank(&durable, 95).as_secs_f64() * 1000.0,
            rank(&commits, 50).as_secs_f64() * 1000.0,
            rank(&commits, 95).as_secs_f64() * 1000.0
        );
    }
}
