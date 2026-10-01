use super::*;
use crate::world::{AIR, ChunkKey, GLOWSTONE, World};

const GENERATION: &str = "requires bloxgloom:generation/v1\nmodule terrain terrain.luau";
const REGISTER: &str =
    "return function(h) h.register_generator('demo:terrain', 1, 'demo:terrain') end";
const MARKER: &str = "local calls = 0; return function(c) calls += 1; assert(calls == 1); c.set_block(0,0,0,'bloxgloom:glowstone') end";

impl Fixture {
    fn generator(&self, register: &str, source: &str) {
        self.package("demo", GENERATION, register);
        std::fs::write(self.0.join("packages/demo/terrain.luau"), source).unwrap();
    }

    fn generation_world(&self) -> io::Result<World> {
        let catalog = Arc::new(Catalog::builtins());
        let startup = self.startup(Arc::clone(&catalog))?;
        World::with_generation(
            u64::MAX,
            self.0.join("save"),
            1,
            catalog,
            startup.generation(),
        )
    }
}

#[test]
fn luau_generation_streams_from_loader_after_restart() {
    let fixture = Fixture::new();
    fixture.generator(REGISTER, MARKER);
    for _ in 0..2 {
        let mut state = Box::new(fixture.open().unwrap());
        let fingerprint = state.world.catalog_arc().fingerprint();
        state
            .world
            .reset_cache_for_test(crate::server::SERVER_CHUNK_CACHE);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stop_rx));
        let result = std::panic::catch_unwind(|| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "luau-generation".into(),
                    profile: 0x67656e,
                    content_fingerprint: fingerprint,
                },
            )
            .unwrap();
            let (received, _) = receive_content_manifest(&mut peer);
            assert_eq!(received, fingerprint);
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                assert!(Instant::now() < deadline, "Luau chunk never streamed");
                if let ServerMessage::WorldSnapshotStart(snapshot) =
                    protocol::read_server(&mut peer).unwrap()
                {
                    assert_eq!(snapshot.chunk.block([0, 0, 0]), Some(GLOWSTONE));
                    break;
                }
            }
        });
        stop_tx.send(()).unwrap();
        server.join().unwrap().unwrap();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}

#[test]
fn luau_generation_baseline_edits_and_identity_survive_restart() {
    let fixture = Fixture::new();
    fixture.generator(REGISTER, MARKER);
    let key = ChunkKey { x: -1, y: 8, z: -1 };
    let mut world = fixture.generation_world().unwrap();
    assert_eq!(
        world.get_chunk(key).unwrap().block([0, 0, 0]),
        Some(GLOWSTONE)
    );
    world.edit(-16, 128, -16, AIR).unwrap();
    drop(world);
    let mut world = fixture.generation_world().unwrap();
    assert_eq!(world.get_chunk(key).unwrap().block([0, 0, 0]), Some(AIR));
    world.edit(-16, 128, -16, GLOWSTONE).unwrap();
    drop(world);
    let mut world = fixture.generation_world().unwrap();
    assert_eq!(
        world.get_chunk(key).unwrap().block([0, 0, 0]),
        Some(GLOWSTONE)
    );
    drop(world);
    // Revision metadata remains, but resetting removes the cell override.
    let catalog = Arc::new(Catalog::builtins());
    let entries = fixture.startup(Arc::clone(&catalog)).unwrap().generation();
    let generator = crate::world::Generator::new(entries).unwrap();
    let storage = crate::storage::Storage::with_generation(
        fixture.0.join("save"),
        u64::MAX,
        catalog,
        &generator,
    )
    .unwrap();
    let snapshot = storage.read_snapshot(key).unwrap();
    assert!(
        storage
            .decode_snapshot(snapshot.as_deref())
            .unwrap()
            .blocks
            .is_empty()
    );
    drop(storage);
    let metadata = std::fs::read(fixture.0.join("save/world.meta")).unwrap();
    fixture.generator(&REGISTER.replace(", 1,", ", 2,"), MARKER);
    let error = fixture.generation_world().err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
        std::fs::read(fixture.0.join("save/world.meta")).unwrap(),
        metadata
    );
}

#[test]
fn luau_generation_sampling_is_exact_frozen_and_fresh_across_parallel_loads() {
    let fixture = Fixture::new();
    let context = bloxgloom_host_api::generation::Context::new(u64::MAX, [-1, 8, -1]);
    let random = context.random_at([-16, 128, -16], u64::MAX);
    fixture.generator(
        REGISTER,
        &format!(
            r#"
        local calls = 0
        return function(c)
            calls += 1; assert(calls == 1)
            assert(c.seed_lo == 4294967295 and c.seed_hi == 4294967295)
            assert(c.chunk_x == -1 and c.chunk_y == 8 and c.chunk_z == -1)
            local x,y,z = c.world_position(0,0,0)
            assert(x == -16 and y == 128 and z == -16)
            local lo,hi = c.random_at(x,y,z,4294967295,4294967295)
            assert(lo == {} and hi == {})
            local sample = c.random_at(x,y,z,7)
            assert(sample == c.random_at(x,y,z,7) and sample >= 0 and sample < 1)
            assert(c.random_at(x,y,z) == c.random_at(x,y,z,0))
            local height = c.builtin_terrain_height(x,z)
            assert(c.builtin_base_block(x,height+1,z) == 'bloxgloom:air')
            c.set_block(0,0,0,'bloxgloom:glowstone')
        end
    "#,
            random as u32,
            (random >> 32) as u32
        ),
    );
    let world = fixture.generation_world().unwrap();
    // Neither file changes nor previous module globals can affect retained input.
    std::fs::write(
        fixture.0.join("packages/demo/terrain.luau"),
        "error('changed')",
    )
    .unwrap();
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let loader = world.loader_view();
            thread::spawn(move || {
                loader
                    .load_chunk_uncached(ChunkKey { x: -1, y: 8, z: -1 })
                    .unwrap()
                    .chunk
            })
        })
        .collect();
    for worker in workers {
        assert_eq!(worker.join().unwrap().block([0, 0, 0]), Some(GLOWSTONE));
    }
}

#[test]
fn luau_generation_rejects_bad_registration_and_caught_output_errors() {
    let fixture = Fixture::new();
    for register in [
        REGISTER.replace("demo:terrain', 1", "other:terrain', 1"),
        REGISTER.replace(", 1,", ", 0,"),
        REGISTER.replace("'demo:terrain')", "'demo:missing')"),
        "return function(h) pcall(function() h.register_generator('demo:terrain', 0, 'demo:terrain') end) end".into(),
        "return function(h) h.register_generator('demo:terrain', 1, 'demo:terrain'); h.register_generator('demo:other', 1, 'demo:terrain') end".into(),
    ] {
        fixture.generator(&register, MARKER);
        assert!(fixture.startup(Arc::new(Catalog::builtins())).is_err());
        assert!(!fixture.0.join("save").exists());
    }
    fixture.package("demo", "module terrain terrain.luau", REGISTER);
    assert!(fixture.startup(Arc::new(Catalog::builtins())).is_err());
    for (source, message) in [
        (
            "return function(c) pcall(function() c.set_block(0.5,0,0,'bloxgloom:stone') end) end",
            "integer",
        ),
        (
            "return function(c) for i=1,4096 do c.set_block(0,0,0,'bloxgloom:stone') end; pcall(function() c.set_block(0,0,0,'bloxgloom:stone') end) end",
            "WriteLimit",
        ),
        (
            "return function(c) c.set_block(0,0,0,string.rep('x',256)) end",
            "255 bytes",
        ),
        (
            "return function(c) c.set_block(0,0,0,'demo:unknown') end",
            "InvalidState",
        ),
        (
            "return function(c) c.set_block(0,0,0,'bloxgloom:glowstone'); error('broken module') end",
            "broken module",
        ),
        ("return function(c) while true do end end", "Limit"),
        (
            "return function(c) local a = string.rep('x', 16000000) end",
            "memory",
        ),
    ] {
        fixture.generator(REGISTER, source);
        let mut world = fixture.generation_world().unwrap();
        let key = ChunkKey { x: -1, y: 8, z: -1 };
        let error = world.get_chunk(key).err().unwrap().to_string();
        assert!(error.contains(message), "{source}: {error}");
        if message != "InvalidState" {
            assert!(error.contains("demo@1.0.0:terrain"), "{error}");
        }
        assert!(world.cached_edit_basis(key).is_none());
        drop(world);
    }
}

#[test]
fn scripted_generator_obeys_remaining_allowance_and_retries_without_partial_output() {
    let fixture = Fixture::new();
    fixture.generator(REGISTER, MARKER);
    let startup = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    let entries = startup.generation();
    let contributor = &entries[0].contributor;
    let context = bloxgloom_host_api::generation::Context::new(73, [0, 8, 0]);
    let mut output = bloxgloom_host_api::generation::Output::default();
    assert!(
        contributor
            .generate_budgeted(context, &mut output, Duration::ZERO)
            .is_err()
    );
    assert_eq!(output.writes().count(), 0);
    assert!(
        contributor
            .generate_budgeted(context, &mut output, Duration::from_nanos(1))
            .is_err()
    );
    assert_eq!(output.writes().count(), 0);
    assert!(
        contributor
            .generate_budgeted(context, &mut output, Duration::from_millis(50))
            .is_ok()
    );
    assert_eq!(output.writes().count(), 1);
}
