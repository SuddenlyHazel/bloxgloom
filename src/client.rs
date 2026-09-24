//! Desktop client: network I/O and meshing stay off the window thread.
use crate::config::Config;
use crate::lighting::LightField;
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::raycast::{self, Hit};
use crate::render::{self, Camera, ChunkMesh, Renderer};
use crate::ui::{SettingId, UiControl, UiDebug, UiFrame, UiLayout, UiScreen, UiSettings};
use crate::world::{Chunk, ChunkKey};
use glam::Vec3;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

const FRAME: Duration = Duration::from_nanos(16_666_667);
const SPEED: f32 = 8.0;
const MAX_CHUNKS: usize = 512;

fn edit_for_hit(hit: Hit, place: bool, selected_block: u8) -> ClientMessage {
    let [x, y, z] = if place { hit.adjacent } else { hit.block };
    ClientMessage::Edit {
        x,
        y,
        z,
        block: if place { selected_block } else { 0 },
    }
}

fn escape_screen(screen: UiScreen) -> UiScreen {
    match screen {
        UiScreen::Playing => UiScreen::Pause,
        UiScreen::Inventory | UiScreen::Pause => UiScreen::Playing,
        UiScreen::Settings => UiScreen::Pause,
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

fn chunk_in_view(key: ChunkKey, center: ChunkKey, radius: u8) -> bool {
    let radius = i64::from(radius);
    (i64::from(key.x) - i64::from(center.x)).abs() <= radius
        && (i64::from(key.y) - i64::from(center.y)).abs() <= 1
        && (i64::from(key.z) - i64::from(center.z)).abs() <= radius
}

enum Incoming {
    Message(ServerMessage),
    Closed(String),
}

struct Network {
    incoming: Receiver<Incoming>,
    outgoing: SyncSender<ClientMessage>,
}

impl Network {
    fn connect(addr: &str, view_distance: u8) -> io::Result<Self> {
        let socket = TcpStream::connect(addr)?;
        socket.set_nodelay(true)?;
        let mut reader = socket.try_clone()?;
        let mut writer = socket;
        let (incoming_tx, incoming) = mpsc::sync_channel(256);
        let (outgoing, outgoing_rx) = mpsc::sync_channel(256);
        thread::spawn(move || {
            loop {
                match protocol::read_server(&mut reader) {
                    Ok(message) => {
                        if incoming_tx.send(Incoming::Message(message)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = incoming_tx.try_send(Incoming::Closed(error.to_string()));
                        break;
                    }
                }
            }
        });
        thread::spawn(move || {
            for message in outgoing_rx {
                if protocol::write_client(&mut writer, &message).is_err() {
                    break;
                }
            }
        });
        outgoing
            .send(ClientMessage::Hello {
                name: "Player".into(),
            })
            .map_err(|_| io::Error::other("network writer stopped"))?;
        outgoing
            .send(ClientMessage::SetView {
                radius: view_distance,
            })
            .map_err(|_| io::Error::other("network writer stopped"))?;
        Ok(Self { incoming, outgoing })
    }

    fn send(&self, message: ClientMessage) -> bool {
        match self.outgoing.try_send(message) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                eprintln!("client command queue full");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

struct ConfigWriter {
    current: Arc<Mutex<Config>>,
    wake: Option<SyncSender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ConfigWriter {
    fn new(config: &Config, path: PathBuf) -> Self {
        let current = Arc::new(Mutex::new(config.clone()));
        let snapshot = Arc::clone(&current);
        let (wake, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            while receiver.recv().is_ok() {
                let config = snapshot.lock().unwrap().clone();
                if let Err(error) = config.save(&path) {
                    eprintln!("settings save: {error}");
                }
            }
        });
        Self {
            current,
            wake: Some(wake),
            worker: Some(worker),
        }
    }

    fn request_save(&self, config: &Config) {
        *self.current.lock().unwrap() = config.clone();
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
    }

    fn finish(&mut self) {
        self.wake.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Mesher {
    jobs: SyncSender<MesherJob>,
    results: Receiver<ChunkMesh>,
}

struct MesherJob {
    chunk: Arc<Chunk>,
    known: HashMap<ChunkKey, Arc<Chunk>>,
    seed: u64,
    revision: u64,
    bounced_gi: bool,
}

impl Mesher {
    fn new() -> Self {
        let (jobs, jobs_rx) = mpsc::sync_channel::<MesherJob>(64);
        let (results_tx, results) = mpsc::sync_channel(64);
        let shared = Arc::new(Mutex::new(jobs_rx));
        for _ in 0..2 {
            let jobs_rx = Arc::clone(&shared);
            let results_tx = results_tx.clone();
            thread::spawn(move || {
                loop {
                    let job = match jobs_rx.lock().unwrap().recv() {
                        Ok(job) => job,
                        Err(_) => break,
                    };
                    let light = if job.bounced_gi {
                        LightField::build_with_bounce(job.chunk.key, &job.known, job.seed, true)
                    } else {
                        LightField::build(job.chunk.key, &job.known, job.seed)
                    };
                    if results_tx
                        .send(render::mesh_chunk_lit(&job.chunk, &light, job.revision))
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        Self { jobs, results }
    }
}

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
    network: Network,
    mesher: Mesher,
    config: Config,
    config_writer: ConfigWriter,
    screen: UiScreen,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    ui_layout: Option<UiLayout>,
    chunks: HashMap<ChunkKey, Arc<Chunk>>,
    pending_mesh: HashMap<ChunkKey, u64>,
    lighting_revisions: HashMap<ChunkKey, u64>,
    next_lighting_revision: u64,
    world_seed: Option<u64>,
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
    unacked: VecDeque<(u64, Vec3)>,
    frame_count: u64,
    last_report: Instant,
    frame_ms: Vec<f32>,
    disconnected: bool,
}

impl ClientApp {
    fn new(network: Network, config: Config, config_path: PathBuf) -> Self {
        let now = Instant::now();
        let effective_view_distance = config.view_distance;
        let config_writer = ConfigWriter::new(&config, config_path);
        Self {
            network,
            mesher: Mesher::new(),
            config,
            config_writer,
            screen: UiScreen::Playing,
            window: None,
            renderer: None,
            ui_layout: None,
            chunks: HashMap::new(),
            pending_mesh: HashMap::new(),
            lighting_revisions: HashMap::new(),
            next_lighting_revision: 1,
            world_seed: None,
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
            unacked: VecDeque::new(),
            frame_count: 0,
            last_report: now,
            frame_ms: Vec::with_capacity(512),
            disconnected: false,
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
        if slot < self.config.hotbar.len() && self.config.selected_slot != slot {
            self.config.selected_slot = slot;
            self.config_writer.request_save(&self.config);
        }
    }

    fn change_setting(&mut self, setting: SettingId, increase: bool) {
        let sign = if increase { 1.0 } else { -1.0 };
        match setting {
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
            UiControl::HotbarSlot(slot) if self.screen == UiScreen::Inventory => {
                self.select_slot(slot as usize)
            }
            UiControl::HotbarSlot(_) => {}
            UiControl::CatalogBlock(block)
                if self.screen == UiScreen::Inventory
                    && (1..=crate::world::MAX_BLOCK).contains(&block) =>
            {
                self.config.hotbar[self.config.selected_slot] = block;
                self.config_writer.request_save(&self.config);
            }
            UiControl::CatalogBlock(_) => {}
            UiControl::Resume => self.set_screen(UiScreen::Playing),
            UiControl::OpenSettings => self.set_screen(UiScreen::Settings),
            UiControl::Exit => event_loop.exit(),
            UiControl::Back => self.set_screen(UiScreen::Pause),
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
            UiScreen::Inventory => (0..9)
                .map(UiControl::HotbarSlot)
                .chain((1..=crate::world::MAX_BLOCK).map(UiControl::CatalogBlock))
                .collect(),
            UiScreen::Pause => vec![UiControl::Resume, UiControl::OpenSettings, UiControl::Exit],
            UiScreen::Settings => vec![
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
        if !self.pending_commands.is_empty() || !self.network.send(message.clone()) {
            if self.pending_commands.len() < 128 {
                self.pending_commands.push_back(message);
            } else {
                eprintln!("client command backlog exceeded limit; disconnecting");
                self.disconnected = true;
            }
        }
    }

    fn queue_relight(&mut self, key: ChunkKey, include_neighbors: bool) {
        let reach = if include_neighbors { 1 } else { 0 };
        for dy in -reach..=reach {
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let (Some(x), Some(y), Some(z)) = (
                        key.x.checked_add(dx),
                        key.y.checked_add(dy),
                        key.z.checked_add(dz),
                    ) else {
                        continue;
                    };
                    let affected = ChunkKey { x, y, z };
                    if !self.chunks.contains_key(&affected) {
                        continue;
                    }
                    let revision = self.next_lighting_revision;
                    self.next_lighting_revision =
                        self.next_lighting_revision.wrapping_add(1).max(1);
                    self.lighting_revisions.insert(affected, revision);
                    self.pending_mesh.insert(affected, revision);
                }
            }
        }
    }

    fn lighting_snapshot(&self, key: ChunkKey) -> HashMap<ChunkKey, Arc<Chunk>> {
        let mut known = HashMap::with_capacity(27);
        for dy in -1i32..=1 {
            for dz in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (Some(x), Some(y), Some(z)) = (
                        key.x.checked_add(dx),
                        key.y.checked_add(dy),
                        key.z.checked_add(dz),
                    ) else {
                        continue;
                    };
                    let neighbor = ChunkKey { x, y, z };
                    if let Some(chunk) = self.chunks.get(&neighbor) {
                        known.insert(neighbor, Arc::clone(chunk));
                    }
                }
            }
        }
        known
    }

    fn accept(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome { id, seed } => {
                self.world_seed = Some(seed);
                eprintln!("connected as player {id}, world seed {seed}")
            }
            ServerMessage::Position { ack_seq, x, y, z } => {
                while self.unacked.front().is_some_and(|(seq, _)| *seq <= ack_seq) {
                    self.unacked.pop_front();
                }
                self.position = Vec3::new(x, y, z);
                for (_, delta) in &self.unacked {
                    self.position += *delta;
                }
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
                            updated.blocks[index] = block;
                            updated.version = version;
                            self.queue_relight(key, true);
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
            ServerMessage::ViewDistance { radius } => {
                self.effective_view_distance = radius;
            }
            ServerMessage::Pong { .. } => {}
        }
    }

    fn poll_work(&mut self) {
        while let Some(message) = self.pending_commands.front().cloned() {
            if !self.network.send(message) {
                break;
            }
            self.pending_commands.pop_front();
        }
        for _ in 0..128 {
            match self.network.incoming.try_recv() {
                Ok(Incoming::Message(message)) => self.accept(message),
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
        self.lighting_revisions
            .retain(|key, _| self.chunks.contains_key(key));
        self.pending_mesh
            .retain(|key, _| self.chunks.contains_key(key));
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
                let Ok(mesh) = self.mesher.results.try_recv() else {
                    break;
                };
                if self
                    .chunks
                    .get(&mesh.key)
                    .is_some_and(|chunk| chunk.version == mesh.version)
                    && self.lighting_revisions.get(&mesh.key) == Some(&mesh.lighting_revision)
                {
                    self.pending_upload.push_back(mesh);
                }
            }
            while let Some(mesh) = self.pending_upload.pop_front() {
                if let Err(mesh) = renderer.enqueue_mesh(mesh) {
                    self.pending_upload.push_front(mesh);
                    break;
                }
            }
        }
        let Some(seed) = self.world_seed else {
            return;
        };
        for _ in 0..16 {
            let Some(key) = self.pending_mesh.keys().next().copied() else {
                break;
            };
            let revision = self.pending_mesh.remove(&key).unwrap();
            let Some(chunk) = self.chunks.get(&key).cloned() else {
                continue;
            };
            let job = MesherJob {
                chunk,
                known: self.lighting_snapshot(key),
                seed,
                revision,
                bounced_gi: self.config.bounced_gi,
            };
            if let Err(TrySendError::Full(_job)) = self.mesher.jobs.try_send(job) {
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
            self.position += delta;
            self.unacked.push_back((seq, delta));
        }
    }

    fn block_at(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        let (key, local) = crate::world::world_to_chunk(x, y, z);
        self.chunks.get(&key)?.block(local)
    }

    fn aimed_block(&self) -> Option<Hit> {
        let camera = self.camera();
        raycast::raycast(camera.position, camera.direction(), 7.0, |x, y, z| {
            self.block_at(x, y, z)
        })
    }

    fn edit_aimed_block(&mut self, place: bool) {
        if self.screen != UiScreen::Playing || !self.grabbed {
            return;
        }
        if let Some(hit) = self.aimed_block() {
            let block = self.config.hotbar[self.config.selected_slot];
            self.queue_command(edit_for_hit(hit, place, block));
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
            hotbar: self.config.hotbar,
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
            catalog_selection: self.config.hotbar[self.config.selected_slot],
            settings: UiSettings {
                sensitivity: self.config.sensitivity,
                fov_degrees: self.config.fov_degrees,
                view_distance: self.effective_view_distance,
                scale: self.config.scale,
                fullscreen: self.config.fullscreen,
                bounced_gi: self.config.bounced_gi,
            },
            hovered: self.focused_control,
        };
        if let Some(renderer) = &mut self.renderer {
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

impl ApplicationHandler for ClientApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Bloxgloom")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                match pollster::block_on(Renderer::new(Arc::clone(&window))) {
                    Ok(renderer) => {
                        self.renderer = Some(renderer);
                        self.window = Some(window);
                        self.refresh_layout();
                        self.apply_fullscreen();
                    }
                    Err(error) => {
                        eprintln!("renderer initialization: {error:?}");
                        event_loop.exit();
                    }
                }
            }
            Err(error) => {
                eprintln!("window creation: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|window| window.id() != id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
                self.refresh_layout();
            }
            WindowEvent::Focused(false) => {
                if self.screen == UiScreen::Playing {
                    self.set_screen(UiScreen::Pause);
                } else {
                    self.set_grab(false);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                if self.screen != UiScreen::Playing {
                    self.focused_control = self
                        .ui_layout
                        .as_ref()
                        .and_then(|layout| layout.hit_test(self.cursor.0, self.cursor.1));
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_down = modifiers.state().shift_key();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if pressed && !event.repeat {
                        if matches!(self.screen, UiScreen::Playing | UiScreen::Inventory)
                            && let Some(slot) = digit_slot(code)
                        {
                            self.select_slot(slot);
                            return;
                        }
                        match code {
                            KeyCode::Escape => {
                                self.on_escape();
                                return;
                            }
                            KeyCode::KeyE => {
                                self.toggle_inventory();
                                return;
                            }
                            KeyCode::F3 => {
                                self.config.debug_hud = !self.config.debug_hud;
                                self.config_writer.request_save(&self.config);
                                return;
                            }
                            KeyCode::Tab if self.screen != UiScreen::Playing => {
                                self.advance_focus(self.shift_down);
                                return;
                            }
                            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space
                                if self.screen != UiScreen::Playing =>
                            {
                                if let Some(control) = self.focused_control {
                                    self.activate_control(event_loop, control);
                                }
                                return;
                            }
                            KeyCode::ArrowLeft | KeyCode::ArrowRight
                                if self.screen == UiScreen::Settings =>
                            {
                                if let Some(
                                    UiControl::Decrease(setting) | UiControl::Increase(setting),
                                ) = self.focused_control
                                {
                                    self.change_setting(setting, code == KeyCode::ArrowRight);
                                }
                                return;
                            }
                            _ => {}
                        }
                    }
                    if self.screen != UiScreen::Playing {
                        return;
                    }
                    match code {
                        KeyCode::KeyW => self.keys.forward = pressed,
                        KeyCode::KeyS => self.keys.back = pressed,
                        KeyCode::KeyA => self.keys.left = pressed,
                        KeyCode::KeyD => self.keys.right = pressed,
                        KeyCode::Space => self.keys.up = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => self.keys.down = pressed,
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if self.screen != UiScreen::Playing {
                    if button == MouseButton::Left {
                        let control = self
                            .ui_layout
                            .as_ref()
                            .and_then(|layout| layout.hit_test(self.cursor.0, self.cursor.1));
                        if let Some(control) = control {
                            self.activate_control(event_loop, control);
                        }
                    }
                } else if !self.grabbed {
                    self.set_grab(true);
                } else if button == MouseButton::Left {
                    self.edit_aimed_block(false);
                } else if button == MouseButton::Right {
                    self.edit_aimed_block(true);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.screen == UiScreen::Playing => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                if y != 0.0 {
                    let shift = if y > 0.0 { -1 } else { 1 };
                    self.select_slot(
                        (self.config.selected_slot as i32 + shift).rem_euclid(9) as usize
                    );
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, event: DeviceEvent) {
        if self.screen == UiScreen::Playing
            && self.grabbed
            && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
        {
            self.yaw += dx as f32 * self.config.sensitivity;
            self.pitch = (self.pitch - dy as f32 * self.config.sensitivity).clamp(-1.55, 1.55);
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.config_writer.request_save(&self.config);
        self.config_writer.finish();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.disconnected {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        if now >= self.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.next_frame += FRAME;
            if self.next_frame <= now {
                self.next_frame = now + FRAME;
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}

pub fn run_client(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = Config::default_path();
    let config = Config::load(&config_path);
    let network = Network::connect(addr, config.view_distance)?;
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut ClientApp::new(network, config, config_path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raycast::Face;

    #[test]
    fn escape_and_inventory_transitions_preserve_menu_flow() {
        assert_eq!(escape_screen(UiScreen::Playing), UiScreen::Pause);
        assert_eq!(escape_screen(UiScreen::Pause), UiScreen::Playing);
        assert_eq!(escape_screen(UiScreen::Settings), UiScreen::Pause);
        assert_eq!(escape_screen(UiScreen::Inventory), UiScreen::Playing);
        assert_eq!(inventory_screen(UiScreen::Playing), UiScreen::Inventory);
        assert_eq!(inventory_screen(UiScreen::Inventory), UiScreen::Playing);
        assert_eq!(inventory_screen(UiScreen::Pause), UiScreen::Pause);
    }

    #[test]
    fn block_edit_uses_selected_hotbar_block_and_hit_face() {
        let hit = Hit {
            block: [2, 3, 4],
            adjacent: [1, 3, 4],
            block_id: 3,
            distance: 2.5,
            face: Face::NegX,
        };
        assert_eq!(
            edit_for_hit(hit, true, 1),
            ClientMessage::Edit {
                x: 1,
                y: 3,
                z: 4,
                block: 1
            }
        );
        assert_eq!(
            edit_for_hit(hit, false, 1),
            ClientMessage::Edit {
                x: 2,
                y: 3,
                z: 4,
                block: 0
            }
        );
    }

    #[test]
    fn client_cache_uses_server_view_radius() {
        let center = ChunkKey { x: -10, y: 4, z: 5 };
        assert!(chunk_in_view(
            ChunkKey {
                x: -16,
                y: 5,
                z: 11
            },
            center,
            6
        ));
        assert!(!chunk_in_view(
            ChunkKey {
                x: -16,
                y: 5,
                z: 11
            },
            center,
            3
        ));
        assert!(!chunk_in_view(ChunkKey { x: -10, y: 6, z: 5 }, center, 6));
    }
}
