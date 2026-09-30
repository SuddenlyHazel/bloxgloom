//! Script decisions through semantic dispatch, nonblocking TCP and durable saves.
use super::*;
use crate::inventory::Stack;
use crate::world::{AIR, GLOWSTONE, SAND};
use bloxgloom_host_api::actions::Request;

#[path = "gameplay/authorization.rs"]
mod authorization;
#[path = "gameplay/blocks.rs"]
mod blocks;
#[path = "gameplay/commands.rs"]
mod commands;
#[path = "gameplay/composed.rs"]
mod composed;
#[path = "gameplay/decisions.rs"]
mod decisions;
#[path = "gameplay/entities.rs"]
mod entities;
#[path = "gameplay/inventory.rs"]
mod inventory;
#[path = "gameplay/observers.rs"]
mod observers;
#[path = "gameplay/player_inventory.rs"]
mod player_inventory;
#[path = "gameplay/player_operations.rs"]
mod player_operations;
#[path = "gameplay/player_teleport.rs"]
mod player_teleport;
#[path = "gameplay/players.rs"]
mod players;
#[path = "gameplay/profile_state.rs"]
mod profile_state;
#[path = "gameplay/runtime_tools.rs"]
mod runtime_tools;

const PROFILE: u128 = 0x5c71;
const REGISTER: &str = "return function(h) h.register_action('demo:shift', 1, 'Shift', 'item', 'bloxgloom:stick', 'demo:action') end";
const SOURCE: &str = r#"
local calls = 0
return function(c, e)
    calls += 1
    assert(calls == 1)
    assert(e.kind == 'ActionRequested' and e.action == 'demo:shift')
    assert(e.slot == 0 and e.cell == nil and e.entity_lo == nil)
    assert(e.position[1] == 0.5 and e.position[2] == 80 and e.position[3] == 0.5)
    -- The request's item-action target is [0,0,0]; this is session state instead.
    assert(c.player_position[1] == 0.5 and c.player_position[2] == 80 and c.player_position[3] == 0.5)
    assert(not pcall(function() c.player_position[1] = 2 end))
    assert(type(c.tick_lo) == 'number' and type(c.tick_hi) == 'number')
    assert(type(print) == 'function' and require == nil and os.clock == nil and os.time == nil and os.date == nil)
    assert(c.transfer(e.slot, 1, 1))
    local previous = c.block(2,80,0).state
    local next = if previous == 'bloxgloom:air' then 'bloxgloom:glowstone' else 'bloxgloom:sand'
    c.set_block(2,80,0,next)
    assert(c.block(2,80,0).state == next)
    local mode = string.byte(e.arguments, 1)
    if mode == 1 then error('rollback script error') end
    if mode == 2 then pcall(function() c.set_block(2.5,80,0,next) end) end
    if mode == 3 then pcall(function() c.set_block(2,80,0,'demo:missing') end) end
    if mode == 4 then pcall(function() while true do end end) end
    if mode == 5 then local _ = string.rep('x', 16000000) end
    if mode == 6 then pcall(function() for i=1,4100 do c.block(2,80,0) end end) end
    if mode == 7 then pcall(function() c.transfer(0,1,129) end) end
    if mode == 8 then pcall(function() c.block(1600,80,0) end) end
end
"#;

impl Fixture {
    fn action(&self, register: &str, source: &str) {
        self.package(
            "demo",
            "requires bloxgloom:actions/v1\nmodule action action.luau",
            register,
        );
        std::fs::write(self.0.join("packages/demo/action.luau"), source).unwrap();
    }
}

struct Peer {
    stream: TcpStream,
    catalog: Arc<Catalog>,
    epoch: u64,
    sequence: u64,
    inventory: Inventory,
}
impl Peer {
    fn connect(address: std::net::SocketAddr, catalog: Arc<Catalog>) -> Self {
        Self::connect_profile(address, catalog, PROFILE)
    }
    fn connect_profile(
        address: std::net::SocketAddr,
        catalog: Arc<Catalog>,
        profile: u128,
    ) -> Self {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut stream,
            &ClientMessage::Hello {
                name: "luau-action".into(),
                profile,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let (fingerprint, _) = receive_content_manifest(&mut stream);
        assert_eq!(fingerprint, catalog.fingerprint());
        protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint }).unwrap();
        let mut peer = Self {
            stream,
            catalog,
            epoch: 0,
            sequence: 0,
            inventory: Inventory::default(),
        };
        let mut session = false;
        let mut inventory = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !session || !inventory {
            match peer.read(deadline) {
                ServerMessage::ActionSession {
                    epoch, next_seq, ..
                } => {
                    peer.epoch = epoch;
                    peer.sequence = next_seq;
                    session = true;
                }
                ServerMessage::Inventory { .. } => inventory = true,
                _ => {}
            }
        }
        peer
    }

    fn read(&mut self, deadline: Instant) -> ServerMessage {
        assert!(
            Instant::now() < deadline,
            "action response deadline exceeded"
        );
        let message = protocol::read_server_with_catalog(&mut self.stream, &self.catalog).unwrap();
        if let ServerMessage::Inventory { revision, slots } = &message {
            self.inventory.revision = *revision;
            self.inventory.slots = slots.clone();
        }
        message
    }

    fn request(&mut self, mode: u8) -> ClientMessage {
        let action_id = (u128::from(self.epoch) << 64) | u128::from(self.sequence);
        self.sequence += 1;
        ClientMessage::EntityInteract {
            action_id,
            target: [0, 0, 0], // Item actions must not trust this as a world target.
            payload: Request {
                key: "demo:shift".into(),
                version: 1,
                slot: 0,
                inventory_revision: self.inventory.revision,
                entity: 0,
                entity_revision: 0,
                arguments: vec![mode],
            }
            .encode()
            .unwrap(),
        }
    }

    fn send(&mut self, request: &ClientMessage) -> (bool, String) {
        let expected = match request {
            ClientMessage::EntityInteract { action_id, .. }
            | ClientMessage::Edit { action_id, .. }
            | ClientMessage::AdminGive { action_id, .. }
            | ClientMessage::AdminSpawnEntity { action_id, .. } => action_id,
            _ => unreachable!(),
        };
        protocol::write_client_with_catalog(&mut self.stream, request, &self.catalog).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::ActionResult {
                action_id,
                accepted,
                reason,
            } = self.read(deadline)
                && action_id == *expected
            {
                return (accepted, reason);
            }
        }
    }

    fn inventory_at(&mut self, count: u16) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.inventory.slots[0].as_ref().map(|stack| stack.count) != Some(count) {
            self.read(deadline);
        }
    }
}

pub(super) fn serve(state: Box<State>, run: impl FnOnce(std::net::SocketAddr)) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (stop_tx, stop_rx) = mpsc::sync_channel(1);
    let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stop_rx));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(address)));
    stop_tx.send(()).unwrap();
    server.join().unwrap().unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn luau_action_loopback_rollbacks_exact_transfer_receipts_and_restart() {
    let fixture = Fixture::new();
    fixture.action(REGISTER, SOURCE);
    let mut original = None;
    for round in 0..3 {
        let mut state = Box::new(fixture.open().unwrap());
        state.spawn_anchor = [0.5, 80.0, 0.5];
        let catalog = state.world.catalog_arc();
        let item = catalog.item_by_key("bloxgloom:stick").unwrap();
        if round == 0 {
            // Give live spawn selection real support/headroom, rather than
            // assuming that setting spawn_anchor authorizes a player position.
            state.world.edit(0, 79, 0, crate::world::STONE).unwrap();
            state.world.edit(0, 80, 0, AIR).unwrap();
            state.world.edit(0, 81, 0, AIR).unwrap();
            state.world.edit(2, 80, 0, AIR).unwrap();
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::with_components(item, 6, 1, vec![3, 9]).unwrap());
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        } else {
            let expected = if round == 1 { AIR } else { GLOWSTONE };
            assert_eq!(state.world.get_block(2, 80, 0).unwrap(), expected);
        }
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            let count = if round < 2 { 6 } else { 5 };
            assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, count);
            assert_eq!(
                peer.inventory.slots[0]
                    .as_ref()
                    .unwrap()
                    .components
                    .as_ref()
                    .unwrap()
                    .bytes
                    .as_ref(),
                [3, 9]
            );
            if round == 0 {
                let mut wrong_item = peer.request(0);
                if let ClientMessage::EntityInteract { payload, .. } = &mut wrong_item {
                    let mut decoded = Request::decode(payload).unwrap();
                    decoded.slot = 1;
                    *payload = decoded.encode().unwrap();
                }
                assert!(!peer.send(&wrong_item).0, "empty selected slot authorized");
                for mode in 1..=7 {
                    let request = peer.request(mode);
                    let (accepted, reason) = peer.send(&request);
                    assert!(!accepted, "mode {mode}: {reason}");
                    // Wire reasons are truncated; full attribution is checked
                    // separately through the same production gameplay planner.
                    assert!(reason.starts_with("demo:shift:"), "mode {mode}: {reason}");
                }
                // All failures precede the next restart; no later successful
                // write can mask a partially published block or inventory.
                assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 6);
                assert!(peer.inventory.slots[1].is_none());
                return;
            }
            if let Some(request) = &original {
                assert!(!peer.send(request).0, "old session replay accepted");
            }
            let request = peer.request(0);
            let (accepted, reason) = peer.send(&request);
            assert!(accepted, "{reason}");
            peer.inventory_at(count - 1);
            let moved = peer.inventory.slots[1].as_ref().unwrap();
            assert_eq!(moved.count, 7 - count);
            assert_eq!(moved.components.as_ref().unwrap().bytes.as_ref(), [3, 9]);
            assert_eq!(moved.components.as_ref().unwrap().version, 1);
            assert!(peer.send(&request).0, "same-session duplicate receipt");
            let mut stale = request.clone();
            if let ClientMessage::EntityInteract { action_id, .. } = &mut stale {
                let ClientMessage::EntityInteract {
                    action_id: fresh, ..
                } = peer.request(0)
                else {
                    unreachable!()
                };
                *action_id = fresh;
            }
            assert!(!peer.send(&stale).0, "stale inventory revision authorized");
            original = Some(request);
        });
    }
    let mut recovered = fixture.open().unwrap();
    assert_eq!(recovered.world.get_block(2, 80, 0).unwrap(), SAND);
    let inventory = recovered.inventory_store.load(PROFILE).unwrap();
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 4);
    assert_eq!(inventory.slots[1].as_ref().unwrap().count, 2);
}

#[test]
fn luau_action_registration_and_persisted_source_identity_fail_closed() {
    let fixture = Fixture::new();
    for register in [
        REGISTER.replace("demo:shift", "other:shift"),
        REGISTER.replace(", 1,", ", 0,"),
        REGISTER.replace("demo:action", "demo:missing"),
        REGISTER.replace("'item'", "'entity'"),
        REGISTER.replace("'bloxgloom:stick'", "'demo:missing'"),
        "return function(h) pcall(function() h.register_action({},1,'Bad','empty',nil,'demo:action') end) end".into(),
    ] {
        fixture.action(&register, SOURCE);
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save").exists());
    }
    fixture.package("demo", "module action action.luau", REGISTER);
    assert!(
        fixture.open().is_err(),
        "undeclared action capability accepted"
    );
    assert!(!fixture.0.join("save").exists());
    fixture.package("helper", "", "return function(_) end");
    fixture.action(REGISTER, SOURCE);
    drop(fixture.open().unwrap());
    let manifest = std::fs::read(fixture.0.join("save/content.map")).unwrap();
    for (register, source) in [
        (REGISTER.to_owned(), format!("{SOURCE}\n-- changed source")),
        (REGISTER.replace(", 1,", ", 2,"), SOURCE.to_owned()),
    ] {
        fixture.action(&register, &source);
        assert!(
            fixture.open().is_err(),
            "changed handler identity reopened save"
        );
        assert_eq!(
            std::fs::read(fixture.0.join("save/content.map")).unwrap(),
            manifest
        );
    }
    fixture.action(REGISTER, SOURCE);
    fixture.package("helper", "", "return function(_) end -- changed helper");
    assert!(
        fixture.open().is_err(),
        "installation source identity ignored"
    );
    assert_eq!(
        std::fs::read(fixture.0.join("save/content.map")).unwrap(),
        manifest
    );
    fixture.package("helper", "", "return function(_) end");
    drop(fixture.open().unwrap());
}

#[test]
fn luau_action_planner_errors_and_unavailable_retry_are_atomic() {
    use crate::server::gameplay::{OperationInput, Participants, plan_removals};
    use bloxgloom_host_api::gameplay::Event;
    let fixture = Fixture::new();
    fixture.action(REGISTER, SOURCE);
    let mut state = fixture.open().unwrap();
    // A running registration retains its snapshot even if local files change.
    std::fs::write(
        fixture.0.join("packages/demo/action.luau"),
        "error('changed file')",
    )
    .unwrap();
    state.world.edit(2, 80, 0, AIR).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(
        Stack::with_components(
            state
                .world
                .catalog()
                .item_by_key("bloxgloom:stick")
                .unwrap(),
            6,
            1,
            vec![3, 9],
        )
        .unwrap(),
    );
    for (mode, message) in [
        (0, ""),
        (1, "rollback script error"),
        (2, "integer"),
        (3, "unknown content"),
        (4, "Limit"),
        (5, "memory"),
        (6, "budget"),
        (7, "integer"),
        (8, "terrain unavailable"),
        (8, ""),
    ] {
        let mut reads = Default::default();
        let mut requested = Vec::new();
        let result = plan_removals(
            &mut state.world,
            &mut reads,
            &mut requested,
            OperationInput {
                edits: &[],
                removals: &[],
                seed: 7,
                tick: 1,
                action: Some(Event::ActionRequested {
                    action: "demo:shift".into(),
                    position: [0.5, 80.0, 0.5],
                    cell: None,
                    entity: None,
                    slot: 0,
                    arguments: vec![mode],
                }),
            },
            Participants {
                actor_inventory_revision: None,
                profile_inventories: None,
                profile_services: None,
                players: &[],
                action_id: None,
                clock: None,
                actor: Some((PROFILE, &inventory)),
                actor_position: Some([0.5, 80.0, 0.5]),
                admin: false,
                entities: &state.entities,
            },
        );
        if message.is_empty() {
            let plan = result.unwrap();
            assert_eq!(plan.edits, [(2, 80, 0, GLOWSTONE)]);
            assert_eq!(plan.inventory.unwrap().slots[0].as_ref().unwrap().count, 5);
        } else {
            let error = result.err().expect("failed handler published a plan");
            assert!(error.to_string().contains(message), "mode {mode}: {error}");
            if mode == 8 {
                assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
                let key = crate::world::world_to_chunk(1600, 80, 0).0;
                assert_eq!(requested, [key]);
                // Satisfy the recorded input before one fresh retry; no sleeps
                // or fallback terrain guesses can conceal lost progress.
                state.world.get_chunk(key).unwrap();
            }
        }
        assert_eq!(state.world.get_block(2, 80, 0).unwrap(), AIR);
        assert_eq!(inventory.slots[0].as_ref().unwrap().count, 6);
    }
}
