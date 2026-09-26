//! Desktop client: network I/O and meshing stay off the window thread.
use crate::config::Config;
use crate::inventory::{HOTBAR_SLOTS, Inventory};
use crate::lighting::LightSample;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::raycast::{self, Hit};
use crate::render::{Camera, ChunkMesh, Renderer};
use crate::ui::{SettingId, UiControl, UiDebug, UiFrame, UiLayout, UiScreen, UiSettings};
use crate::world::{AIR, BlockId, Chunk, ChunkKey};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::TrySendError;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

const FRAME: Duration = Duration::from_nanos(16_666_667);
const SPEED: f32 = 8.0;
// Diagnostic interest-volume bound, not an eviction budget.
const MAX_CHUNKS: usize = (2 * crate::protocol::MAX_VIEW_DISTANCE as usize + 1).pow(2)
    * (2 * crate::protocol::VERTICAL_VIEW_DISTANCE as usize + 1);
const MAX_OUTSTANDING_ACTIONS: usize = 128;
const MAX_INCOMING_PER_FRAME: usize = 32;
const INCOMING_FRAME_BUDGET: Duration = Duration::from_millis(2);

pub(crate) mod drops;
use drops::DropAnimator;
mod movement;
use movement::predict_player_movement;

#[cfg(test)]
fn edit_for_hit(
    hit: Hit,
    place: bool,
    selected_item: Option<crate::items::ItemId>,
    slot: u8,
    action_id: u128,
) -> Option<ClientMessage> {
    edit_for_hit_with_catalog(
        hit,
        place,
        selected_item,
        slot,
        action_id,
        0.0,
        crate::content::catalog(),
    )
}

fn edit_for_hit_with_catalog(
    hit: Hit,
    place: bool,
    selected_item: Option<crate::items::ItemId>,
    slot: u8,
    action_id: u128,
    yaw: f32,
    catalog: &crate::content::Catalog,
) -> Option<ClientMessage> {
    let block = if place {
        let default_state = crate::items::placeable_block_in(selected_item?, catalog)?;
        // A cardinal `facing` property is a placement hint, not authority:
        // server-side block behavior validates the requested legal state and
        // derives every occupied cell before WAL admission.
        let facing = if !yaw.is_finite() {
            "north"
        } else if yaw.cos().abs() >= yaw.sin().abs() {
            if yaw.cos() > 0.0 { "west" } else { "east" }
        } else if yaw.sin() > 0.0 {
            "north"
        } else {
            "south"
        };
        catalog
            .state_with_property(default_state, "facing", facing)
            .unwrap_or(default_state)
    } else {
        AIR
    };
    let [x, y, z] = if place && catalog.block_flags(hit.block_id) & crate::content::REPLACEABLE == 0
    {
        hit.adjacent
    } else {
        hit.block
    };
    Some(ClientMessage::Edit {
        action_id,
        x,
        y,
        z,
        block,
        slot,
    })
}

fn escape_screen(screen: UiScreen) -> UiScreen {
    match screen {
        UiScreen::Playing => UiScreen::Pause,
        UiScreen::Inventory | UiScreen::Admin | UiScreen::Pause => UiScreen::Playing,
        UiScreen::Settings => UiScreen::Pause,
        UiScreen::Graphics => UiScreen::Settings,
    }
}

fn inventory_screen(screen: UiScreen) -> UiScreen {
    match screen {
        UiScreen::Playing => UiScreen::Inventory,
        UiScreen::Inventory => UiScreen::Playing,
        other => other,
    }
}

fn digit_slot(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::Digit1 => Some(0),
        KeyCode::Digit2 => Some(1),
        KeyCode::Digit3 => Some(2),
        KeyCode::Digit4 => Some(3),
        KeyCode::Digit5 => Some(4),
        KeyCode::Digit6 => Some(5),
        KeyCode::Digit7 => Some(6),
        KeyCode::Digit8 => Some(7),
        KeyCode::Digit9 => Some(8),
        _ => None,
    }
}

/// Urgent edits first, then missing geometry, then background replacements.
/// Distance and coordinates provide stable ordering instead of hash iteration.
fn mesh_priority(
    key: ChunkKey,
    center: ChunkKey,
    urgent: bool,
    displayed: bool,
) -> (u8, u64, i32, i32, i32) {
    let lane = if urgent {
        0
    } else if !displayed {
        1
    } else {
        2
    };
    let distance = i64::from(key.x).abs_diff(i64::from(center.x))
        + i64::from(key.y).abs_diff(i64::from(center.y))
        + i64::from(key.z).abs_diff(i64::from(center.z));
    (lane, distance, key.x, key.y, key.z)
}

/// Propagated light uses a local halo; direct sky also reads captured columns above it.
fn lighting_depends_on(target: ChunkKey, changed: ChunkKey) -> bool {
    i64::from(target.x).abs_diff(i64::from(changed.x)) <= 1
        && i64::from(target.z).abs_diff(i64::from(changed.z)) <= 1
        && i64::from(target.y) <= i64::from(changed.y) + 1
}

fn chunk_in_view(key: ChunkKey, center: ChunkKey, radius: u8) -> bool {
    let radius = i64::from(radius);
    (i64::from(key.x) - i64::from(center.x)).abs() <= radius
        && (i64::from(key.y) - i64::from(center.y)).abs()
            <= i64::from(crate::protocol::VERTICAL_VIEW_DISTANCE)
        && (i64::from(key.z) - i64::from(center.z)).abs() <= radius
}

/// High bits identify the server-issued durable session; low bits preserve
/// action order within that session.
fn action_id(session: u64, sequence: u64) -> u128 {
    (u128::from(session) << 64) | u128::from(sequence)
}

fn command_action_id(message: &ClientMessage) -> Option<u128> {
    match message {
        ClientMessage::Edit { action_id, .. }
        | ClientMessage::InventoryMove { action_id, .. }
        | ClientMessage::DropStack { action_id, .. }
        | ClientMessage::AdminGive { action_id, .. }
        | ClientMessage::AdminSpawnMossbun { action_id }
        | ClientMessage::EntityInteract { action_id, .. } => Some(*action_id),
        _ => None,
    }
}

#[derive(Default)]
struct ActionTracker {
    epoch: u64,
    next_seq: u64,
    acknowledged: u64,
    terminal: BTreeSet<u64>,
}

impl ActionTracker {
    fn install_fresh_session(
        &mut self,
        epoch: u64,
        next_seq: u64,
        acked_seq: u64,
    ) -> Result<(), &'static str> {
        if epoch == 0 || next_seq != 1 || acked_seq != 0 || self.epoch != 0 {
            return Err("invalid fresh action session");
        }
        self.epoch = epoch;
        self.next_seq = next_seq;
        Ok(())
    }

    fn allocate(&mut self) -> Option<u128> {
        if self.epoch == 0 || self.next_seq == u64::MAX {
            return None;
        }
        let id = action_id(self.epoch, self.next_seq);
        self.next_seq += 1;
        Some(id)
    }

    fn terminal_result(&mut self, id: u128) -> Result<Option<u64>, &'static str> {
        let epoch = (id >> 64) as u64;
        let seq = id as u64;
        if epoch != self.epoch || seq == 0 || seq >= self.next_seq {
            return Err("result outside the current action session");
        }
        if seq <= self.acknowledged {
            return Ok(None);
        }
        self.terminal.insert(seq);
        let before = self.acknowledged;
        while self.terminal.remove(&(self.acknowledged + 1)) {
            self.acknowledged += 1;
        }
        Ok((self.acknowledged != before).then_some(self.acknowledged))
    }
}

pub(crate) mod actors;
mod admin;
mod entities;
mod workers;
use entities::{Assembly, EntityClientRegistry, EntityVerb, Replicas};
use workers::{ConfigWriter, Incoming, Mesher, MesherJob, Network};

#[derive(Default)]
struct Keys {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

struct ClientApp {
    catalog: Arc<crate::content::Catalog>,
    inventory: Inventory,
    drop_animator: DropAnimator,
    actor_animator: actors::ActorAnimator,
    drops_revision: u64,
    inventory_source: Option<u8>,
    network: Network,
    mesher: Mesher,
    config: Config,
    config_writer: ConfigWriter,
    screen: UiScreen,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    ui_layout: Option<UiLayout>,
    chunks: HashMap<ChunkKey, Arc<Chunk>>,
    replicas: Replicas,
    entity_registry: EntityClientRegistry,
    pending_mesh: HashMap<ChunkKey, u64>,
    urgent_mesh: std::collections::HashSet<ChunkKey>,
    lighting_revisions: HashMap<ChunkKey, u64>,
    light_samples: HashMap<ChunkKey, (u64, Box<[LightSample]>)>,
    next_lighting_revision: u64,
    world_seed: Option<u64>,
    owned_entity_id: Option<u64>,
    pending_upload: VecDeque<ChunkMesh>,
    pending_commands: VecDeque<ClientMessage>,
    position: Vec3,
    yaw: f32,
    pitch: f32,
    keys: Keys,
    shift_down: bool,
    grabbed: bool,
    cursor: (f32, f32),
    focused_control: Option<UiControl>,
    status: Option<(String, Instant)>,
    effective_view_distance: u8,
    last_fps: f32,
    last_p95_ms: f32,
    last_visible_chunks: usize,
    next_frame: Instant,
    last_frame: Instant,
    next_seq: u64,
    actions: ActionTracker,
    pending_actions: BTreeMap<u128, ClientMessage>,
    deferred_actions: BTreeMap<u128, Instant>,
    unacked: VecDeque<(u64, Vec3)>,
    frame_count: u64,
    last_report: Instant,
    frame_ms: Vec<f32>,
    disconnected: bool,
    admin_enabled: bool,
    admin_input: String,
    admin_page: usize,
}

impl ClientApp {
    fn new(network: Network, config: Config, config_path: PathBuf) -> Self {
        let now = Instant::now();
        let catalog = Arc::clone(&network.catalog);
        let effective_view_distance = config.view_distance;
        let config_writer = ConfigWriter::new(&config, config_path);
        let entity_registry = EntityClientRegistry::builtins(&catalog);
        Self {
            catalog,
            inventory: Inventory::default(),
            drop_animator: DropAnimator::new(now),
            actor_animator: actors::ActorAnimator::default(),
            drops_revision: 0,
            inventory_source: None,
            network,
            mesher: Mesher::new(),
            config,
            config_writer,
            screen: UiScreen::Playing,
            window: None,
            renderer: None,
            ui_layout: None,
            chunks: HashMap::new(),
            replicas: Replicas::default(),
            entity_registry,
            pending_mesh: HashMap::new(),
            urgent_mesh: std::collections::HashSet::new(),
            lighting_revisions: HashMap::new(),
            light_samples: HashMap::new(),
            next_lighting_revision: 1,
            world_seed: None,
            owned_entity_id: None,
            pending_upload: VecDeque::new(),
            pending_commands: VecDeque::new(),
            position: Vec3::new(0.5, 40.0, 0.5),
            yaw: 0.0,
            pitch: -0.2,
            keys: Keys::default(),
            shift_down: false,
            grabbed: false,
            cursor: (0.0, 0.0),
            focused_control: None,
            status: None,
            effective_view_distance,
            last_fps: 0.0,
            last_p95_ms: 0.0,
            last_visible_chunks: 0,
            next_frame: now,
            last_frame: now,
            next_seq: 1,
            actions: ActionTracker::default(),
            pending_actions: BTreeMap::new(),
            deferred_actions: BTreeMap::new(),
            unacked: VecDeque::new(),
            frame_count: 0,
            last_report: now,
            frame_ms: Vec::with_capacity(512),
            disconnected: false,
            admin_enabled: false,
            admin_input: String::new(),
            admin_page: 0,
        }
    }

    fn camera(&self) -> Camera {
        Camera {
            position: self.position + Vec3::Y * 1.6,
            yaw: self.yaw,
            pitch: self.pitch,
            fov_y_radians: self.config.fov_degrees.to_radians(),
        }
    }

    fn show_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now() + Duration::from_secs(4)));
    }

    fn set_screen(&mut self, screen: UiScreen) {
        self.screen = screen;
        self.keys = Keys::default();
        self.focused_control = None;
        self.inventory_source = None;
        self.set_grab(screen == UiScreen::Playing);
        self.refresh_layout();
        if screen == UiScreen::Playing && !self.grabbed {
            self.show_status("Click to capture mouse");
        }
    }

    fn refresh_layout(&mut self) {
        if let Some(window) = &self.window {
            let size = window.inner_size();
            self.ui_layout = Some(UiLayout::new(
                size.width,
                size.height,
                self.config.scale,
                self.screen,
            ));
        }
    }

    fn on_escape(&mut self) {
        self.set_screen(escape_screen(self.screen));
    }

    fn toggle_inventory(&mut self) {
        let next = inventory_screen(self.screen);
        if next != self.screen {
            self.set_screen(next);
        }
    }

    fn select_slot(&mut self, slot: usize) {
        if slot < HOTBAR_SLOTS && self.config.selected_slot != slot {
            self.config.selected_slot = slot;
            self.config_writer.request_save(&self.config);
        }
    }

    fn inventory_click(&mut self, slot: u8, split: bool) {
        if slot as usize >= crate::inventory::SLOTS {
            return;
        }
        match self.inventory_source {
            None => {
                if self.inventory.slots[slot as usize].is_some() {
                    self.inventory_source = Some(slot);
                }
            }
            Some(source) if source == slot => self.inventory_source = None,
            Some(source) => {
                if let Some(stack) = self.inventory.slots[source as usize].as_ref() {
                    let count = if split {
                        stack.count.div_ceil(2)
                    } else {
                        stack.count
                    };
                    let Some(action_id) = self.allocate_action_id() else {
                        self.show_status("Action session pending or busy");
                        return;
                    };
                    self.queue_command(ClientMessage::InventoryMove {
                        action_id,
                        from: source,
                        to: slot,
                        count,
                    });
                }
                self.inventory_source = None;
            }
        }
    }

    fn change_setting(&mut self, setting: SettingId, increase: bool) {
        let sign = if increase { 1.0 } else { -1.0 };
        match setting {
            SettingId::PostProcessing => self.config.post_processing = !self.config.post_processing,
            SettingId::Bloom => self.config.bloom_enabled = !self.config.bloom_enabled,
            SettingId::Exposure => {
                self.config.exposure = (self.config.exposure + sign * 0.05).clamp(0.25, 4.0)
            }
            SettingId::BloomStrength => {
                self.config.bloom_strength =
                    (self.config.bloom_strength + sign * 0.02).clamp(0.0, 1.0)
            }
            SettingId::Sensitivity => {
                self.config.sensitivity =
                    (self.config.sensitivity + sign * 0.00025).clamp(0.0002, 0.01);
            }
            SettingId::FieldOfView => {
                self.config.fov_degrees = (self.config.fov_degrees + sign * 5.0).clamp(40.0, 110.0);
            }
            SettingId::ViewDistance => {
                let radius = (i32::from(self.config.view_distance) + sign as i32).clamp(1, 6) as u8;
                if radius != self.config.view_distance {
                    self.config.view_distance = radius;
                    self.queue_command(ClientMessage::SetView { radius });
                }
            }
            SettingId::UiScale => {
                self.config.scale = (self.config.scale + sign * 0.1).clamp(0.75, 2.0);
                self.refresh_layout();
            }
            SettingId::Lighting => {
                self.config.bounced_gi = !self.config.bounced_gi;
                let keys: Vec<_> = self.chunks.keys().copied().collect();
                for key in keys {
                    self.queue_relight(key, false);
                }
            }
        }
        self.config.sanitize();
        self.config_writer.request_save(&self.config);
    }

    fn apply_fullscreen(&self) {
        if let Some(window) = &self.window {
            window.set_fullscreen(if self.config.fullscreen {
                Some(Fullscreen::Borderless(None))
            } else {
                None
            });
        }
    }

    fn activate_control(&mut self, event_loop: &ActiveEventLoop, control: UiControl) {
        match control {
            UiControl::HotbarSlot(_) => {}
            UiControl::InventorySlot(slot) if self.screen == UiScreen::Inventory => {
                self.inventory_click(slot, false)
            }
            UiControl::InventorySlot(_) => {}
            UiControl::Resume => self.set_screen(UiScreen::Playing),
            UiControl::OpenSettings => self.set_screen(UiScreen::Settings),
            UiControl::ToggleSettingsPage => {
                self.set_screen(if self.screen == UiScreen::Graphics {
                    UiScreen::Settings
                } else {
                    UiScreen::Graphics
                })
            }
            UiControl::OpenAdmin if self.admin_enabled => self.set_screen(UiScreen::Admin),
            UiControl::OpenAdmin => {}
            UiControl::AdminItem(index) if self.screen == UiScreen::Admin => {
                self.admin_grant_index(index)
            }
            UiControl::AdminItem(_) => {}
            UiControl::AdminPrev if self.screen == UiScreen::Admin => {
                self.admin_page = self.admin_page.saturating_sub(1)
            }
            UiControl::AdminNext if self.screen == UiScreen::Admin => {
                let pages = self.catalog.items().count().div_ceil(24).max(1);
                self.admin_page = (self.admin_page + 1).min(pages - 1);
            }
            UiControl::AdminRun if self.screen == UiScreen::Admin => self.admin_run(),
            UiControl::AdminPrev | UiControl::AdminNext | UiControl::AdminRun => {}
            UiControl::Exit => event_loop.exit(),
            UiControl::Back => self.set_screen(if self.screen == UiScreen::Graphics {
                UiScreen::Settings
            } else {
                UiScreen::Pause
            }),
            UiControl::Decrease(setting) => self.change_setting(setting, false),
            UiControl::Increase(setting) => self.change_setting(setting, true),
            UiControl::ToggleFullscreen => {
                self.config.fullscreen = !self.config.fullscreen;
                self.apply_fullscreen();
                self.config_writer.request_save(&self.config);
            }
        }
    }

    fn focus_order(&self) -> Vec<UiControl> {
        match self.screen {
            UiScreen::Playing => Vec::new(),
            UiScreen::Inventory => (0..crate::inventory::SLOTS as u8)
                .map(UiControl::InventorySlot)
                .collect(),
            UiScreen::Admin => (0..24u8)
                .map(UiControl::AdminItem)
                .chain([
                    UiControl::AdminPrev,
                    UiControl::AdminNext,
                    UiControl::AdminRun,
                ])
                .collect(),
            UiScreen::Pause => {
                let mut controls = vec![UiControl::Resume, UiControl::OpenSettings];
                if self.admin_enabled {
                    controls.push(UiControl::OpenAdmin);
                }
                controls.push(UiControl::Exit);
                controls
            }
            UiScreen::Settings => vec![
                UiControl::ToggleSettingsPage,
                UiControl::Decrease(SettingId::Sensitivity),
                UiControl::Increase(SettingId::Sensitivity),
                UiControl::Decrease(SettingId::FieldOfView),
                UiControl::Increase(SettingId::FieldOfView),
                UiControl::Decrease(SettingId::ViewDistance),
                UiControl::Increase(SettingId::ViewDistance),
                UiControl::Decrease(SettingId::UiScale),
                UiControl::Increase(SettingId::UiScale),
                UiControl::Decrease(SettingId::Lighting),
                UiControl::Increase(SettingId::Lighting),
                UiControl::ToggleFullscreen,
                UiControl::Back,
            ],
            UiScreen::Graphics => vec![
                UiControl::ToggleSettingsPage,
                UiControl::Decrease(SettingId::PostProcessing),
                UiControl::Increase(SettingId::PostProcessing),
                UiControl::Decrease(SettingId::Exposure),
                UiControl::Increase(SettingId::Exposure),
                UiControl::Decrease(SettingId::Bloom),
                UiControl::Increase(SettingId::Bloom),
                UiControl::Decrease(SettingId::BloomStrength),
                UiControl::Increase(SettingId::BloomStrength),
                UiControl::Back,
            ],
        }
    }

    fn advance_focus(&mut self, reverse: bool) {
        let controls = self.focus_order();
        if controls.is_empty() {
            return;
        }
        let next = self
            .focused_control
            .and_then(|focused| controls.iter().position(|control| *control == focused))
            .map(|index| {
                (index + controls.len() + if reverse { controls.len() - 1 } else { 1 })
                    % controls.len()
            })
            .unwrap_or(0);
        self.focused_control = Some(controls[next]);
    }

    fn set_grab(&mut self, grab: bool) {
        if let Some(window) = &self.window {
            if grab {
                let success = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                    .is_ok();
                self.grabbed = success;
                window.set_cursor_visible(!success);
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
                self.grabbed = false;
            }
        }
    }

    fn queue_command(&mut self, message: ClientMessage) {
        if let ClientMessage::Resync { key } = &message
            && self.pending_commands.iter().any(|pending| matches!(pending, ClientMessage::Resync { key: pending_key } if pending_key == key))
        {
            return;
        }
        if let Some(action_id) = command_action_id(&message) {
            self.pending_actions.insert(action_id, message.clone());
            if self.pending_commands.contains(&message) {
                return;
            }
        }
        if !self.pending_commands.is_empty() || !self.network.send(message.clone()) {
            if self.pending_commands.len() < 128 {
                self.pending_commands.push_back(message);
            } else {
                eprintln!("client command backlog exceeded limit; disconnecting");
                self.disconnected = true;
            }
        }
    }

    fn allocate_action_id(&mut self) -> Option<u128> {
        if self.pending_actions.len() >= MAX_OUTSTANDING_ACTIONS {
            return None;
        }
        self.actions.allocate()
    }

    fn queue_relight(&mut self, key: ChunkKey, include_neighbors: bool) {
        for affected in self.chunks.keys().copied().filter(|affected| {
            *affected == key || (include_neighbors && lighting_depends_on(*affected, key))
        }) {
            let revision = self.next_lighting_revision;
            self.next_lighting_revision = self.next_lighting_revision.wrapping_add(1).max(1);
            self.lighting_revisions.insert(affected, revision);
            self.pending_mesh.insert(affected, revision);
            // Renderer uploads have already passed the client revision check.
            // Cancel them here as soon as any lighting dependency changes.
            if let Some(renderer) = &mut self.renderer {
                renderer.discard_pending_chunk(affected);
            }
        }
        self.pending_upload
            .retain(|mesh| self.lighting_revisions.get(&mesh.key) == Some(&mesh.lighting_revision));
    }

    fn queue_edited_chunk_relight(&mut self, key: ChunkKey) {
        self.queue_relight(key, true);
        // The light footprint belongs to the edit too: rebuilding only its
        // own chunk urgently leaves neighbouring faces lit by an old lamp.
        self.urgent_mesh.extend(
            self.chunks
                .keys()
                .copied()
                .filter(|affected| lighting_depends_on(*affected, key)),
        );
    }

    fn lighting_snapshot(&self, key: ChunkKey) -> HashMap<ChunkKey, Arc<Chunk>> {
        self.chunks
            .iter()
            .filter(|(neighbor, _)| lighting_depends_on(key, **neighbor))
            .map(|(neighbor, chunk)| (*neighbor, Arc::clone(chunk)))
            .collect()
    }

    fn accept(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::ContentManifestPart { .. } => {
                self.disconnected = true;
                self.show_status("Unexpected content manifest after handshake");
            }
            ServerMessage::Welcome { id, seed } => {
                self.world_seed = Some(seed);
                eprintln!("connected as player {id}, world seed {seed}")
            }
            ServerMessage::OwnedEntity { id } => {
                if self.owned_entity_id.is_some_and(|old| old != id) {
                    self.disconnected = true;
                    self.show_status("Conflicting owned entity identity");
                } else {
                    self.owned_entity_id = Some(id);
                }
            }
            ServerMessage::ActionSession {
                epoch,
                next_seq,
                acked_seq,
            } => {
                if let Err(error) = self
                    .actions
                    .install_fresh_session(epoch, next_seq, acked_seq)
                {
                    eprintln!("action session: {error}");
                    self.disconnected = true;
                }
            }
            ServerMessage::Position { ack_seq, x, y, z } => {
                while self.unacked.front().is_some_and(|(seq, _)| *seq <= ack_seq) {
                    self.unacked.pop_front();
                }
                let mut predicted = Vec3::new(x, y, z);
                for (_, delta) in &self.unacked {
                    predicted =
                        predict_player_movement(&self.chunks, &self.catalog, predicted, *delta);
                }
                self.position = predicted;
            }
            ServerMessage::Chunk(chunk) => {
                let key = chunk.key;
                if self
                    .chunks
                    .get(&key)
                    .is_some_and(|old| old.version > chunk.version)
                {
                    return;
                }
                let modified = chunk.version != 0;
                self.chunks.insert(key, Arc::new(chunk));
                self.queue_relight(key, modified);
            }
            ServerMessage::WorldSnapshotStart(_)
            | ServerMessage::EntitySnapshotPage(_)
            | ServerMessage::WorldCommitPart(_) => {
                let block_commit = matches!(&message, ServerMessage::WorldCommitPart(_));
                match self.replicas.accept(
                    message,
                    &self.catalog,
                    &mut self.chunks,
                    &self.entity_registry,
                ) {
                    Assembly::Waiting => {}
                    Assembly::Installed(keys) => {
                        for key in keys {
                            if block_commit {
                                self.queue_edited_chunk_relight(key);
                            } else {
                                self.queue_relight(key, true);
                            }
                        }
                    }
                    Assembly::Resync(keys) => {
                        for key in keys {
                            self.queue_command(ClientMessage::Resync { key });
                        }
                    }
                }
            }
            ServerMessage::Delta {
                key,
                version,
                x,
                y,
                z,
                block,
            } => {
                if let Some(chunk) = self.chunks.get_mut(&key) {
                    if version == chunk.version + 1 {
                        if let Some(index) = Chunk::index([x as usize, y as usize, z as usize]) {
                            let updated = Arc::make_mut(chunk);
                            updated.blocks.set(index, block);
                            updated.version = version;
                            self.queue_edited_chunk_relight(key);
                        }
                    } else if version > chunk.version {
                        self.queue_command(ClientMessage::Resync { key });
                    }
                } else {
                    self.queue_command(ClientMessage::Resync { key });
                }
            }
            ServerMessage::EditRejected { reason } => {
                self.show_status(format!("Edit rejected: {reason}"));
            }
            ServerMessage::ActionResult {
                action_id,
                accepted,
                reason,
            } => {
                if !accepted {
                    self.show_status(format!("Action rejected: {reason}"));
                }
                self.pending_actions.remove(&action_id);
                self.deferred_actions.remove(&action_id);
                match self.actions.terminal_result(action_id) {
                    Ok(Some(through_seq)) => self.queue_command(ClientMessage::ActionAck {
                        epoch: self.actions.epoch,
                        through_seq,
                    }),
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("action result: {error}");
                        self.disconnected = true;
                    }
                }
            }
            ServerMessage::ActionDeferred { action_id } => {
                if self.pending_actions.contains_key(&action_id) {
                    self.deferred_actions
                        .insert(action_id, Instant::now() + Duration::from_millis(50));
                }
            }
            ServerMessage::ViewDistance { radius } => {
                self.effective_view_distance = radius;
            }
            ServerMessage::Inventory { revision, slots } => {
                if revision >= self.inventory.revision {
                    self.inventory = Inventory { revision, slots };
                }
            }
            ServerMessage::Drops { revision, items } => {
                if revision >= self.drops_revision {
                    self.drops_revision = revision;
                    self.drop_animator.snapshot(items, Instant::now());
                }
            }
            ServerMessage::Pickups { items } => {
                self.drop_animator.picked_up(items, Instant::now());
            }
            ServerMessage::Pong { .. } => {}
        }
    }

    fn poll_work(&mut self) {
        let now = Instant::now();
        let due: Vec<_> = self
            .deferred_actions
            .iter()
            .filter_map(|(&id, &at)| (at <= now).then_some(id))
            .collect();
        for id in due {
            self.deferred_actions.remove(&id);
            if let Some(command) = self.pending_actions.get(&id).cloned() {
                self.queue_command(command);
            }
        }
        while let Some(message) = self.pending_commands.front().cloned() {
            if !self.network.send(message) {
                break;
            }
            self.pending_commands.pop_front();
        }
        let incoming_started = Instant::now();
        for count in 0..MAX_INCOMING_PER_FRAME {
            if count > 0 && incoming_started.elapsed() >= INCOMING_FRAME_BUDGET {
                break;
            }
            match self.network.incoming.try_recv() {
                Ok(Incoming::Message(message)) => self.accept(*message),
                Ok(Incoming::Closed(reason)) => {
                    eprintln!("disconnected: {reason}");
                    self.disconnected = true;
                    break;
                }
                Err(_) => break,
            }
        }
        let center = crate::world::world_to_chunk(
            self.position.x.floor() as i32,
            self.position.y.floor() as i32,
            self.position.z.floor() as i32,
        )
        .0;
        let radius = self.effective_view_distance;
        let mut evicted = Vec::new();
        self.chunks.retain(|key, chunk| {
            let keep = chunk_in_view(*key, center, radius);
            if !keep {
                evicted.push((*key, chunk.version != 0));
            }
            keep
        });
        self.replicas.retain(|key| self.chunks.contains_key(&key));
        self.lighting_revisions
            .retain(|key, _| self.chunks.contains_key(key));
        self.light_samples
            .retain(|key, _| self.chunks.contains_key(key));
        self.pending_mesh
            .retain(|key, _| self.chunks.contains_key(key));
        self.urgent_mesh.retain(|key| self.chunks.contains_key(key));
        for &(key, modified) in &evicted {
            if modified {
                self.queue_relight(key, true);
            }
        }
        if let Some(renderer) = &mut self.renderer {
            for (key, _) in evicted {
                renderer.remove_chunk(key);
            }
            self.pending_upload
                .retain(|mesh| self.chunks.contains_key(&mesh.key));
            for _ in 0..(128usize.saturating_sub(self.pending_upload.len())).min(64) {
                let Ok(result) = self.mesher.results.try_recv() else {
                    break;
                };
                let mesh = result.mesh;
                if self
                    .chunks
                    .get(&mesh.key)
                    .is_some_and(|chunk| chunk.version == mesh.version)
                    && self.lighting_revisions.get(&mesh.key) == Some(&mesh.lighting_revision)
                {
                    self.light_samples
                        .insert(mesh.key, (mesh.lighting_revision, result.lighting));
                    if self.urgent_mesh.contains(&mesh.key) {
                        self.pending_upload.push_front(mesh);
                    } else {
                        self.pending_upload.push_back(mesh);
                    }
                }
            }
            while let Some(mesh) = self.pending_upload.pop_front() {
                let key = mesh.key;
                if self
                    .chunks
                    .get(&key)
                    .is_none_or(|chunk| chunk.version != mesh.version)
                    || self.lighting_revisions.get(&key) != Some(&mesh.lighting_revision)
                {
                    continue;
                }
                if let Err(mesh) = renderer.enqueue_mesh(mesh, self.urgent_mesh.contains(&key)) {
                    self.pending_upload.push_front(mesh);
                    break;
                }
                self.urgent_mesh.remove(&key);
            }
        }
        let Some(seed) = self.world_seed else {
            return;
        };
        for _ in 0..16 {
            let Some(key) = self.pending_mesh.keys().copied().min_by_key(|key| {
                mesh_priority(
                    *key,
                    center,
                    self.urgent_mesh.contains(key),
                    self.renderer
                        .as_ref()
                        .is_some_and(|renderer| renderer.has_chunk_mesh(*key)),
                )
            }) else {
                break;
            };
            let revision = self.pending_mesh.remove(&key).unwrap();
            let Some(chunk) = self.chunks.get(&key).cloned() else {
                continue;
            };
            let job = MesherJob {
                chunk,
                known: self.lighting_snapshot(key),
                catalog: Arc::clone(&self.catalog),
                seed,
                revision,
                bounced_gi: self.config.bounced_gi,
            };
            let sender = if self.urgent_mesh.contains(&key) {
                &self.mesher.urgent_jobs
            } else {
                &self.mesher.jobs
            };
            if let Err(TrySendError::Full(_job)) = sender.try_send(job) {
                self.pending_mesh.insert(key, revision);
                break;
            }
        }
        if self.chunks.len() > MAX_CHUNKS {
            eprintln!("client chunk cache exceeded target: {}", self.chunks.len());
        }
    }

    fn move_player(&mut self, dt: f32) {
        if self.screen != UiScreen::Playing || !self.grabbed {
            return;
        }
        if !self.pending_commands.is_empty() {
            return;
        }
        let forward = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let right = Vec3::new(-self.yaw.sin(), 0.0, self.yaw.cos());
        let mut direction = Vec3::ZERO;
        if self.keys.forward {
            direction += forward;
        }
        if self.keys.back {
            direction -= forward;
        }
        if self.keys.right {
            direction += right;
        }
        if self.keys.left {
            direction -= right;
        }
        if self.keys.up {
            direction += Vec3::Y;
        }
        if self.keys.down {
            direction -= Vec3::Y;
        }
        if direction == Vec3::ZERO {
            return;
        }
        let delta = direction.normalize() * SPEED * dt.min(0.05);
        let seq = self.next_seq;
        self.next_seq += 1;
        if self.unacked.len() >= 256 {
            return;
        }
        if self.network.send(ClientMessage::Move {
            seq,
            dx: delta.x,
            dy: delta.y,
            dz: delta.z,
        }) {
            self.position =
                predict_player_movement(&self.chunks, &self.catalog, self.position, delta);
            self.unacked.push_back((seq, delta));
        }
    }

    fn block_at(&self, x: i32, y: i32, z: i32) -> Option<BlockId> {
        let (key, local) = crate::world::world_to_chunk(x, y, z);
        self.chunks.get(&key)?.block(local)
    }

    fn light_at(&self, position: Vec3) -> LightSample {
        self.sample_light(position, false)
    }

    /// Actors use the last completed field while relighting, just as terrain
    /// keeps its displayed mesh until replacement. A pending revision is not
    /// darkness. Completed dark samples still replace old illumination.
    fn actor_light_at(&self, position: Vec3) -> LightSample {
        self.sample_light(position, true)
    }

    fn sample_light(&self, position: Vec3, displayed: bool) -> LightSample {
        let (key, local) = crate::world::world_to_chunk(
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        );
        let Some((revision, samples)) = self.light_samples.get(&key) else {
            return LightSample::default();
        };
        if !displayed && self.lighting_revisions.get(&key) != Some(revision) {
            return LightSample::default();
        }
        Chunk::index(local)
            .and_then(|index| samples.get(index))
            .copied()
            .unwrap_or_default()
    }

    fn aimed_block(&self) -> Option<Hit> {
        let camera = self.camera();
        raycast::raycast_with_catalog(
            camera.position,
            camera.direction(),
            7.0,
            |x, y, z| self.block_at(x, y, z),
            &self.catalog,
        )
    }

    fn edit_aimed_block(&mut self, place: bool) {
        if self.screen != UiScreen::Playing || !self.grabbed {
            return;
        }
        if let Some(hit) = self.aimed_block() {
            let item = self.inventory.slots[self.config.selected_slot]
                .as_ref()
                .map(|stack| stack.item);
            if let Some(mut command) = edit_for_hit_with_catalog(
                hit,
                place,
                item,
                self.config.selected_slot as u8,
                0,
                self.yaw,
                &self.catalog,
            ) {
                let Some(action_id) = self.allocate_action_id() else {
                    self.show_status("Action session pending or busy");
                    return;
                };
                if let ClientMessage::Edit { action_id: id, .. } = &mut command {
                    *id = action_id;
                }
                self.queue_command(command);
            } else {
                self.show_status(if item.is_some() {
                    "Selected item cannot be placed"
                } else {
                    "Selected slot is empty"
                });
            }
        }
    }

    /// Aim-block interaction through the generic entity registry: the first
    /// adapter handling the hit builds the opaque request bytes and the
    /// server owns every inventory decision.
    fn interact_aimed_entity(&mut self, verb: EntityVerb) {
        if self.screen != UiScreen::Playing || !self.grabbed {
            return;
        }
        let Some(hit) = self.aimed_block() else {
            return;
        };
        if !self.entity_registry.handles(hit, &self.catalog) {
            return;
        }
        let Some(action_id) = self.allocate_action_id() else {
            self.show_status("Action session pending or busy");
            return;
        };
        if let Some(message) = self.entity_registry.interact(
            hit,
            &self.catalog,
            action_id,
            self.config.selected_slot as u8,
            verb,
        ) {
            self.queue_command(message);
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.poll_work();
        self.move_player(dt);
        let camera = self.camera();
        if self.status.as_ref().is_some_and(|(_, until)| now > *until) {
            self.status = None;
        }
        let target = if self.screen == UiScreen::Playing {
            self.aimed_block().map(|hit| hit.block)
        } else {
            None
        };
        let status = if self.screen == UiScreen::Playing && !self.grabbed {
            Some("Click to capture mouse")
        } else {
            self.status.as_ref().map(|(message, _)| message.as_str())
        };
        let ui = UiFrame {
            screen: self.screen,
            selected_slot: self.config.selected_slot,
            inventory: self.inventory.slots.clone(),
            inventory_source: self.inventory_source,
            admin_enabled: self.admin_enabled,
            admin_page: self.admin_page,
            admin_input: &self.admin_input,
            target,
            status,
            debug: self.config.debug_hud.then_some(UiDebug {
                position: self.position.to_array(),
                fps: self.last_fps,
                frame_ms: self.last_p95_ms,
                visible_chunks: self.last_visible_chunks,
                cached_chunks: self.chunks.len(),
                latency_ms: None,
            }),
            settings: UiSettings {
                post_processing: self.config.post_processing,
                exposure: self.config.exposure,
                bloom_enabled: self.config.bloom_enabled,
                bloom_strength: self.config.bloom_strength,
                sensitivity: self.config.sensitivity,
                fov_degrees: self.config.fov_degrees,
                view_distance: self.effective_view_distance,
                scale: self.config.scale,
                fullscreen: self.config.fullscreen,
                bounced_gi: self.config.bounced_gi,
            },
            hovered: self.focused_control,
        };
        let mut visual_drops = self.drop_animator.visuals(now, self.position);
        for drop in &mut visual_drops {
            drop.light = self.light_at(drop.center);
        }
        let mut visual_avatars = self
            .replicas
            .visual_avatars(self.position, self.owned_entity_id);
        self.actor_animator.present(&mut visual_avatars, now);
        for avatar in &mut visual_avatars {
            let height = match avatar.model {
                crate::render::AvatarModel::Player => 1.45,
                crate::render::AvatarModel::Mossbun => 0.5,
            };
            let sample = self.actor_light_at(avatar.position + Vec3::Y * height);
            avatar.light_levels = [sample.sky, sample.glow, 0, 0];
            avatar.bounce = [sample.bounce[0], sample.bounce[1], sample.bounce[2], 0];
        }
        if let Some(renderer) = &mut self.renderer {
            renderer.set_drops(&visual_drops);
            renderer.set_avatars(&visual_avatars);
            renderer.configure_post(
                self.config.post_processing,
                self.config.exposure,
                if self.config.bloom_enabled {
                    self.config.bloom_strength
                } else {
                    0.0
                },
            );
            match renderer.render(camera, &ui) {
                Ok(stats) => {
                    self.last_visible_chunks = stats.visible_chunks;
                    self.frame_count += 1;
                    self.frame_ms.push(dt * 1000.0);
                    if now.duration_since(self.last_report) >= Duration::from_secs(5) {
                        let seconds = now.duration_since(self.last_report).as_secs_f32();
                        self.frame_ms.sort_by(f32::total_cmp);
                        let p95 = self.frame_ms[((self.frame_ms.len() - 1) * 95) / 100];
                        let p99 = self.frame_ms[((self.frame_ms.len() - 1) * 99) / 100];
                        self.last_fps = self.frame_count as f32 / seconds;
                        self.last_p95_ms = p95;
                        eprintln!(
                            "{:.1} FPS | frame p95 {:.1} ms, p99 {:.1} ms | {} chunks visible | {} triangles | {} uploads, {} pending | {} cached chunks",
                            self.last_fps,
                            p95,
                            p99,
                            stats.visible_chunks,
                            stats.drawn_triangles,
                            stats.uploaded_chunks,
                            stats.pending_chunks,
                            self.chunks.len()
                        );
                        self.last_report = now;
                        self.frame_count = 0;
                        self.frame_ms.clear();
                    }
                }
                Err(error) => eprintln!("render error: {error:?}"),
            }
        }
    }
}

mod events;

pub fn run_client(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_client_inner(addr, false)
}

pub fn run_client_with_admin(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_client_inner(addr, true)
}

fn run_client_inner(addr: &str, admin_enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = Config::default_path();
    let mut config = Config::load(&config_path);
    config.ensure_profile(&config_path)?;
    let network = Network::connect(addr, config.view_distance, config.profile)?;
    let event_loop = EventLoop::new()?;
    let mut app = ClientApp::new(network, config, config_path);
    app.admin_enabled = admin_enabled;
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests;
