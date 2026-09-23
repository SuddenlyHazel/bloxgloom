//! Desktop client: network I/O and meshing stay off the window thread.
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::render::{self, Camera, ChunkMesh, Renderer};
use crate::world::{Chunk, ChunkKey};
use glam::Vec3;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const FRAME: Duration = Duration::from_nanos(16_666_667);
const SPEED: f32 = 8.0;
const MAX_CHUNKS: usize = 512;

enum Incoming {
    Message(ServerMessage),
    Closed(String),
}

struct Network {
    incoming: Receiver<Incoming>,
    outgoing: SyncSender<ClientMessage>,
}

impl Network {
    fn connect(addr: &str) -> io::Result<Self> {
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
            .send(ClientMessage::SetView { radius: 3 })
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

struct Mesher {
    jobs: SyncSender<Chunk>,
    results: Receiver<ChunkMesh>,
}

impl Mesher {
    fn new() -> Self {
        let (jobs, jobs_rx) = mpsc::sync_channel::<Chunk>(64);
        let (results_tx, results) = mpsc::sync_channel(64);
        let shared = Arc::new(Mutex::new(jobs_rx));
        for _ in 0..2 {
            let jobs_rx = Arc::clone(&shared);
            let results_tx = results_tx.clone();
            thread::spawn(move || {
                loop {
                    let chunk = match jobs_rx.lock().unwrap().recv() {
                        Ok(chunk) => chunk,
                        Err(_) => break,
                    };
                    if results_tx.send(render::mesh_chunk(&chunk)).is_err() {
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
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    chunks: HashMap<ChunkKey, Chunk>,
    pending_mesh: HashMap<ChunkKey, Chunk>,
    pending_upload: VecDeque<ChunkMesh>,
    pending_commands: VecDeque<ClientMessage>,
    position: Vec3,
    yaw: f32,
    pitch: f32,
    keys: Keys,
    grabbed: bool,
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
    fn new(network: Network) -> Self {
        let now = Instant::now();
        Self {
            network,
            mesher: Mesher::new(),
            window: None,
            renderer: None,
            chunks: HashMap::new(),
            pending_mesh: HashMap::new(),
            pending_upload: VecDeque::new(),
            pending_commands: VecDeque::new(),
            position: Vec3::new(0.5, 40.0, 0.5),
            yaw: 0.0,
            pitch: -0.2,
            keys: Keys::default(),
            grabbed: false,
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
            fov_y_radians: 70f32.to_radians(),
        }
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
        if let ClientMessage::Resync { key } = &message {
            if self.pending_commands.iter().any(|pending| matches!(pending, ClientMessage::Resync { key: pending_key } if pending_key == key)) {
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

    fn accept(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome { id, seed } => {
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
                self.pending_mesh.insert(key, chunk.clone());
                self.chunks.insert(key, chunk);
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
                            chunk.blocks[index] = block;
                            chunk.version = version;
                            self.pending_mesh.insert(key, chunk.clone());
                        }
                    } else if version > chunk.version {
                        self.queue_command(ClientMessage::Resync { key });
                    }
                } else {
                    self.queue_command(ClientMessage::Resync { key });
                }
            }
            ServerMessage::EditRejected { reason } => eprintln!("edit rejected: {reason}"),
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
        let mut evicted = Vec::new();
        self.chunks.retain(|key, _| {
            let keep = (i64::from(key.x) - i64::from(center.x)).abs() <= 4
                && (i64::from(key.y) - i64::from(center.y)).abs() <= 2
                && (i64::from(key.z) - i64::from(center.z)).abs() <= 4;
            if !keep {
                evicted.push(*key);
            }
            keep
        });
        self.pending_mesh
            .retain(|key, _| self.chunks.contains_key(key));
        if let Some(renderer) = &mut self.renderer {
            for key in evicted {
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
        for _ in 0..16 {
            let Some(key) = self.pending_mesh.keys().next().copied() else {
                break;
            };
            let chunk = self.pending_mesh.remove(&key).unwrap();
            if let Err(TrySendError::Full(chunk)) = self.mesher.jobs.try_send(chunk) {
                self.pending_mesh.insert(key, chunk);
                break;
            }
        }
        if self.chunks.len() > MAX_CHUNKS {
            eprintln!("client chunk cache exceeded target: {}", self.chunks.len());
        }
    }

    fn move_player(&mut self, dt: f32) {
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

    fn edit_aimed_block(&mut self, place: bool) {
        let camera = self.camera();
        let direction = camera.direction();
        let mut previous = None;
        let mut last = None;
        for step in 1..=70 {
            let point = camera.position + direction * (step as f32 * 0.1);
            let coord = (
                point.x.floor() as i32,
                point.y.floor() as i32,
                point.z.floor() as i32,
            );
            if last == Some(coord) {
                continue;
            }
            last = Some(coord);
            match self.block_at(coord.0, coord.1, coord.2) {
                Some(0) => previous = Some(coord),
                Some(_) => {
                    let target = if place { previous } else { Some(coord) };
                    if let Some((x, y, z)) = target {
                        self.queue_command(ClientMessage::Edit {
                            x,
                            y,
                            z,
                            block: if place { 2 } else { 0 },
                        });
                    }
                    return;
                }
                None => return,
            }
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.poll_work();
        self.move_player(dt);
        let camera = self.camera();
        if let Some(renderer) = &mut self.renderer {
            match renderer.render(camera) {
                Ok(stats) => {
                    self.frame_count += 1;
                    self.frame_ms.push(dt * 1000.0);
                    if now.duration_since(self.last_report) >= Duration::from_secs(5) {
                        let seconds = now.duration_since(self.last_report).as_secs_f32();
                        self.frame_ms.sort_by(f32::total_cmp);
                        let p95 = self.frame_ms[((self.frame_ms.len() - 1) * 95) / 100];
                        let p99 = self.frame_ms[((self.frame_ms.len() - 1) * 99) / 100];
                        eprintln!(
                            "{:.1} FPS | frame p95 {:.1} ms, p99 {:.1} ms | {} chunks visible | {} triangles | {} uploads, {} pending | {} cached chunks",
                            self.frame_count as f32 / seconds,
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
            }
            WindowEvent::Focused(false) => self.set_grab(false),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        KeyCode::KeyW => self.keys.forward = pressed,
                        KeyCode::KeyS => self.keys.back = pressed,
                        KeyCode::KeyA => self.keys.left = pressed,
                        KeyCode::KeyD => self.keys.right = pressed,
                        KeyCode::Space => self.keys.up = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => self.keys.down = pressed,
                        KeyCode::Escape if pressed => self.set_grab(false),
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if !self.grabbed {
                    self.set_grab(true);
                } else if button == MouseButton::Left {
                    self.edit_aimed_block(false);
                } else if button == MouseButton::Right {
                    self.edit_aimed_block(true);
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, event: DeviceEvent) {
        if self.grabbed {
            if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
                self.yaw += dx as f32 * 0.002;
                self.pitch = (self.pitch - dy as f32 * 0.002).clamp(-1.55, 1.55);
            }
        }
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
    let network = Network::connect(addr)?;
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut ClientApp::new(network))?;
    Ok(())
}
