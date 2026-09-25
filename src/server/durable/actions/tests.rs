use super::*;
use crate::items::{ItemId, STICK};
use crate::server::entities::{CellCoord, KilnSlot, kiln_block_states, kiln_payload};
use crate::server::movement::MovementState;
use crate::server::{Client, DEFAULT_VIEW, State, server_state};
use crate::world::{GRASS, MAX_GENERATED_HEIGHT, RED_FLOWER};
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_save_dir(label: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("bloxgloom-{label}-{}-{stamp}", std::process::id()))
}

fn add_test_client(state: &mut State, position: [f32; 3], inventory: Inventory) -> TcpStream {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, _receiver) = state.outbound.client_queue();
    let center = world_to_chunk(
        position[0].floor() as i32,
        position[1].floor() as i32,
        position[2].floor() as i32,
    )
    .0;
    state.clients.insert(
        1,
        Client {
            profile: 17,
            inventory,
            last_drops_revision: u64::MAX,
            last_drop_anchor: [i32::MAX; 3],
            last_sent_drops: Vec::new(),
            sender,
            socket,
            sent: Default::default(),
            sent_epochs: Default::default(),
            sent_block_versions: Default::default(),
            sent_entity_revisions: Default::default(),
            next_snapshot_epoch: 1,
            center,
            radius: DEFAULT_VIEW,
            movement: MovementState::new(position, 0),
            pending_moves: Default::default(),
        },
    );
    peer
}

fn edit_request(message: ClientMessage) -> DurableRequest {
    DurableRequest::Command {
        id: 1,
        message,
        queued_at: Instant::now(),
    }
}

fn grant_action_epoch(state: &mut State, profile: u128) -> u64 {
    assert!(
        state
            .durability
            .request_epoch_grant(profile, TickId::new(1))
            .unwrap()
            .is_none()
    );
    for _ in 0..2_000 {
        super::super::receipt::poll_journal_receipts(state).unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    state
        .durability
        .request_epoch_grant(profile, TickId::new(2))
        .unwrap()
        .unwrap()
}

fn settle_live_action(state: &mut State, tick: u64, message: ClientMessage) {
    super::super::coordinator::handle_live_message(state, 1, message, TickId::new(tick)).unwrap();
    for current in tick..tick + 2_000 {
        super::super::coordinator::process_durable_actions(
            state,
            TickId::new(current),
            Instant::now(),
        )
        .unwrap();
        if state.durability.queued.is_empty() && state.durability.pending.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("live durable action did not settle");
}

#[test]
fn admin_grant_requires_server_authorization_and_persists_inventory() {
    let path = temp_save_dir("admin-grant");
    let mut state = server_state(23, path.clone()).unwrap();
    let _peer = add_test_client(&mut state, [0.5, 80.0, 0.5], Inventory::default());
    let epoch = grant_action_epoch(&mut state, 17);
    let item = state.world.catalog().items().next().unwrap().id;
    settle_live_action(
        &mut state,
        3,
        ClientMessage::AdminGive {
            action_id: (u128::from(epoch) << 64) | 1,
            item,
            count: 128,
        },
    );
    assert!(!state.durability.failed);
    assert_eq!(state.clients[&1].inventory.slots[0], None);

    state.admin_profile = Some(17);
    settle_live_action(
        &mut state,
        4,
        ClientMessage::AdminGive {
            action_id: (u128::from(epoch) << 64) | 2,
            item,
            count: 128,
        },
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0],
        Some(crate::inventory::Stack::new(item, 128))
    );
    drop(state);
    let state = server_state(23, path.clone()).unwrap();
    assert_eq!(
        state.inventory_store.load(17).unwrap().slots[0],
        Some(crate::inventory::Stack::new(item, 128))
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn missing_plant_support_requests_its_exact_vertical_neighbor() {
    let path = temp_save_dir("edit-support-prefetch");
    let mut state = server_state(23, path.clone()).unwrap();
    state.world.reset_cache_for_test(1);
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16;
    let target = world_to_chunk(0, target_y, 0).0;
    let support = world_to_chunk(0, target_y - 1, 0).0;
    assert_ne!(target, support);
    state.world.get_chunk(target).unwrap();

    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(
        crate::items::ItemId::new(RED_FLOWER.get()),
        1,
    ));
    let peer = add_test_client(&mut state, [0.5, target_y as f32, 0.5], inventory);
    let request = edit_request(ClientMessage::Edit {
        action_id: 1,
        x: 0,
        y: target_y,
        z: 0,
        block: RED_FLOWER,
        slot: 0,
    });

    let result = plan_durable_request(&mut state, &request, TickId::new(1));
    let error = result.err().expect("missing support must defer the edit");
    assert_eq!(error.kind(), ErrorKind::WouldBlock, "{error}");
    assert!(state.loader.is_pending(support));
    assert!(state.world.cached_block(0, target_y, 0).is_some());

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn missing_plant_check_requests_the_exact_above_chunk() {
    let path = temp_save_dir("edit-above-prefetch");
    let mut state = server_state(29, path.clone()).unwrap();
    // A one-chunk cache lets the test keep the harvest target resident while
    // its vertical neighbor remains absent.
    state.world.reset_cache_for_test(1);
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16 - 1;
    let target = world_to_chunk(0, target_y, 0).0;
    let above = world_to_chunk(0, target_y + 1, 0).0;
    assert_ne!(target, above);
    state.world.get_chunk(target).unwrap();
    state.world.edit(0, target_y, 0, GRASS).unwrap();

    let peer = add_test_client(
        &mut state,
        [0.5, target_y as f32, 0.5],
        Inventory::default(),
    );
    let request = edit_request(ClientMessage::Edit {
        action_id: 2,
        x: 0,
        y: target_y,
        z: 0,
        block: crate::world::AIR,
        slot: 0,
    });

    let result = plan_durable_request(&mut state, &request, TickId::new(1));
    assert!(matches!(result, Err(error) if error.kind() == ErrorKind::WouldBlock));
    assert!(state.loader.is_pending(above));
    assert!(state.world.cached_block(0, target_y, 0).is_some());

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn kiln_plan_spans_vertical_chunk_seam_without_pre_wal_visibility() {
    let path = temp_save_dir("kiln-seam-plan");
    let mut state = server_state(31, path.clone()).unwrap();
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16 - 1;
    let lower = world_to_chunk(0, target_y, 0).0;
    let upper = world_to_chunk(0, target_y + 1, 0).0;
    assert_ne!(lower, upper);
    state.world.get_chunk(lower).unwrap();
    state.world.get_chunk(upper).unwrap();

    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(crate::content::KILN_ITEM, 1));
    let peer = add_test_client(&mut state, [0.5, target_y as f32, -2.5], inventory);
    let facing_east = state
        .world
        .catalog()
        .state_with_property(crate::content::KILN_DEFAULT_STATE, "facing", "east")
        .unwrap();
    let request = edit_request(ClientMessage::Edit {
        action_id: 3,
        x: 0,
        y: target_y,
        z: 0,
        block: facing_east,
        slot: 0,
    });
    let action = plan_durable_request(&mut state, &request, TickId::new(1))
        .unwrap()
        .unwrap();
    assert_eq!(action.world_edits.len(), 2);
    assert_eq!(action.deltas.len(), 2);
    assert_eq!(action.inventory.as_ref().unwrap().slots[0], None);
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();
    assert_eq!(state.entities.len(), 0);
    assert_eq!(state.world.cached_block(0, target_y, 0), Some(AIR));
    assert_eq!(state.world.cached_block(0, target_y + 1, 0), Some(AIR));
    assert_eq!(
        state.clients.get(&1).unwrap().inventory.slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn kiln_place_interact_tick_restart_and_break_conserve_items_across_seam() {
    let path = temp_save_dir("kiln-live-seam-restart");
    let profile = 17;
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16 - 1;
    let anchor = CellCoord::new(-1, target_y, -1);
    let upper = CellCoord::new(-1, target_y + 1, -1);
    let lower_chunk = anchor.chunk();
    let upper_chunk = upper.chunk();
    assert_ne!(lower_chunk, upper_chunk);

    let mut state = server_state(53, path.clone()).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(crate::content::KILN_ITEM, 1));
    inventory.slots[1] = Some(crate::inventory::Stack::new(STICK, 1));
    inventory.slots[2] = Some(crate::inventory::Stack::new(
        ItemId(crate::world::GRAVEL.0),
        1,
    ));
    let player_peer = add_test_client(
        &mut state,
        [
            anchor.x as f32 + 0.5,
            anchor.y as f32,
            anchor.z as f32 + 3.5,
        ],
        inventory,
    );
    state.world.get_chunk(lower_chunk).unwrap();
    state.world.get_chunk(upper_chunk).unwrap();
    assert_eq!(
        state.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(AIR)
    );
    assert_eq!(
        state.world.cached_block(upper.x, upper.y, upper.z),
        Some(AIR)
    );

    let epoch = grant_action_epoch(&mut state, profile);
    let action_id = |sequence: u64| (u128::from(epoch) << 64) | u128::from(sequence);
    settle_live_action(
        &mut state,
        10,
        ClientMessage::Edit {
            action_id: action_id(1),
            x: anchor.x,
            y: anchor.y,
            z: anchor.z,
            block: crate::content::KILN_DEFAULT_STATE,
            slot: 0,
        },
    );
    let entity_id = state
        .entities
        .anchored_at(anchor)
        .expect("WAL-installed kiln");
    assert_eq!(state.entities.anchored_at(upper), Some(entity_id));
    assert_eq!(
        state.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(crate::content::KILN_DEFAULT_STATE)
    );

    let fuel_request = ClientMessage::EntityInteract {
        action_id: action_id(2),
        target: [anchor.x, anchor.y, anchor.z],
        payload: vec![1, 0, 0, 1, 1, 0], // insert one stick as fuel
    };
    super::super::coordinator::handle_live_message(&mut state, 1, fuel_request, TickId::new(11))
        .unwrap();
    super::super::coordinator::process_durable_actions(&mut state, TickId::new(11), Instant::now())
        .unwrap();
    let lower_key = super::super::chunk_state_key(lower_chunk);
    let upper_key = super::super::chunk_state_key(upper_chunk);
    assert!(state.durability.reserved.contains(&lower_key));
    assert!(state.durability.reserved.contains(&upper_key));
    let staged = match &state.durability.pending[0].payload {
        super::super::PendingPayload::Action(action) => action,
        super::super::PendingPayload::Fire(_) => panic!("kiln action staged as fire"),
    };
    assert!(staged.world_edits.is_empty());
    assert!(
        staged
            .entities
            .as_ref()
            .unwrap()
            .changes()
            .iter()
            .all(|change| change.key.domain != "bloxgloom:chunk_snapshot")
    );

    let overlapping_edit = state
        .world
        .prepare_edits(&[(upper.x, upper.y, upper.z, crate::world::STONE)])
        .unwrap();
    let overlapping_action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: overlapping_edit,
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entity_wakes: Vec::new(),
        entities: None,
    };
    assert!(matches!(
        state
            .durability
            .try_stage(TickId::new(12), &overlapping_action, None, None),
        Err(super::super::StageError::Conflict)
    ));
    for current in 12..2_012 {
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(current),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() && state.durability.queued.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!state.durability.reserved.contains(&lower_key));
    assert!(!state.durability.reserved.contains(&upper_key));
    settle_live_action(
        &mut state,
        12,
        ClientMessage::EntityInteract {
            action_id: action_id(3),
            target: [anchor.x, anchor.y, anchor.z],
            payload: vec![1, 0, 1, 2, 1, 0], // insert one gravel input
        },
    );
    assert_eq!(state.clients[&1].inventory.slots[1], None);
    assert_eq!(state.clients[&1].inventory.slots[2], None);

    for tick in [30, 50, 70, 90] {
        super::super::coordinator::queue_interaction_actions(&mut state, TickId::new(tick));
        for current in tick..tick + 2_000 {
            super::super::coordinator::process_durable_actions(
                &mut state,
                TickId::new(current),
                Instant::now(),
            )
            .unwrap();
            if state.durability.queued.is_empty() && state.durability.pending.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(state.durability.pending.is_empty());
        assert!(state.durability.queued.is_empty());
    }
    let cooked = state.entities.snapshot(entity_id).unwrap();
    let payload = kiln_payload(&cooked).unwrap();
    assert_eq!(
        payload.slot(KilnSlot::Output),
        Some(&crate::inventory::Stack::new(
            ItemId(crate::world::STONE.0),
            1
        ))
    );
    assert!(payload.is_lit());
    let lit_states = kiln_block_states(state.world.catalog(), payload).unwrap();
    assert_eq!(
        state.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(lit_states[0])
    );
    assert_eq!(
        state.world.cached_block(upper.x, upper.y, upper.z),
        Some(lit_states[1])
    );
    assert_eq!(state.durability.receipt_ledger(profile).results.len(), 3);
    drop(player_peer);
    drop(state);

    let mut recovered = server_state(53, path.clone()).unwrap();
    recovered.world.get_chunk(lower_chunk).unwrap();
    recovered.world.get_chunk(upper_chunk).unwrap();
    let recovered_id = recovered
        .entities
        .anchored_at(anchor)
        .expect("recovered kiln");
    assert_eq!(recovered_id, entity_id);
    let recovered_snapshot = recovered.entities.snapshot(recovered_id).unwrap();
    let recovered_payload = kiln_payload(&recovered_snapshot).unwrap();
    assert!(recovered_payload.is_lit());
    assert_eq!(
        recovered_payload.slot(KilnSlot::Output),
        Some(&crate::inventory::Stack::new(
            ItemId(crate::world::STONE.0),
            1
        ))
    );
    let recovered_states = kiln_block_states(recovered.world.catalog(), recovered_payload).unwrap();
    assert_eq!(
        recovered.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(recovered_states[0])
    );
    assert_eq!(
        recovered.world.cached_block(upper.x, upper.y, upper.z),
        Some(recovered_states[1])
    );

    let inventory = recovered.inventory_store.load(profile).unwrap();
    let peer = add_test_client(
        &mut recovered,
        [
            anchor.x as f32 + 0.5,
            anchor.y as f32,
            anchor.z as f32 + 3.5,
        ],
        inventory,
    );
    settle_live_action(
        &mut recovered,
        100,
        ClientMessage::Edit {
            action_id: action_id(4),
            x: upper.x,
            y: upper.y,
            z: upper.z,
            block: AIR,
            slot: 0,
        },
    );
    assert_eq!(recovered.entities.anchored_at(anchor), None);
    assert_eq!(recovered.entities.anchored_at(upper), None);
    assert_eq!(
        recovered.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(AIR)
    );
    assert_eq!(
        recovered.world.cached_block(upper.x, upper.y, upper.z),
        Some(AIR)
    );
    let drop_position = [
        anchor.x as f32 + 0.5,
        anchor.y as f32 + 0.5,
        anchor.z as f32 + 0.5,
    ];
    let stacks: Vec<_> = recovered
        .drops
        .nearby(drop_position)
        .iter()
        .map(|drop| recovered.drops.stack(drop.id).unwrap())
        .collect();
    assert_eq!(stacks.len(), 2);
    assert_eq!(stacks.iter().map(|stack| stack.count).sum::<u16>(), 2);
    assert!(
        stacks
            .iter()
            .any(|stack| stack.item == crate::content::KILN_ITEM)
    );
    assert!(
        stacks
            .iter()
            .any(|stack| stack.item == ItemId(crate::world::STONE.0))
    );

    drop(peer);
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}

struct CounterCodec;

impl crate::server::entities::EntityPayloadCodec for CounterCodec {
    fn decode(
        &self,
        bytes: &[u8],
    ) -> Result<crate::server::entities::EntityPayload, crate::server::entities::EntityCodecError>
    {
        let [value] = bytes else {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        };
        Ok(crate::server::entities::EntityPayload::new(*value))
    }

    fn encode(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        payload
            .downcast_ref::<u8>()
            .copied()
            .map(|value| vec![value])
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)
    }

    fn public_view(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        self.encode(payload)
    }
}

/// Private payload whose public projection deliberately omits a field.
/// The codec encodes both bytes for persistence but projects only `shown`,
/// so any planner-observable `secret` byte proves a privacy-boundary break.
#[derive(Clone)]
struct MatePayload {
    shown: u8,
    secret: u8,
}

struct MateCodec;

impl crate::server::entities::EntityPayloadCodec for MateCodec {
    fn decode(
        &self,
        bytes: &[u8],
    ) -> Result<crate::server::entities::EntityPayload, crate::server::entities::EntityCodecError>
    {
        let [shown, secret] = bytes else {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        };
        Ok(crate::server::entities::EntityPayload::new(MatePayload {
            shown: *shown,
            secret: *secret,
        }))
    }

    fn encode(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        payload
            .downcast_ref::<MatePayload>()
            .map(|mate| vec![mate.shown, mate.secret])
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)
    }

    fn public_view(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        payload
            .downcast_ref::<MatePayload>()
            .map(|mate| vec![mate.shown])
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)
    }
}

/// A non-kiln anchored probe that branches on its neighbours: it copies the
/// public last byte of the lowest-ID mate-type neighbour into its own
/// payload. Echoing the last byte (not the first) keeps the privacy test
/// sensitive: a leaked private encoding would end in the secret byte.
struct PairTick {
    mate_type: crate::content::EntityTypeId,
}

impl crate::server::entities::EntityTickPolicy for PairTick {
    fn read_radius_chunks(&self) -> u8 {
        1
    }

    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        current_tick: u64,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{
            EntityBlockStateChange, EntityError, EntityPayload, EntityTickPlan,
        };
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        if current_tick < due {
            return Err(EntityError::InvalidType);
        }
        let (anchor, anchor_state) = match &snapshot.location {
            crate::server::entities::EntityLocation::Anchored {
                anchor,
                anchor_state,
                ..
            } => (*anchor, *anchor_state),
            crate::server::entities::EntityLocation::Mobile { .. } => {
                return Err(EntityError::WrongOwnership);
            }
        };
        let echo = neighbours
            .iter()
            .filter(|view| view.entity_type == self.mate_type)
            .min_by_key(|view| view.id)
            .and_then(|view| view.payload.last().copied())
            .ok_or(EntityError::InvalidType)?;
        Ok(EntityTickPlan {
            payload: Some(EntityPayload::new(echo)),
            next_tick: current_tick + 5,
            anchor_update: None,
            block_states: vec![EntityBlockStateChange {
                cell: anchor,
                before: anchor_state,
                after: anchor_state,
            }],
            wakes: Vec::new(),
            transfer: None,
        })
    }
}

/// A non-kiln anchored/mobile probe type. Its planners echo the live anchor
/// state with no writes, so the test exercises generic dispatch and
/// validation without coupling to kiln rules.
struct CounterInteract;
impl crate::server::entities::EntityInteractionPolicy for CounterInteract {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        _request: &[u8],
        inventory: &crate::inventory::Inventory,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        _neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityInteractionPlan, crate::server::entities::EntityError>
    {
        use crate::server::entities::{EntityBlockStateChange, EntityInteractionPlan};
        let (anchor, anchor_state) = match &snapshot.location {
            crate::server::entities::EntityLocation::Anchored {
                anchor,
                anchor_state,
                ..
            } => (*anchor, *anchor_state),
            crate::server::entities::EntityLocation::Mobile { .. } => {
                return Err(crate::server::entities::EntityError::WrongOwnership);
            }
        };
        let mut next = inventory.clone();
        next.revision = next.revision.wrapping_add(1);
        Ok(EntityInteractionPlan {
            payload: crate::server::entities::EntityPayload::new(9u8),
            inventory: next,
            block_states: vec![EntityBlockStateChange {
                cell: anchor,
                before: anchor_state,
                after: anchor_state,
            }],
            wakes: Vec::new(),
        })
    }
}

struct CounterTick;

impl crate::server::entities::EntityTickPolicy for CounterTick {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        current_tick: u64,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        _neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::EntityTickPlan;
        let Some(due) = snapshot.next_tick else {
            return Err(crate::server::entities::EntityError::InvalidType);
        };
        if current_tick < due {
            return Err(crate::server::entities::EntityError::InvalidType);
        }
        Ok(EntityTickPlan {
            payload: Some(crate::server::entities::EntityPayload::new(8u8)),
            next_tick: current_tick + 5,
            anchor_update: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
        })
    }
}

/// A non-kiln anchored probe that reads the cell above its anchor and
/// branches its payload on solidity. Declares a one-chunk read radius.
struct WatcherTick;

impl crate::server::entities::EntityTickPolicy for WatcherTick {
    fn read_radius_chunks(&self) -> u8 {
        1
    }

    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        current_tick: u64,
        catalog: &crate::content::Catalog,
        view: &crate::server::voxel_view::VoxelView,
        _neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{
            EntityBlockStateChange, EntityError, EntityPayload, EntityTickPlan,
        };
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        if current_tick < due {
            return Err(EntityError::InvalidType);
        }
        let (anchor, anchor_state) = match &snapshot.location {
            crate::server::entities::EntityLocation::Anchored {
                anchor,
                anchor_state,
                ..
            } => (*anchor, *anchor_state),
            crate::server::entities::EntityLocation::Mobile { .. } => {
                return Err(EntityError::WrongOwnership);
            }
        };
        let above = view
            .block(anchor.x, anchor.y + 1, anchor.z)
            .map_err(|_| EntityError::ViewOutOfRange)?;
        let solid = catalog.block_flags(above) & crate::content::SOLID != 0;
        Ok(EntityTickPlan {
            payload: Some(EntityPayload::new(u8::from(solid))),
            next_tick: current_tick + 5,
            anchor_update: None,
            block_states: vec![EntityBlockStateChange {
                cell: anchor,
                before: anchor_state,
                after: anchor_state,
            }],
            wakes: Vec::new(),
            transfer: None,
        })
    }
}

/// A radius-zero mobile probe for the retry contract. On its first due tick
/// it reads far outside its captured home chunk and must fail closed; on any
/// later tick it reads inside the capture and must plan normally. The tick
/// does the scoping because planners are pure: only the tick (and the world)
/// can differ between attempts.
struct FarReadTick;

impl crate::server::entities::EntityTickPolicy for FarReadTick {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        current_tick: u64,
        _catalog: &crate::content::Catalog,
        view: &crate::server::voxel_view::VoxelView,
        _neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{EntityError, EntityPayload, EntityTickPlan};
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        if current_tick < due {
            return Err(EntityError::InvalidType);
        }
        let position = match &snapshot.location {
            crate::server::entities::EntityLocation::Mobile { position } => *position,
            crate::server::entities::EntityLocation::Anchored { .. } => {
                return Err(EntityError::WrongOwnership);
            }
        };
        if current_tick == due {
            return match view.block(
                position[0] as i32 + 64,
                position[1] as i32,
                position[2] as i32,
            ) {
                Err(_) => Err(EntityError::ViewOutOfRange),
                Ok(_) => Ok(EntityTickPlan {
                    payload: Some(EntityPayload::new(42u8)),
                    next_tick: current_tick + 5,
                    anchor_update: None,
                    block_states: Vec::new(),
                    wakes: Vec::new(),
                    transfer: None,
                }),
            };
        }
        view.block(position[0] as i32, position[1] as i32, position[2] as i32)
            .map_err(|_| EntityError::ViewOutOfRange)?;
        Ok(EntityTickPlan {
            payload: Some(EntityPayload::new(43u8)),
            next_tick: current_tick + 5,
            anchor_update: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
        })
    }
}

#[test]
fn generic_entity_path_serves_non_kiln_tick_and_interaction() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    let path = temp_save_dir("generic-entity-path");
    let mobile_id_type = crate::content::EntityTypeId(70_004);
    let anchored_id_type = crate::content::EntityTypeId(70_005);
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key) in [
        (mobile_id_type, "test:counter_mobile"),
        (anchored_id_type, "test:counter_anchored"),
    ] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: 0x434f_554e_5400_0001,
            })
            .unwrap();
    }
    let compatible: BTreeSet<_> = catalog
        .identities()
        .into_iter()
        .filter(|(kind, id, _, _)| *kind == b'S' && *id != 0)
        .map(|(_, id, _, _)| crate::content::BlockStateId(id))
        .collect();
    assert!(!compatible.is_empty());
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:counter_mobile".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(CounterTick)),
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:counter_anchored".into(),
        ownership: EntityOwnership::anchored(compatible, 1),
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: Some(Arc::new(CounterInteract)),
        tick_planner: None,
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();

    // Anchored interaction through the generic dispatcher. The anchor is the
    // highest non-air cell in its column so the footprint preimage is stable.
    let mut anchor = None;
    for y in (1..100).rev() {
        let key = world_to_chunk(0, y, 0).0;
        state.world.get_chunk(key).unwrap();
        if state.world.cached_block(0, y, 0) != Some(AIR) {
            anchor = Some(CellCoord::new(0, y, 0));
            break;
        }
    }
    let anchor = anchor.expect("terrain column has a non-air cell");
    let resident = state
        .world
        .cached_block(anchor.x, anchor.y, anchor.z)
        .expect("chosen anchor cell is resident");
    let peer = add_test_client(
        &mut state,
        [0.5, anchor.y as f32, 3.5],
        Inventory::default(),
    );

    // Mobile tick through the generic dispatcher (previously kiln-routed).
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: mobile_id_type,
            position: [0.5, 80.0, 0.5],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let mobile_id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    assert_eq!(
        state.entities.snapshot(mobile_id).unwrap().next_tick,
        Some(6)
    );
    let action = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: mobile_id },
        TickId::new(6),
    )
    .unwrap()
    .expect("due non-kiln tick plans through the generic path");
    assert!(action.world_edits.is_empty());
    assert!(action.entities.is_some());
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();
    // Planning stages a WAL record; the live store is untouched before receipt.
    assert_eq!(
        state.entities.snapshot(mobile_id).unwrap().next_tick,
        Some(6)
    );

    // Anchored interaction planning for the probe type.
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Anchored {
            entity_type: anchored_id_type,
            anchor,
            anchor_state: resident,
            footprint: vec![anchor],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let anchored_id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    let request = edit_request(ClientMessage::EntityInteract {
        action_id: 9,
        target: [anchor.x, anchor.y, anchor.z],
        payload: vec![0],
    });
    let action = plan_durable_request(&mut state, &request, TickId::new(7))
        .unwrap()
        .expect("non-kiln interaction plans through the generic path");
    assert_eq!(action.inventory.as_ref().unwrap().revision, 1);
    assert!(action.world_edits.is_empty());
    assert!(action.entities.is_some());
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();
    // The stored payload is unchanged until the WAL receipt applies.
    assert_eq!(
        state
            .entities
            .snapshot(anchored_id)
            .unwrap()
            .private_payload
            .downcast_ref::<u8>(),
        Some(&7u8)
    );

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_planner_branches_on_neighbor_and_replans_identically() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    let path = temp_save_dir("watcher-neighbor-plan");
    let watcher_type = crate::content::EntityTypeId(70_006);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: watcher_type,
            key: "test:watcher".into(),
            schema_version: 1,
            schema_fingerprint: 0x5741_5443_4800_0001,
        })
        .unwrap();
    let compatible: BTreeSet<_> = catalog
        .identities()
        .into_iter()
        .filter(|(kind, id, _, _)| *kind == b'S' && *id != 0)
        .map(|(_, id, _, _)| crate::content::BlockStateId(id))
        .collect();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:watcher".into(),
        ownership: EntityOwnership::anchored(compatible, 1),
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(WatcherTick)),
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    // Scan a far column so most of the declared read set starts unloaded;
    // the first plan attempts must defer through the loader path.
    let mut anchor = None;
    for y in (1..100).rev() {
        let key = world_to_chunk(512, y, 0).0;
        state.world.get_chunk(key).unwrap();
        if state.world.cached_block(512, y, 0) != Some(AIR) {
            anchor = Some(CellCoord::new(512, y, 0));
            break;
        }
    }
    let anchor = anchor.expect("terrain column has a non-air cell");
    // The neighbor read may cross a chunk boundary; keep it resident so the
    // first plan attempt exercises branching rather than deferral.
    state
        .world
        .get_chunk(world_to_chunk(anchor.x, anchor.y + 1, anchor.z).0)
        .unwrap();
    let resident = state
        .world
        .cached_block(anchor.x, anchor.y, anchor.z)
        .expect("chosen anchor cell is resident");
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Anchored {
            entity_type: watcher_type,
            anchor,
            anchor_state: resident,
            footprint: vec![anchor],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();

    // Neighbor chunks stream in through the real deferral path; each
    // WouldBlock requests the still-missing remainder of the read set.
    let mut action = None;
    let mut deferred = false;
    for _ in 0..50 {
        match plan_durable_request(
            &mut state,
            &DurableRequest::EntityTick { id },
            TickId::new(6),
        ) {
            Ok(planned) => {
                action = planned;
                break;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                deferred = true;
                for _ in 0..2_000 {
                    crate::server::streaming::poll_chunk_loads(&mut state).unwrap();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            Err(error) => panic!("watcher tick must plan or defer, got {error:?}"),
        }
    }
    assert!(deferred, "read set must miss at least one chunk");
    let action = action.expect("watcher read set becomes resident");
    assert!(action.world_edits.is_empty());
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();

    // Identical inputs produce identical plans across repeated calls.
    let snapshot = state.entities.snapshot(id).unwrap();
    let view = entity::capture_view_for_plan(&mut state, &snapshot.location, 1).unwrap();
    let neighbours =
        entity::capture_entity_view_for_plan(&mut state, &snapshot.location, 1, id).unwrap();
    let catalog = state.world.catalog_arc();
    let descriptor = state.entities.types().descriptor(watcher_type).unwrap();
    let first = descriptor
        .plan_tick(&snapshot, 6, &catalog, &view, &neighbours)
        .unwrap();
    let second = descriptor
        .plan_tick(&snapshot, 6, &catalog, &view, &neighbours)
        .unwrap();
    assert_eq!(first.next_tick, second.next_tick);
    assert_eq!(first.block_states, second.block_states);
    let planned_byte = |plan: &crate::server::entities::EntityTickPlan| {
        plan.payload.as_ref().unwrap().downcast_ref::<u8>().copied()
    };
    assert_eq!(planned_byte(&first), planned_byte(&second));
    let above = state
        .world
        .cached_block(anchor.x, anchor.y + 1, anchor.z)
        .unwrap();
    assert_eq!(
        planned_byte(&first),
        Some(u8::from(
            catalog.block_flags(above) & crate::content::SOLID != 0
        )),
        "planned payload branches on the neighboring block"
    );

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_planner_read_outside_capture_is_an_error() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::sync::Arc;

    let path = temp_save_dir("planner-read-outside-capture");
    let far_type = crate::content::EntityTypeId(70_007);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: far_type,
            key: "test:far_read".into(),
            schema_version: 1,
            schema_fingerprint: 0x4641_5232_4400_0001,
        })
        .unwrap();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:far_read".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(FarReadTick)),
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: far_type,
            position: [0.5, 80.0, 0.5],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();

    let result = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id },
        TickId::new(6),
    );
    assert!(
        matches!(result, Err(error) if error.kind() == ErrorKind::InvalidInput),
        "a read outside the captured set must error, not read air"
    );
    assert!(!state.durability.failed);
    // A rejected plan must leave the entity schedulable: its due tick is
    // unchanged, it is still due, and a later attempt with an in-range read
    // plans normally.
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, Some(6));
    assert!(state.entities.due_entities(7, 8).contains(&id));
    let action = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id },
        TickId::new(7),
    )
    .unwrap()
    .expect("in-range retry plans after the rejected attempt");
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_planner_reads_neighbour_public_view() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    let path = temp_save_dir("pair-neighbour-plan");
    let mate_type = crate::content::EntityTypeId(70_008);
    let watcher_type = crate::content::EntityTypeId(70_010);
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key) in [
        (mate_type, "test:mate"),
        (watcher_type, "test:pair_watcher"),
    ] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: 0x5041_4952_5400_0001,
            })
            .unwrap();
    }
    let compatible: BTreeSet<_> = catalog
        .identities()
        .into_iter()
        .filter(|(kind, id, _, _)| *kind == b'S' && *id != 0)
        .map(|(_, id, _, _)| crate::content::BlockStateId(id))
        .collect();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:mate".into(),
        ownership: EntityOwnership::anchored(compatible.clone(), 2),
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 2,
        codec: Arc::new(MateCodec),
        interaction_policy: None,
        tick_planner: None,
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:pair_watcher".into(),
        ownership: EntityOwnership::anchored(compatible, 1),
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(PairTick { mate_type })),
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    let mut anchor = None;
    for y in (1..100).rev() {
        let key = world_to_chunk(0, y, 0).0;
        state.world.get_chunk(key).unwrap();
        if state.world.cached_block(0, y, 0) != Some(AIR) {
            anchor = Some(CellCoord::new(0, y, 0));
            break;
        }
    }
    let anchor = anchor.expect("terrain column has a non-air cell");
    let y = anchor.y;
    let resident = state
        .world
        .cached_block(anchor.x, anchor.y, anchor.z)
        .expect("chosen anchor cell is resident");
    // Mate anchor states never touch the world at spawn time, so they can be
    // arbitrary non-zero states; only the planned watcher's preimage is read.
    let stone = crate::world::STONE;
    let spawn_watcher = state
        .entities
        .prepare_spawn(EntitySpawn::Anchored {
            entity_type: watcher_type,
            anchor,
            anchor_state: resident,
            footprint: vec![anchor],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let watcher_id = spawn_watcher.entity_id();
    state.entities.apply_committed(spawn_watcher).unwrap();
    let spawn_named =
        |state: &mut State, anchor: CellCoord, footprint: Vec<CellCoord>, shown: u8, secret: u8| {
            let spawn = state
                .entities
                .prepare_spawn(EntitySpawn::Anchored {
                    entity_type: mate_type,
                    anchor,
                    anchor_state: stone,
                    footprint,
                    payload: EntityPayload::new(MatePayload { shown, secret }),
                    spawn_tick: 1,
                })
                .unwrap();
            let id = spawn.entity_id();
            state.entities.apply_committed(spawn).unwrap();
            id
        };
    let mate_a = spawn_named(
        &mut state,
        CellCoord::new(20, y, 0),
        vec![CellCoord::new(20, y, 0)],
        11,
        12,
    );
    let mate_b = spawn_named(
        &mut state,
        CellCoord::new(-4, y, 0),
        vec![CellCoord::new(-4, y, 0)],
        22,
        23,
    );
    let mate_c = spawn_named(
        &mut state,
        CellCoord::new(512, 80, 0),
        vec![CellCoord::new(512, 80, 0)],
        44,
        45,
    );
    let upper = CellCoord::new(16, y, 0);
    let mate_d = spawn_named(
        &mut state,
        CellCoord::new(15, y, 0),
        vec![CellCoord::new(15, y, 0), upper],
        33,
        34,
    );
    assert!(mate_a < mate_b && mate_b < mate_d);
    // The far mate's chunk is resident but outside the declared read set, so
    // residency alone must never make it visible.
    state.world.get_chunk(world_to_chunk(512, 80, 0).0).unwrap();

    let mut action = None;
    for _ in 0..50 {
        match plan_durable_request(
            &mut state,
            &DurableRequest::EntityTick { id: watcher_id },
            TickId::new(6),
        ) {
            Ok(planned) => {
                action = planned;
                break;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                for _ in 0..2_000 {
                    crate::server::streaming::poll_chunk_loads(&mut state).unwrap();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            Err(error) => panic!("pair watcher tick must plan or defer, got {error:?}"),
        }
    }
    let action = action.expect("pair read set becomes resident");
    assert!(action.world_edits.is_empty());
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();

    let snapshot = state.entities.snapshot(watcher_id).unwrap();
    let neighbours =
        entity::capture_entity_view_for_plan(&mut state, &snapshot.location, 1, watcher_id)
            .unwrap();
    // Sorted by ID across chunk pages, deduplicated across the two pages
    // mate D touches, self excluded, far mate outside the set absent.
    assert_eq!(neighbours.len(), 3);
    let ids: Vec<_> = neighbours.iter().map(|view| view.id).collect();
    assert_eq!(ids, vec![mate_a, mate_b, mate_d]);
    assert!(
        !ids.contains(&mate_c),
        "a neighbour outside the declared capture set is invisible"
    );
    for (id, shown, secret) in [
        (mate_a, 11u8, 12u8),
        (mate_b, 22u8, 23u8),
        (mate_d, 33u8, 34u8),
    ] {
        let entry = neighbours.iter().find(|view| view.id == id).unwrap();
        assert_eq!(
            entry.payload,
            vec![shown],
            "neighbour projection is exactly the public view; secret {secret} must not cross"
        );
    }
    // Deterministic ordering across repeated captures.
    let again = entity::capture_entity_view_for_plan(&mut state, &snapshot.location, 1, watcher_id)
        .unwrap();
    assert_eq!(again.iter().map(|view| view.id).collect::<Vec<_>>(), ids);

    // The plan branches on the lowest-ID mate's public last byte. A leaked
    // private encoding would end in the secret byte instead.
    let view = entity::capture_view_for_plan(&mut state, &snapshot.location, 1).unwrap();
    let catalog = state.world.catalog_arc();
    let descriptor = state.entities.types().descriptor(watcher_type).unwrap();
    let first = descriptor
        .plan_tick(&snapshot, 6, &catalog, &view, &neighbours)
        .unwrap();
    let second = descriptor
        .plan_tick(&snapshot, 6, &catalog, &view, &neighbours)
        .unwrap();
    let planned_byte = |plan: &crate::server::entities::EntityTickPlan| {
        plan.payload.as_ref().unwrap().downcast_ref::<u8>().copied()
    };
    assert_eq!(planned_byte(&first), Some(11));
    assert_eq!(planned_byte(&first), planned_byte(&second));

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_planner_unavailable_neighbour_defers() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    let path = temp_save_dir("neighbour-defer-plan");
    let roamer_type = crate::content::EntityTypeId(70_009);
    let watcher_type = crate::content::EntityTypeId(70_006);
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key) in [(roamer_type, "test:roamer"), (watcher_type, "test:watcher")] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: 0x524f_414d_4500_0001,
            })
            .unwrap();
    }
    let compatible: BTreeSet<_> = catalog
        .identities()
        .into_iter()
        .filter(|(kind, id, _, _)| *kind == b'S' && *id != 0)
        .map(|(_, id, _, _)| crate::content::BlockStateId(id))
        .collect();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:roamer".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: None,
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:watcher".into(),
        ownership: EntityOwnership::anchored(compatible, 1),
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(WatcherTick)),
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    let mut anchor = None;
    for y in (1..100).rev() {
        let key = world_to_chunk(0, y, 0).0;
        state.world.get_chunk(key).unwrap();
        if state.world.cached_block(0, y, 0) != Some(AIR) {
            anchor = Some(CellCoord::new(0, y, 0));
            break;
        }
    }
    let anchor = anchor.expect("terrain column has a non-air cell");
    state
        .world
        .get_chunk(world_to_chunk(anchor.x, anchor.y + 1, anchor.z).0)
        .unwrap();
    let resident = state
        .world
        .cached_block(anchor.x, anchor.y, anchor.z)
        .expect("chosen anchor cell is resident");
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Anchored {
            entity_type: watcher_type,
            anchor,
            anchor_state: resident,
            footprint: vec![anchor],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let watcher_id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    // The roamer lives in a neighbouring chunk that nothing has loaded, so
    // no fabricated empty entry may stand in for it.
    let roamer = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: roamer_type,
            position: [20.5, anchor.y as f32, 0.5],
            payload: EntityPayload::new(3u8),
            spawn_tick: 1,
        })
        .unwrap();
    let roamer_id = roamer.entity_id();
    state.entities.apply_committed(roamer).unwrap();

    let result = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: watcher_id },
        TickId::new(6),
    );
    assert!(
        matches!(result, Err(error) if error.kind() == ErrorKind::WouldBlock),
        "an unavailable neighbour must defer, not read empty"
    );
    assert_eq!(
        state.entities.snapshot(watcher_id).unwrap().next_tick,
        Some(6)
    );

    let mut action = None;
    for _ in 0..50 {
        match plan_durable_request(
            &mut state,
            &DurableRequest::EntityTick { id: watcher_id },
            TickId::new(6),
        ) {
            Ok(planned) => {
                action = planned;
                break;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                for _ in 0..2_000 {
                    crate::server::streaming::poll_chunk_loads(&mut state).unwrap();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            Err(error) => panic!("watcher tick must plan or defer, got {error:?}"),
        }
    }
    let action = action.expect("neighbour chunk becomes resident");
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();
    let snapshot = state.entities.snapshot(watcher_id).unwrap();
    let neighbours =
        entity::capture_entity_view_for_plan(&mut state, &snapshot.location, 1, watcher_id)
            .unwrap();
    assert!(neighbours.iter().any(|view| view.id == roamer_id));

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_neighbour_view_bound_rejects_one_entity_and_keeps_serving() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    let path = temp_save_dir("neighbour-bound-plan");
    let roamer_type = crate::content::EntityTypeId(70_009);
    let watcher_type = crate::content::EntityTypeId(70_006);
    let healthy_type = crate::content::EntityTypeId(70_014);
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key) in [
        (roamer_type, "test:roamer"),
        (watcher_type, "test:watcher"),
        (healthy_type, "test:healthy"),
    ] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: 0x4341_5054_4300_0001,
            })
            .unwrap();
    }
    let compatible: BTreeSet<_> = catalog
        .identities()
        .into_iter()
        .filter(|(kind, id, _, _)| *kind == b'S' && *id != 0)
        .map(|(_, id, _, _)| crate::content::BlockStateId(id))
        .collect();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:roamer".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: None,
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:watcher".into(),
        ownership: EntityOwnership::anchored(compatible, 1),
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(WatcherTick)),
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:healthy".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(CounterTick)),
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    // The healthy entity stages through the WAL so the checkpoint mirror
    // replays its tick commit; the watcher and its crowd are only ever read.
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    let healthy_id = stage_mobile_spawn(&mut state, healthy_type, [0.5, 80.0, 0.5], 7);
    let mut anchor = None;
    for y in (1..100).rev() {
        let key = world_to_chunk(0, y, 0).0;
        state.world.get_chunk(key).unwrap();
        if state.world.cached_block(0, y, 0) != Some(AIR) {
            anchor = Some(CellCoord::new(0, y, 0));
            break;
        }
    }
    let anchor = anchor.expect("terrain column has a non-air cell");
    let y = anchor.y;
    let resident = state
        .world
        .cached_block(anchor.x, anchor.y, anchor.z)
        .expect("chosen anchor cell is resident");
    // The watcher and its whole crowd stage through the WAL: direct spawns
    // would move the WAL-owned watermark behind the journal's back and break
    // the healthy commit this test drives next.
    let watcher_id = stage_entity_spawn(
        &mut state,
        EntitySpawn::Anchored {
            entity_type: watcher_type,
            anchor,
            anchor_state: resident,
            footprint: vec![anchor],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        },
    );
    // 40 roamers in each of two chunks inside the declared read set: every
    // page fits its per-chunk limit, but the assembled neighbour set is 80.
    // A capture that truncated to the 64-entry bound would plan happily.
    let mut crowd = Vec::with_capacity(80);
    for (base_x, count) in [(20.5f32, 40u16), (-4.5f32, 40u16)] {
        for index in 0..count {
            crowd.push(EntitySpawn::Mobile {
                entity_type: roamer_type,
                position: [base_x + f32::from(index) * 0.1, y as f32, 0.5],
                payload: EntityPayload::new(0u8),
                spawn_tick: 1,
            });
        }
    }
    stage_entity_spawn_batch(&mut state, crowd);

    // Drive the coordinator's own drain: while chunks load the over-cap
    // request defers, and once the set is resident only that ONE entity's
    // work is rejected — everything else keeps committing and the
    // coordinator never stops.
    let mut settled_healthy_at = None;
    for pass in 0..5_000 {
        crate::server::streaming::poll_chunk_loads(&mut state).unwrap();
        super::super::coordinator::queue_interaction_actions(&mut state, TickId::new(6));
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(6),
            Instant::now(),
        )
        .expect("a capacity condition must never stop the coordinator");
        if state.durability.pending.is_empty()
            && state.entities.snapshot(healthy_id).unwrap().revision == 2
        {
            if let Some(first) = settled_healthy_at {
                if pass - first > 20 {
                    break;
                }
            } else {
                settled_healthy_at = Some(pass);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        settled_healthy_at.is_some(),
        "unrelated work must progress past the over-cap entity"
    );
    // Fail per-entity, never global: admission stays open, other work
    // committed, and the unplannable tick left its schedule alone.
    assert!(!state.durability.failed);
    assert!(state.durability.pending.is_empty());
    assert_eq!(
        state
            .entities
            .snapshot(healthy_id)
            .unwrap()
            .private_payload
            .downcast_ref::<u8>(),
        Some(&8u8)
    );
    assert_eq!(state.entities.snapshot(watcher_id).unwrap().revision, 1);
    assert_eq!(
        state.entities.snapshot(watcher_id).unwrap().next_tick,
        Some(6)
    );
    assert!(state.entities.due_entities(7, 8).contains(&watcher_id));
    // The stable over-cap condition rejects as capacity, not corruption: no
    // truncation happened, and the coordinator is still serving. Missing
    // chunks still defer first; only the resident set must reject.
    let mut outcome = None;
    for _ in 0..500 {
        crate::server::streaming::poll_chunk_loads(&mut state).unwrap();
        match plan_durable_request(
            &mut state,
            &DurableRequest::EntityTick { id: watcher_id },
            TickId::new(6),
        ) {
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2));
            }
            result => {
                outcome = Some(
                    result
                        .as_ref()
                        .map(|_| "planned".to_owned())
                        .map_err(|error| (error.kind(), error.to_string())),
                );
                break;
            }
        }
    }
    let outcome = outcome.expect("the read set becomes resident");
    assert!(
        matches!(&outcome, Err((ErrorKind::QuotaExceeded, reason)) if reason.contains("count bound")),
        "expected the assembled-set bound to reject per-entity, got {outcome:?}"
    );
    assert!(!state.durability.failed);

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn coordinator_drain_preserves_deferred_entity_tick_until_commit() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    let path = temp_save_dir("neighbour-drain-commit");
    let roamer_type = crate::content::EntityTypeId(70_009);
    let watcher_type = crate::content::EntityTypeId(70_006);
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key) in [(roamer_type, "test:roamer"), (watcher_type, "test:watcher")] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: 0x4452_4149_4e00_0001,
            })
            .unwrap();
    }
    let compatible: BTreeSet<_> = catalog
        .identities()
        .into_iter()
        .filter(|(kind, id, _, _)| *kind == b'S' && *id != 0)
        .map(|(_, id, _, _)| crate::content::BlockStateId(id))
        .collect();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:roamer".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: None,
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:watcher".into(),
        ownership: EntityOwnership::anchored(compatible, 1),
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(WatcherTick)),
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    let mut anchor = None;
    for y in (1..100).rev() {
        let key = world_to_chunk(0, y, 0).0;
        state.world.get_chunk(key).unwrap();
        if state.world.cached_block(0, y, 0) != Some(AIR) {
            anchor = Some(CellCoord::new(0, y, 0));
            break;
        }
    }
    let anchor = anchor.expect("terrain column has a non-air cell");
    let y = anchor.y;
    let resident = state
        .world
        .cached_block(anchor.x, anchor.y, anchor.z)
        .expect("chosen anchor cell is resident");
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Anchored {
            entity_type: watcher_type,
            anchor,
            anchor_state: resident,
            footprint: vec![anchor],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let watcher_id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    // The neighbour's chunk is never preloaded, so the declared read set
    // misses on the first planning pass and the work must defer.
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: roamer_type,
            position: [20.5, y as f32, 0.5],
            payload: EntityPayload::new(3u8),
            spawn_tick: 1,
        })
        .unwrap();
    let roamer_id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();

    // The due tick enters through the coordinator's own queueing.
    super::super::coordinator::queue_interaction_actions(&mut state, TickId::new(6));
    assert!(
        state.durability.queued.iter().any(
            |request| matches!(request, DurableRequest::EntityTick { id } if *id == watcher_id)
        )
    );
    super::super::coordinator::process_durable_actions(&mut state, TickId::new(6), Instant::now())
        .unwrap();
    // The deferring pass must preserve the work: still queued, nothing
    // staged, schedule untouched, and the entity still due.
    assert!(
        state.durability.queued.iter().any(
            |request| matches!(request, DurableRequest::EntityTick { id } if *id == watcher_id)
        )
    );
    assert!(state.durability.pending.is_empty());
    assert_eq!(
        state.entities.snapshot(watcher_id).unwrap().next_tick,
        Some(6)
    );
    assert!(state.entities.due_entities(6, 8).contains(&watcher_id));

    // Keep draining the coordinator only. No queueing helper runs again: the
    // re-queued request must be retried and committed by the drain itself.
    for _ in 0..5_000 {
        crate::server::streaming::poll_chunk_loads(&mut state).unwrap();
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(6),
            Instant::now(),
        )
        .unwrap();
        if state.durability.queued.is_empty() && state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(state.durability.queued.is_empty());
    assert!(state.durability.pending.is_empty());
    assert!(!state.durability.failed);

    // Committed schedule: the planned due tick advanced and the due scan
    // reflects it. The payload proves a real commit, not just rescheduling.
    let snapshot = state.entities.snapshot(watcher_id).unwrap();
    assert_eq!(snapshot.next_tick, Some(11));
    let above = state
        .world
        .cached_block(anchor.x, anchor.y + 1, anchor.z)
        .unwrap();
    let expected = u8::from(state.world.catalog().block_flags(above) & crate::content::SOLID != 0);
    assert_eq!(
        snapshot.private_payload.downcast_ref::<u8>(),
        Some(&expected)
    );
    assert!(!state.entities.due_entities(6, 8).contains(&watcher_id));
    assert!(
        state
            .entities
            .due_tick_entries(11, None, 8)
            .contains(&(11, watcher_id))
    );
    let _ = roamer_id;

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

struct WideCodec;

impl crate::server::entities::EntityPayloadCodec for WideCodec {
    fn decode(
        &self,
        bytes: &[u8],
    ) -> Result<crate::server::entities::EntityPayload, crate::server::entities::EntityCodecError>
    {
        let [value] = bytes else {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        };
        Ok(crate::server::entities::EntityPayload::new(*value))
    }

    fn encode(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        payload
            .downcast_ref::<u8>()
            .copied()
            .map(|value| vec![value])
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)
    }

    fn public_view(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        // Each projection is the maximum public view; 20 neighbours exceed
        // the capture byte bound while staying under the count bound.
        payload
            .downcast_ref::<u8>()
            .map(|value| vec![*value; crate::server::entities::MAX_ENTITY_PUBLIC_VIEW_BYTES])
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)
    }
}

#[test]
fn entity_neighbour_view_byte_bound_is_an_error() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::sync::Arc;

    let path = temp_save_dir("neighbour-byte-bound");
    let wide_type = crate::content::EntityTypeId(70_012);
    let tick_type = crate::content::EntityTypeId(70_013);
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key) in [(wide_type, "test:wide"), (tick_type, "test:cap_tick")] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: 0x4259_5445_4200_0001,
            })
            .unwrap();
    }
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:wide".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(WideCodec),
        interaction_policy: None,
        tick_planner: None,
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:cap_tick".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(CounterTick)),
    });

    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: tick_type,
            position: [0.5, 80.0, 0.5],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap();
    let id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    for index in 0..20u16 {
        let spawn = state
            .entities
            .prepare_spawn(EntitySpawn::Mobile {
                entity_type: wide_type,
                position: [0.5 + f32::from(index) * 0.2, 80.0, 0.5],
                payload: EntityPayload::new(0u8),
                spawn_tick: 1,
            })
            .unwrap();
        state.entities.apply_committed(spawn).unwrap();
    }
    let result = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id },
        TickId::new(6),
    );
    assert!(
        matches!(result, Err(error) if error.kind() == ErrorKind::QuotaExceeded
            && error.to_string().contains("byte bound")),
        "the capture byte bound must reject per-entity as capacity, never truncate"
    );
    // A capacity rejection is per-entity: the coordinator stays up.
    assert!(!state.durability.failed);

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

/// Units of durable work for the poke-consumer probe below.
const POKE_DONE: u8 = 3;

/// A mobile consumer that performs a bounded amount of durable work (payload
/// 0 up to `POKE_DONE`), then idles on its schedule grid. Its next due time
/// always anchors to the previous persisted due time, so an early (woken)
/// tick does the same work sooner without rescheduling the grid. A woken
/// attempt with nothing to do reaffirms the schedule, which the coordinator
/// turns into a no-op instead of a commit.
struct PokeConsumer;

impl crate::server::entities::EntityTickPolicy for PokeConsumer {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        current_tick: u64,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        _neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{EntityError, EntityPayload, EntityTickPlan};
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        let phase: u8 = snapshot
            .private_payload
            .downcast_ref()
            .copied()
            .ok_or(EntityError::InvalidPayload)?;
        let (payload, next_tick) = if phase < POKE_DONE {
            (
                Some(EntityPayload::new(phase + 1)),
                due.checked_add(10).ok_or(EntityError::RevisionExhausted)?,
            )
        } else if current_tick < due {
            (None, due)
        } else {
            (
                None,
                due.checked_add(10).ok_or(EntityError::RevisionExhausted)?,
            )
        };
        Ok(EntityTickPlan {
            payload,
            next_tick,
            anchor_update: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
        })
    }
}

/// A mobile producer that wakes its consumer peer every tick until the peer's
/// public view shows the work is done. The destination comes from the
/// captured neighbour view, so emission is deterministic.
struct PokeProducer {
    consumer: crate::content::EntityTypeId,
}

impl crate::server::entities::EntityTickPolicy for PokeProducer {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        _current_tick: u64,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{EntityError, EntityTickPlan};
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        let peer = neighbours
            .iter()
            .filter(|view| view.entity_type == self.consumer)
            .min_by_key(|view| view.id)
            .ok_or(EntityError::InvalidType)?;
        let wakes = if peer.payload == vec![POKE_DONE] {
            Vec::new()
        } else {
            vec![peer.id]
        };
        Ok(EntityTickPlan {
            payload: None,
            next_tick: due.checked_add(5).ok_or(EntityError::RevisionExhausted)?,
            anchor_update: None,
            block_states: Vec::new(),
            wakes,
            transfer: None,
        })
    }
}

/// A mobile probe whose planner wakes an entity ID that is not in its
/// neighbour view. Planning must reject the whole plan without failing.
struct PokeBlind;

impl crate::server::entities::EntityTickPolicy for PokeBlind {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        _current_tick: u64,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        _neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{EntityError, EntityTickPlan};
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        Ok(EntityTickPlan {
            payload: None,
            next_tick: due.checked_add(5).ok_or(EntityError::RevisionExhausted)?,
            anchor_update: None,
            block_states: Vec::new(),
            wakes: vec![crate::server::entities::EntityId::new(999_999).unwrap()],
            transfer: None,
        })
    }
}

fn poke_catalog(
    producer: crate::content::EntityTypeId,
    consumer: crate::content::EntityTypeId,
) -> crate::content::Catalog {
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key, fingerprint) in [
        (producer, "test:poke_producer", 0x504f_4b45_5000_0001),
        (consumer, "test:poke_consumer", 0x504f_4b45_4300_0001),
    ] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: fingerprint,
            })
            .unwrap();
    }
    catalog
}

fn poke_startup(
    catalog: crate::content::Catalog,
    consumer: crate::content::EntityTypeId,
    consumer_interval: u32,
) -> crate::server::startup::ServerStartup {
    use crate::server::entities::{EntityOwnership, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::sync::Arc;

    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:poke_producer".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(PokeProducer { consumer })
            as Arc<dyn crate::server::entities::EntityTickPolicy>),
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:poke_consumer".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(consumer_interval),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(
            Arc::new(PokeConsumer) as Arc<dyn crate::server::entities::EntityTickPolicy>
        ),
    });
    startup
}

fn stage_entity_spawn(
    state: &mut State,
    spawn: crate::server::entities::EntitySpawn,
) -> crate::server::entities::EntityId {
    // Spawns stage through the WAL so the entity checkpoint mirror replays
    // the same batches as the live store. Direct `apply_committed` spawns
    // would desync the mirror on the first submitted tick commit.
    let prepared = state.entities.prepare_spawn(spawn).unwrap();
    let id = prepared.entity_id();
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the spawn");
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(prepared),
        entity_wakes: Vec::new(),
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None, Some(permit))
            .unwrap()
    );
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(state, TickId::new(1), Instant::now())
            .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        state.durability.pending.is_empty(),
        "poke spawn must commit"
    );
    id
}

fn stage_mobile_spawn(
    state: &mut State,
    entity_type: crate::content::EntityTypeId,
    position: [f32; 3],
    payload: u8,
) -> crate::server::entities::EntityId {
    use crate::server::entities::{EntityPayload, EntitySpawn};

    stage_entity_spawn(
        state,
        EntitySpawn::Mobile {
            entity_type,
            position,
            payload: EntityPayload::new(payload),
            spawn_tick: 1,
        },
    )
}

/// Stages a whole group of spawns as one WAL batch so the live store and
/// the checkpoint mirror advance together. Direct `apply_committed` spawns
/// would move the WAL-owned watermark behind the journal's back and break
/// every later staged commit.
fn stage_entity_spawn_batch(
    state: &mut State,
    spawns: Vec<crate::server::entities::EntitySpawn>,
) -> Vec<crate::server::entities::EntityId> {
    let prepared = state.entities.prepare_spawn_batch(spawns).unwrap();
    let ids = prepared.entity_ids();
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the spawn batch");
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(prepared),
        entity_wakes: Vec::new(),
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None, Some(permit))
            .unwrap()
    );
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(state, TickId::new(1), Instant::now())
            .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        state.durability.pending.is_empty(),
        "spawn batch must commit"
    );
    ids
}

fn spawn_poke_pair(
    state: &mut State,
    producer: crate::content::EntityTypeId,
    consumer: crate::content::EntityTypeId,
) -> (
    crate::server::entities::EntityId,
    crate::server::entities::EntityId,
) {
    let producer_id = stage_mobile_spawn(state, producer, [0.5, 80.0, 0.5], 0);
    let consumer_id = stage_mobile_spawn(state, consumer, [2.5, 80.0, 0.5], 0);
    (producer_id, consumer_id)
}

fn poke_consumer_phase(state: &State, consumer: crate::server::entities::EntityId) -> u8 {
    state
        .entities
        .snapshot(consumer)
        .unwrap()
        .private_payload
        .downcast_ref::<u8>()
        .copied()
        .unwrap()
}

fn drive_poke_tick(state: &mut State, tick: u64, drop_wakes: bool) {
    super::super::coordinator::queue_interaction_actions(state, TickId::new(tick));
    super::super::coordinator::process_durable_actions(state, TickId::new(tick), Instant::now())
        .unwrap();
    if drop_wakes {
        // Drop every effect before delivery: the destination stays on its
        // persisted schedule and must converge to the same work, only later.
        state.durability.pending_wakes.clear();
        state
            .durability
            .queued
            .retain(|request| !matches!(request, DurableRequest::EntityWake { .. }));
    }
    // Settle at the same tick until every staged receipt applies. Repeating
    // the tick absorbs WAL latency without opening new dues, so each tick's
    // durable outcome is exact before the clock advances.
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(
            state,
            TickId::new(tick),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        state.durability.pending.is_empty(),
        "poke tick {tick} must settle"
    );
    if drop_wakes {
        state.durability.pending_wakes.clear();
        state
            .durability
            .queued
            .retain(|request| !matches!(request, DurableRequest::EntityWake { .. }));
    }
}

#[test]
fn entity_wake_effects_are_optional_for_convergence() {
    let producer_type = crate::content::EntityTypeId(70_010);
    let consumer_type = crate::content::EntityTypeId(70_011);
    // The consumer's persisted dues below this tick are all committed by both
    // runs; the next grid point stays unopened so the tail cannot diverge.
    const LAST_TICK: u64 = 148;

    let mut outcomes = Vec::new();
    for drop_wakes in [false, true] {
        let label = if drop_wakes {
            "poke-converge-dropped"
        } else {
            "poke-converge-delivered"
        };
        let path = temp_save_dir(label);
        let catalog = poke_catalog(producer_type, consumer_type);
        let startup = poke_startup(catalog, consumer_type, 10);
        let mut state =
            crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
        let (producer_id, consumer_id) = spawn_poke_pair(&mut state, producer_type, consumer_type);
        let mut done_tick = None;
        for tick in 1..=LAST_TICK {
            drive_poke_tick(&mut state, tick, drop_wakes);
            if done_tick.is_none() && poke_consumer_phase(&state, consumer_id) == POKE_DONE {
                done_tick = Some(tick);
            }
        }
        // Settle at the final tick without opening new dues: receipts apply
        // and leftover wakes deliver (or are dropped) on the same schedule
        // grid, so both runs must land on identical durable state.
        for _ in 0..2_000 {
            drive_poke_tick(&mut state, LAST_TICK, drop_wakes);
            let wakes_pending = !state.durability.pending_wakes.is_empty()
                || state.durability.queued.iter().any(|request| {
                    matches!(
                        request,
                        DurableRequest::EntityTick { .. } | DurableRequest::EntityWake { .. }
                    )
                });
            if state.durability.pending.is_empty() && !wakes_pending {
                break;
            }
        }
        assert!(
            state.durability.pending.is_empty(),
            "poke scenario must settle"
        );
        let consumer = state.entities.snapshot(consumer_id).unwrap();
        let producer = state.entities.snapshot(producer_id).unwrap();
        outcomes.push((
            consumer.private_payload.downcast_ref::<u8>().copied(),
            consumer.next_tick,
            consumer.revision,
            consumer.owner,
            consumer.location,
            producer.next_tick,
            producer.revision,
            done_tick,
        ));
        drop(state);
        fs::remove_dir_all(path).unwrap();
    }

    let [delivered, dropped] = outcomes.as_slice() else {
        panic!("both poke scenarios must run");
    };
    // The proof of the notification rule: identical final durable state
    // whether every wake was delivered or every wake was dropped.
    assert_eq!(delivered.0, Some(POKE_DONE));
    assert_eq!(delivered.0, dropped.0, "consumer work must converge");
    assert_eq!(delivered.1, dropped.1, "consumer schedule must converge");
    assert_eq!(delivered.2, dropped.2, "consumer revisions must converge");
    assert_eq!(delivered.3, dropped.3, "consumer owner must converge");
    assert_eq!(delivered.4, dropped.4, "consumer location must converge");
    assert_eq!(delivered.5, dropped.5, "producer schedule must converge");
    assert_eq!(delivered.6, dropped.6, "producer revisions must converge");
    // ... only later without the wakes.
    assert!(
        delivered.7.unwrap() < dropped.7.unwrap(),
        "delivered wakes must finish the same work sooner: {outcomes:?}"
    );
}

#[test]
fn entity_wake_delivery_runs_the_destination_next_tick_never_same_tick() {
    let producer_type = crate::content::EntityTypeId(70_010);
    let consumer_type = crate::content::EntityTypeId(70_011);
    let path = temp_save_dir("poke-no-cascade");
    let catalog = poke_catalog(producer_type, consumer_type);
    // The consumer's own grid is far away: any work it does comes from wakes.
    let startup = poke_startup(catalog, consumer_type, 100);
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    let (_producer_id, consumer_id) = spawn_poke_pair(&mut state, producer_type, consumer_type);

    for tick in 1..=6u64 {
        drive_poke_tick(&mut state, tick, false);
    }
    // The producer's tick-6 commit applied during its own tick, and delivery
    // recorded the wake without queueing any destination work in the same pass.
    assert!(state.durability.pending.is_empty());
    assert_eq!(state.durability.pending_wakes, vec![consumer_id]);
    assert!(
        !state
            .durability
            .queued
            .iter()
            .any(|request| matches!(request, DurableRequest::EntityWake { .. })),
        "a tick-N delivery must not plan its destination in tick N"
    );
    assert_eq!(state.entities.snapshot(consumer_id).unwrap().revision, 1);

    // The interaction/commit barrier queues the wake; the next tick plans it,
    // far ahead of the consumer's own persisted due time.
    super::super::coordinator::queue_interaction_actions(&mut state, TickId::new(6));
    assert!(
        state.durability.queued.iter().any(
            |request| matches!(request, DurableRequest::EntityWake { id } if *id == consumer_id)
        )
    );
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(7),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(poke_consumer_phase(&state, consumer_id), 1);
    assert_eq!(state.entities.snapshot(consumer_id).unwrap().revision, 2);

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_wake_to_an_unseen_entity_rejects_the_plan_without_failing() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::sync::Arc;

    let path = temp_save_dir("poke-blind-wake");
    let blind_type = crate::content::EntityTypeId(70_012);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: blind_type,
            key: "test:poke_blind".into(),
            schema_version: 1,
            schema_fingerprint: 0x504f_4b45_4200_0001,
        })
        .unwrap();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:poke_blind".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(PokeBlind)),
    });
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: blind_type,
            position: [0.5, 80.0, 0.5],
            payload: EntityPayload::new(0u8),
            spawn_tick: 1,
        })
        .unwrap();
    let id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();

    let result = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id },
        TickId::new(6),
    );
    assert!(
        matches!(result, Err(error) if error.kind() == ErrorKind::InvalidInput),
        "a wake outside the neighbour view must reject the plan"
    );
    // A rejected effect set never takes the coordinator-fatal path.
    assert!(!state.durability.failed);
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, Some(6));
    assert!(state.entities.due_entities(7, 8).contains(&id));

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_wake_with_nothing_to_do_reaffirms_instead_of_committing() {
    let consumer_type = crate::content::EntityTypeId(70_011);
    let path = temp_save_dir("poke-reaffirm");
    let catalog = poke_catalog(crate::content::EntityTypeId(70_010), consumer_type);
    let startup = poke_startup(catalog, consumer_type, 100);
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    // Already done and far from due: a woken attempt must no-op, while the
    // due attempt still advances the schedule.
    let consumer_id = stage_mobile_spawn(&mut state, consumer_type, [2.5, 80.0, 0.5], POKE_DONE);
    assert_eq!(
        state.entities.snapshot(consumer_id).unwrap().next_tick,
        Some(101)
    );

    state
        .durability
        .queued
        .push_back(DurableRequest::EntityWake { id: consumer_id });
    super::super::coordinator::process_durable_actions(&mut state, TickId::new(50), Instant::now())
        .unwrap();
    let snapshot = state.entities.snapshot(consumer_id).unwrap();
    assert_eq!(snapshot.revision, 1, "a reaffirmed wake must not commit");
    assert_eq!(snapshot.next_tick, Some(101));
    assert!(state.durability.pending.is_empty());

    state
        .durability
        .queued
        .push_back(DurableRequest::EntityTick { id: consumer_id });
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(101),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let snapshot = state.entities.snapshot(consumer_id).unwrap();
    assert_eq!(
        snapshot.revision, 2,
        "the due tick still advances the schedule"
    );
    assert_eq!(snapshot.next_tick, Some(111));

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

/// An anchored interaction probe that wakes its mobile consumer peer, like
/// `CounterInteract` but with the notification channel attached.
struct PokeWaker {
    consumer: crate::content::EntityTypeId,
}

impl crate::server::entities::EntityInteractionPolicy for PokeWaker {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        _request: &[u8],
        inventory: &crate::inventory::Inventory,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityInteractionPlan, crate::server::entities::EntityError>
    {
        use crate::server::entities::{EntityBlockStateChange, EntityError, EntityInteractionPlan};
        let (anchor, anchor_state) = match &snapshot.location {
            crate::server::entities::EntityLocation::Anchored {
                anchor,
                anchor_state,
                ..
            } => (*anchor, *anchor_state),
            crate::server::entities::EntityLocation::Mobile { .. } => {
                return Err(EntityError::WrongOwnership);
            }
        };
        let peer = neighbours
            .iter()
            .filter(|view| view.entity_type == self.consumer)
            .min_by_key(|view| view.id)
            .ok_or(EntityError::InvalidType)?;
        let mut next = inventory.clone();
        next.revision = next.revision.wrapping_add(1);
        Ok(EntityInteractionPlan {
            payload: crate::server::entities::EntityPayload::new(9u8),
            inventory: next,
            block_states: vec![EntityBlockStateChange {
                cell: anchor,
                before: anchor_state,
                after: anchor_state,
            }],
            wakes: if peer.payload == vec![POKE_DONE] {
                Vec::new()
            } else {
                vec![peer.id]
            },
        })
    }
}

#[test]
fn entity_interaction_wakes_route_to_next_tick_delivery() {
    use crate::server::entities::{EntityOwnership, EntityPayload, EntitySpawn, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    let path = temp_save_dir("poke-interact-wake");
    let waker_type = crate::content::EntityTypeId(70_013);
    let consumer_type = crate::content::EntityTypeId(70_011);
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key, fingerprint) in [
        (waker_type, "test:poke_waker", 0x504f_4b45_5700_0001),
        (consumer_type, "test:poke_consumer", 0x504f_4b45_4300_0001),
    ] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: fingerprint,
            })
            .unwrap();
    }
    let compatible: BTreeSet<_> = catalog
        .identities()
        .into_iter()
        .filter(|(kind, id, _, _)| *kind == b'S' && *id != 0)
        .map(|(_, id, _, _)| crate::content::BlockStateId(id))
        .collect();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:poke_waker".into(),
        ownership: EntityOwnership::anchored(compatible, 1),
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: Some(Arc::new(PokeWaker {
            consumer: consumer_type,
        })),
        tick_planner: None,
    });
    startup.register_entity_type(StartupEntityType {
        key: "test:poke_consumer".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(100),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(PokeConsumer)),
    });
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();

    let mut anchor = None;
    for y in (1..100).rev() {
        let key = world_to_chunk(0, y, 0).0;
        state.world.get_chunk(key).unwrap();
        if state.world.cached_block(0, y, 0) != Some(AIR) {
            anchor = Some(CellCoord::new(0, y, 0));
            break;
        }
    }
    let anchor = anchor.expect("terrain column has a non-air cell");
    let resident = state
        .world
        .cached_block(anchor.x, anchor.y, anchor.z)
        .expect("chosen anchor cell is resident");
    // The consumer shares the anchor chunk, so the radius-zero neighbour
    // view already covers it with no loader round-trips.
    let consumer_id = stage_mobile_spawn(&mut state, consumer_type, [3.5, anchor.y as f32, 3.5], 0);
    let waker_id = stage_entity_spawn(
        &mut state,
        EntitySpawn::Anchored {
            entity_type: waker_type,
            anchor,
            anchor_state: resident,
            footprint: vec![anchor],
            payload: EntityPayload::new(0u8),
            spawn_tick: 1,
        },
    );
    assert!(state.entities.due_entities(200, 8).contains(&consumer_id));

    let peer = add_test_client(
        &mut state,
        [0.5, anchor.y as f32, 3.5],
        Inventory::default(),
    );
    let epoch = grant_action_epoch(&mut state, 17);
    settle_live_action(
        &mut state,
        10,
        ClientMessage::EntityInteract {
            action_id: (u128::from(epoch) << 64) | 1,
            target: [anchor.x, anchor.y, anchor.z],
            payload: vec![0],
        },
    );
    assert_eq!(
        state
            .entities
            .snapshot(waker_id)
            .unwrap()
            .private_payload
            .downcast_ref::<u8>(),
        Some(&9u8)
    );
    // The interaction committed its durable work and recorded the wake
    // without running the destination in the same pass.
    assert_eq!(state.durability.pending_wakes, vec![consumer_id]);
    assert_eq!(state.entities.snapshot(consumer_id).unwrap().revision, 1);

    super::super::coordinator::queue_interaction_actions(&mut state, TickId::new(10));
    drive_poke_tick(&mut state, 11, false);
    assert_eq!(poke_consumer_phase(&state, consumer_id), 1);
    assert_eq!(state.entities.snapshot(consumer_id).unwrap().revision, 2);

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

/// Probe item-bin payload for transfer tests: one stack, no components.
/// Counts stay inside the 128-block cap; zero is an empty bin that keeps its
/// item key so a later deposit can adopt it.
#[derive(Clone, Debug, Eq, PartialEq)]
struct BinPayload {
    item: crate::items::ItemId,
    count: u16,
}

struct BinCodec;

impl crate::server::entities::EntityPayloadCodec for BinCodec {
    fn decode(
        &self,
        bytes: &[u8],
    ) -> Result<crate::server::entities::EntityPayload, crate::server::entities::EntityCodecError>
    {
        if bytes.len() != 6 {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        }
        let item = crate::items::ItemId(u32::from_le_bytes(bytes[0..4].try_into().unwrap()));
        let count = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        if item.0 == 0 || count > crate::inventory::STACK_LIMIT {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        }
        Ok(crate::server::entities::EntityPayload::new(BinPayload {
            item,
            count,
        }))
    }

    fn encode(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        let bin = payload
            .downcast_ref::<BinPayload>()
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)?;
        if bin.item.0 == 0 || bin.count > crate::inventory::STACK_LIMIT {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        }
        let mut bytes = Vec::with_capacity(6);
        bytes.extend(bin.item.0.to_le_bytes());
        bytes.extend(bin.count.to_le_bytes());
        Ok(bytes)
    }

    fn public_view(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        // The full stock is public: planners branch on neighbour counts, and
        // the trusted layer never consults another entity's private payload.
        self.encode(payload)
    }
}

/// Pure exchange hooks for the bin probe. Withdraw removes exactly the
/// requested count and returns the taken stack; deposit adds the whole stack
/// or refuses it whole. Both are deterministic functions of their inputs.
struct BinExchange;

impl crate::server::entities::EntityTransferPolicy for BinExchange {
    fn withdraw(
        &self,
        payload: &crate::server::entities::EntityPayload,
        item: crate::items::ItemId,
        count: u16,
        catalog: &crate::content::Catalog,
    ) -> Result<
        Option<(
            crate::server::entities::EntityPayload,
            crate::inventory::Stack,
        )>,
        crate::server::entities::EntityError,
    > {
        use crate::server::entities::{EntityError, EntityPayload};
        let bin = payload
            .downcast_ref::<BinPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        if count == 0 || count > crate::inventory::STACK_LIMIT || catalog.item(item).is_none() {
            return Err(EntityError::InvalidPayload);
        }
        if bin.item != item || bin.count < count {
            return Ok(None);
        }
        let taken = crate::inventory::Stack::new(item, count);
        if !taken.valid_in(catalog) {
            return Err(EntityError::InvalidPayload);
        }
        let mut after = bin.clone();
        after.count -= count;
        Ok(Some((EntityPayload::new(after), taken)))
    }

    fn deposit(
        &self,
        payload: &crate::server::entities::EntityPayload,
        stack: &crate::inventory::Stack,
        catalog: &crate::content::Catalog,
    ) -> Result<Option<crate::server::entities::EntityPayload>, crate::server::entities::EntityError>
    {
        use crate::server::entities::{EntityError, EntityPayload};
        let bin = payload
            .downcast_ref::<BinPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        if !stack.valid_in(catalog) {
            return Err(EntityError::InvalidPayload);
        }
        let mut after = bin.clone();
        if after.count == 0 {
            after.item = stack.item;
        }
        if after.item != stack.item {
            return Ok(None);
        }
        let total = u32::from(after.count) + u32::from(stack.count);
        if total > u32::from(crate::inventory::STACK_LIMIT) {
            return Ok(None);
        }
        after.count = total as u16;
        Ok(Some(EntityPayload::new(after)))
    }
}

/// A mobile receiver that pulls a fixed count from a visible bin peer while
/// its own stock is below target. It plans from public projections only: the
/// source count comes from the neighbour view, never from private state.
/// `blind_source` bypasses discovery to declare a pull the plan cannot see,
/// which the trusted layer must reject.
struct BinPullTick {
    source_type: crate::content::EntityTypeId,
    item: crate::items::ItemId,
    count: u16,
    target: u16,
    blind_source: Option<crate::server::entities::EntityId>,
}

impl crate::server::entities::EntityTickPolicy for BinPullTick {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        current_tick: u64,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{EntityError, EntityItemTransfer, EntityTickPlan};
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        if current_tick < due {
            return Err(EntityError::InvalidType);
        }
        let own = snapshot
            .private_payload
            .downcast_ref::<BinPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        let next_tick = due.checked_add(5).ok_or(EntityError::RevisionExhausted)?;
        if let Some(source) = self.blind_source {
            return Ok(EntityTickPlan {
                payload: None,
                next_tick,
                anchor_update: None,
                block_states: Vec::new(),
                wakes: Vec::new(),
                transfer: Some(EntityItemTransfer {
                    source,
                    item: self.item,
                    count: self.count,
                }),
            });
        }
        let peer = neighbours
            .iter()
            .filter(|view| view.entity_type == self.source_type)
            .min_by_key(|view| view.id);
        let source_count = peer
            .and_then(|view| {
                let bytes = view.payload.as_slice();
                let item_bytes: [u8; 4] = bytes.get(0..4)?.try_into().ok()?;
                let count_bytes: [u8; 2] = bytes.get(4..6)?.try_into().ok()?;
                (bytes.len() == 6
                    && crate::items::ItemId(u32::from_le_bytes(item_bytes)) == self.item)
                    .then(|| u16::from_le_bytes(count_bytes))
            })
            .unwrap_or(0);
        let transfer =
            (own.count < self.target && source_count >= self.count).then(|| EntityItemTransfer {
                source: peer.expect("source has stock, so a peer is visible").id,
                item: self.item,
                count: self.count,
            });
        // The payload update itself is empty: the deposit computes the
        // receiver's after-payload from this snapshot inside the trusted
        // layer, so the planner never touches another entity's state.
        Ok(EntityTickPlan {
            payload: None,
            next_tick,
            anchor_update: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer,
        })
    }
}

fn bin_catalog(
    source: crate::content::EntityTypeId,
    sink: crate::content::EntityTypeId,
) -> crate::content::Catalog {
    let mut catalog = crate::content::Catalog::builtins();
    for (id, key, fingerprint) in [
        (source, "test:bin_source", 0x4249_4e53_5200_0001),
        (sink, "test:bin_sink", 0x4249_4e53_4b00_0001),
    ] {
        catalog
            .register_entity_type(crate::content::EntityTypeDef {
                id,
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: fingerprint,
            })
            .unwrap();
    }
    catalog
}

fn bin_startup(
    catalog: crate::content::Catalog,
    source: crate::content::EntityTypeId,
    sink: crate::content::EntityTypeId,
    blind_source: Option<crate::server::entities::EntityId>,
    target: u16,
) -> crate::server::startup::ServerStartup {
    use crate::server::entities::{EntityOwnership, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::sync::Arc;

    let key_of = |id| {
        if id == source {
            "test:bin_source".to_owned()
        } else {
            "test:bin_sink".to_owned()
        }
    };
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    // The source is passive: no schedule, only exchange hooks. It never
    // deducts speculatively; stock leaves only inside the receiver's batch.
    startup.register_entity_type(StartupEntityType {
        key: key_of(source),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 6,
        codec: Arc::new(BinCodec),
        interaction_policy: None,
        tick_planner: None,
    });
    startup.register_entity_transfer_policy(
        key_of(source),
        Arc::new(BinExchange) as Arc<dyn crate::server::entities::EntityTransferPolicy>,
    );
    startup.register_entity_type(StartupEntityType {
        key: key_of(sink),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 6,
        codec: Arc::new(BinCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(BinPullTick {
            source_type: source,
            item: STICK,
            count: 30,
            target,
            blind_source,
        })
            as Arc<dyn crate::server::entities::EntityTickPolicy>),
    });
    startup.register_entity_transfer_policy(
        key_of(sink),
        Arc::new(BinExchange) as Arc<dyn crate::server::entities::EntityTransferPolicy>,
    );
    startup
}

fn bin_count(state: &State, id: crate::server::entities::EntityId) -> u16 {
    state
        .entities
        .snapshot(id)
        .unwrap()
        .private_payload
        .downcast_ref::<BinPayload>()
        .unwrap()
        .count
}

fn stage_bin(
    state: &mut State,
    entity_type: crate::content::EntityTypeId,
    position: [f32; 3],
    count: u16,
) -> crate::server::entities::EntityId {
    use crate::server::entities::{EntityPayload, EntitySpawn};

    stage_entity_spawn(
        state,
        EntitySpawn::Mobile {
            entity_type,
            position,
            payload: EntityPayload::new(BinPayload { item: STICK, count }),
            spawn_tick: 1,
        },
    )
}

/// Stages one prepared entity payload update through the WAL so the live
/// store and the checkpoint mirror advance together.
fn stage_entity_update(
    state: &mut State,
    id: crate::server::entities::EntityId,
    patch: crate::server::entities::EntityPatch,
    tick: u64,
) {
    let snapshot = state.entities.snapshot(id).unwrap();
    let prepared = state
        .entities
        .prepare_update(id, snapshot.revision, patch)
        .unwrap();
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the update");
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(prepared),
        entity_wakes: Vec::new(),
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), &action, None, Some(permit))
            .unwrap()
    );
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(
            state,
            TickId::new(tick),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(state.durability.pending.is_empty(), "update must commit");
}

fn settle_commit_action(state: &mut State, action: &CommitAction, tick: u64) {
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the transfer");
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), action, None, Some(permit))
            .unwrap()
    );
    for _ in 0..2_000 {
        super::super::coordinator::process_durable_actions(
            state,
            TickId::new(tick),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(state.durability.pending.is_empty(), "transfer must commit");
}

#[test]
fn entity_tick_transfer_moves_items_atomically_and_conserves() {
    let source_type = crate::content::EntityTypeId(71_101);
    let sink_type = crate::content::EntityTypeId(71_102);
    let path = temp_save_dir("bin-transfer-commit");
    let catalog = bin_catalog(source_type, sink_type);
    let startup = bin_startup(catalog, source_type, sink_type, None, 40);
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    let source_id = stage_bin(&mut state, source_type, [0.5, 80.0, 0.5], 100);
    let sink_id = stage_bin(&mut state, sink_type, [2.5, 80.0, 0.5], 10);
    assert_eq!(
        bin_count(&state, source_id) + bin_count(&state, sink_id),
        110
    );

    // The plan holds both ends in one prepared transaction: both entity IDs,
    // both entity keys, and exactly one revision watermark — one WAL record.
    let planned = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: sink_id },
        TickId::new(6),
    )
    .unwrap()
    .expect("due pull plans a transfer");
    let batch = planned.entities.as_ref().expect("transfer stages entities");
    let mut ids = batch.entity_ids();
    ids.sort_unstable();
    let mut expected = vec![source_id, sink_id];
    expected.sort_unstable();
    assert_eq!(ids, expected, "one transaction covers both entities");
    assert_eq!(
        batch
            .changes()
            .iter()
            .filter(|change| change.key.domain == crate::server::entities::ENTITY_REVISION_DOMAIN)
            .count(),
        1,
        "one WAL record carries the whole move"
    );
    state.entities.validate_prepared(batch).unwrap();
    // Planning stages nothing: both payloads are untouched before the receipt.
    assert_eq!(bin_count(&state, source_id), 100);
    assert_eq!(bin_count(&state, sink_id), 10);

    // The commit applies both ends together through the real coordinator.
    drive_poke_tick(&mut state, 6, false);
    assert!(!state.durability.failed);
    assert_eq!(bin_count(&state, source_id), 70);
    assert_eq!(bin_count(&state, sink_id), 40);
    assert_eq!(
        bin_count(&state, source_id) + bin_count(&state, sink_id),
        110,
        "items are neither created nor destroyed"
    );
    // The sender's schedule is untouched by the receiver-triggered pull.
    assert_eq!(state.entities.snapshot(source_id).unwrap().next_tick, None);
    assert_eq!(
        state.entities.snapshot(sink_id).unwrap().next_tick,
        Some(11)
    );

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_tick_transfer_rejects_whole_on_stale_preimage_and_replans() {
    use crate::server::entities::EntityPatch;

    let source_type = crate::content::EntityTypeId(71_101);
    let sink_type = crate::content::EntityTypeId(71_102);
    let path = temp_save_dir("bin-transfer-stale-retry");
    let catalog = bin_catalog(source_type, sink_type);
    let startup = bin_startup(catalog, source_type, sink_type, None, 40);
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    let source_id = stage_bin(&mut state, source_type, [0.5, 80.0, 0.5], 100);
    let sink_id = stage_bin(&mut state, sink_type, [2.5, 80.0, 0.5], 10);

    // Plan the pull, then advance the recipient's revision through the WAL
    // (a schedule-only update: stock is untouched) so the planned `before`
    // goes stale before it can stage.
    let planned = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: sink_id },
        TickId::new(6),
    )
    .unwrap()
    .expect("due pull plans a transfer");
    stage_entity_update(
        &mut state,
        sink_id,
        EntityPatch {
            payload: None,
            next_tick: Some(Some(11)),
            position: None,
        },
        6,
    );
    assert_eq!(bin_count(&state, source_id), 100);
    assert_eq!(bin_count(&state, sink_id), 10);

    // The whole transaction is rejected: the stale `before` fails validation
    // and nothing moved.
    let stale = planned.entities.as_ref().expect("transfer stages entities");
    assert!(
        matches!(
            state.entities.validate_prepared(stale),
            Err(crate::server::entities::EntityError::InvalidTransaction
                | crate::server::entities::EntityError::StaleRevision { .. })
        ),
        "a stale recipient preimage must reject the whole batch"
    );
    assert_eq!(bin_count(&state, source_id), 100);
    assert_eq!(bin_count(&state, sink_id), 10);
    assert_eq!(
        bin_count(&state, source_id) + bin_count(&state, sink_id),
        110
    );

    // The work re-plans against the fresh revision and eventually commits.
    let retry = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: sink_id },
        TickId::new(11),
    )
    .unwrap()
    .expect("stale work re-plans");
    state
        .entities
        .validate_prepared(retry.entities.as_ref().unwrap())
        .unwrap();
    settle_commit_action(&mut state, &retry, 11);
    assert!(!state.durability.failed);
    assert_eq!(bin_count(&state, source_id), 70);
    assert_eq!(bin_count(&state, sink_id), 40);
    assert_eq!(
        bin_count(&state, source_id) + bin_count(&state, sink_id),
        110,
        "total item count is identical across success, rejection, and retry"
    );

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_tick_transfer_full_destination_defers_without_partial_apply() {
    let source_type = crate::content::EntityTypeId(71_101);
    let sink_type = crate::content::EntityTypeId(71_102);
    let path = temp_save_dir("bin-transfer-full-defer");
    let catalog = bin_catalog(source_type, sink_type);
    let startup = bin_startup(catalog, source_type, sink_type, None, 128);
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    // 120 + 30 would exceed the 128 cap, so the pull must defer whole.
    let source_id = stage_bin(&mut state, source_type, [0.5, 80.0, 0.5], 100);
    let sink_id = stage_bin(&mut state, sink_type, [2.5, 80.0, 0.5], 120);

    let result = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: sink_id },
        TickId::new(6),
    );
    assert!(
        matches!(result, Err(error) if error.kind() == ErrorKind::WouldBlock),
        "a non-fitting transfer defers instead of half-applying"
    );
    assert!(!state.durability.failed);
    assert_eq!(bin_count(&state, source_id), 100);
    assert_eq!(bin_count(&state, sink_id), 120);
    assert_eq!(
        state.entities.snapshot(sink_id).unwrap().next_tick,
        Some(6),
        "deferred work keeps its schedule"
    );

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn entity_tick_transfer_source_outside_view_rejects_the_plan() {
    let source_type = crate::content::EntityTypeId(71_101);
    let sink_type = crate::content::EntityTypeId(71_102);
    let path = temp_save_dir("bin-transfer-undeclared");
    let catalog = bin_catalog(source_type, sink_type);
    // The planner names a source its captured view cannot see. The trusted
    // layer must reject the write it did not declare, without failing.
    let distant = crate::server::entities::EntityId::new(1).unwrap();
    let startup = bin_startup(catalog, source_type, sink_type, Some(distant), 40);
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    // Far outside the receiver's radius-0 capture: resident or not, the
    // planner's declared view cannot see it.
    let source_id = stage_bin(&mut state, source_type, [512.5, 80.0, 0.5], 100);
    let sink_id = stage_bin(&mut state, sink_type, [2.5, 80.0, 0.5], 10);
    assert_eq!(source_id, distant);

    let result = plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id: sink_id },
        TickId::new(6),
    );
    assert!(
        matches!(result, Err(error) if error.kind() == ErrorKind::InvalidInput),
        "a policy cannot cause a write it did not declare"
    );
    assert!(!state.durability.failed);
    assert_eq!(state.entities.snapshot(sink_id).unwrap().revision, 1);
    assert_eq!(state.entities.snapshot(source_id).unwrap().revision, 1);
    assert_eq!(bin_count(&state, source_id), 100);
    assert_eq!(bin_count(&state, sink_id), 10);

    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn chunk_request_failure_defers_without_verifying_any_preimage() {
    use crate::world::STONE;

    let path = temp_save_dir("edit-request-failure-defer");
    let mut state = server_state(23, path.clone()).unwrap();
    // One cached chunk: the client's own. The edit target sits just across
    // the chunk boundary, within reach but never loaded.
    state.world.reset_cache_for_test(1);
    state.world.get_chunk(world_to_chunk(15, 80, 0).0).unwrap();
    let peer = add_test_client(&mut state, [15.5, 80.0, 0.5], Inventory::default());
    // The edit target was never loaded, and the loader is stopped, so the
    // chunk request itself fails. The edit must defer for retry — not fail
    // the work, and never commit with an unverified preimage.
    assert!(state.world.cached_block(16, 80, 0).is_none());
    state.loader.stop_for_test();
    let request = edit_request(ClientMessage::Edit {
        action_id: 1,
        x: 16,
        y: 80,
        z: 0,
        block: STONE,
        slot: 0,
    });
    let result = plan_durable_request(&mut state, &request, TickId::new(1));
    let outcome = result
        .as_ref()
        .map(|_| "planned".to_owned())
        .map_err(|error| (error.kind(), error.to_string()));
    assert!(
        matches!(&outcome, Err((ErrorKind::WouldBlock, _))),
        "a failed chunk request defers the edit, got {outcome:?}"
    );
    assert!(!state.durability.failed);
    assert!(state.durability.pending.is_empty());
    assert_eq!(
        state.clients.get(&1).unwrap().inventory,
        Inventory::default()
    );

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

/// A mobile probe whose planner returns a due time that does not advance.
/// That is genuine planner corruption, not capacity, and must still stop
/// the coordinator.
struct BadTick;

impl crate::server::entities::EntityTickPolicy for BadTick {
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        current_tick: u64,
        _catalog: &crate::content::Catalog,
        _view: &crate::server::voxel_view::VoxelView,
        _neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        use crate::server::entities::{EntityError, EntityTickPlan};
        let Some(due) = snapshot.next_tick else {
            return Err(EntityError::InvalidType);
        };
        if current_tick < due {
            return Err(EntityError::InvalidType);
        }
        Ok(EntityTickPlan {
            payload: None,
            next_tick: due,
            anchor_update: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
        })
    }
}

#[test]
fn genuine_entity_corruption_still_stops_the_coordinator() {
    use crate::server::entities::{EntityOwnership, TickPolicy};
    use crate::server::startup::StartupEntityType;
    use std::sync::Arc;

    let path = temp_save_dir("neighbour-corrupt-fatal");
    let bad_type = crate::content::EntityTypeId(70_015);
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: bad_type,
            key: "test:bad_tick".into(),
            schema_version: 1,
            schema_fingerprint: 0x4241_4454_4900_0001,
        })
        .unwrap();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:bad_tick".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(BadTick)),
    });
    let mut state = crate::server::server_state_with_startup(7, path.clone(), 1, startup).unwrap();
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    let id = stage_mobile_spawn(&mut state, bad_type, [0.5, 80.0, 0.5], 7);

    super::super::coordinator::queue_interaction_actions(&mut state, TickId::new(6));
    let result = super::super::coordinator::process_durable_actions(
        &mut state,
        TickId::new(6),
        Instant::now(),
    );
    let outcome = result
        .as_ref()
        .map(|_| "ok".to_owned())
        .map_err(|error| (error.kind(), error.to_string()));
    assert!(
        matches!(&outcome, Err((ErrorKind::InvalidData, _))),
        "genuine corruption must still stop the coordinator, got {outcome:?}"
    );
    assert!(state.durability.failed);
    assert_eq!(state.entities.snapshot(id).unwrap().revision, 1);
    let closed = super::super::coordinator::process_durable_actions(
        &mut state,
        TickId::new(6),
        Instant::now(),
    );
    assert!(closed.is_err(), "durable admission must stay closed");

    drop(state);
    fs::remove_dir_all(path).unwrap();
}
