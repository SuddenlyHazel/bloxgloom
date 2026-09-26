//! Paced, real-socket soak of the production nonblocking server listener.
//! Saves are isolated and disposable; no local player world is opened.

mod client;
mod report;

use super::fixture::TempSaveDir;
use crate::inventory::{Inventory, InventoryStore};
use crate::items::ItemId;
use crate::protocol::{self, ClientMessage};
use crate::server::metrics::TickSample;
use crate::server::net::{self, TransportStats};
use crate::server::{State, server_state_with_limit};
use client::{ClientHandle, ClientStats};
pub(in crate::server) use report::TcpSoakReport;
use std::io::{self, ErrorKind, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const SEED: u64 = 0xB10C_6100;
const WARMUP_TICKS: u64 = 700;
const PROFILE_BASE: u128 = 0xB10C_6000_0000_0000;

#[derive(Clone, Copy, Debug)]
pub(in crate::server) enum TcpScene {
    Clustered,
    Spread,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::server) struct TcpSoakConfig {
    pub clients: usize,
    pub ticks: usize,
    pub scene: TcpScene,
}

pub(in crate::server) fn run(config: TcpSoakConfig) -> io::Result<TcpSoakReport> {
    if !(1..=128).contains(&config.clients) || !(100..=15_000).contains(&config.ticks) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "TCP soak requires 1..=128 clients and 100..=15000 ticks",
        ));
    }
    let temp = TempSaveDir::create()?;
    seed_inventories(&temp.path, config.clients)?;
    let mut state = Box::new(server_state_with_limit(
        SEED,
        temp.path.clone(),
        (config.clients + 24).min(256),
    )?);
    // Only this disposable benchmark save requests an early checkpoint-gated
    // WAL rotation. Production servers retain their normal limit.
    state.durability.force_rotation_at_sequence = Some(20);
    let outbound = Arc::clone(&state.outbound);
    let (sample_tx, sample_rx) = mpsc::sync_channel(1024);
    state.tick_observer = Some(sample_tx);
    let latest_tick = Arc::new(AtomicU64::new(0));
    let measure_start = Arc::new(AtomicU64::new(u64::MAX));
    let samples = Arc::new(Mutex::new(Vec::with_capacity(config.ticks)));
    let collector = spawn_collector(
        sample_rx,
        Arc::clone(&latest_tick),
        Arc::clone(&measure_start),
        Arc::clone(&samples),
        config.ticks,
    )?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let transport = Arc::new(TransportStats::default());
    let mut server = ServerGuard::start(listener, state, Arc::clone(&transport))?;

    let result = exercise(config, address, &latest_tick, &measure_start);
    server.stop()?;
    collector
        .join()
        .map_err(|_| io::Error::other("tick collector panicked"))?;
    let (client_stats, reconnects) = result?;
    let samples = samples
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let report = report::summarize(
        config.clients,
        config.scene,
        config.ticks,
        &samples,
        &client_stats,
        transport.snapshot(),
        outbound.snapshot(),
        reconnects,
    );
    report.print();
    Ok(report)
}

fn seed_inventories(path: &std::path::Path, clients: usize) -> io::Result<()> {
    let store = InventoryStore::new(path)?;
    for index in 0..clients {
        let mut inventory = Inventory::default();
        if inventory.insert(ItemId::new(crate::world::STONE.get()), 64) != 0 {
            return Err(io::Error::other("failed to seed benchmark inventory"));
        }
        let bytes = InventoryStore::encode_snapshot(&inventory)?;
        store.checkpoint_snapshot(PROFILE_BASE + index as u128 + 1, &bytes)?;
    }
    Ok(())
}

fn spawn_collector(
    receiver: mpsc::Receiver<TickSample>,
    latest: Arc<AtomicU64>,
    start: Arc<AtomicU64>,
    samples: Arc<Mutex<Vec<TickSample>>>,
    ticks: usize,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("tcp-soak-ticks".into())
        .spawn(move || {
            while let Ok(sample) = receiver.recv() {
                latest.store(sample.tick_id, Ordering::Release);
                let first = start.load(Ordering::Acquire);
                if sample.tick_id >= first && sample.tick_id < first.saturating_add(ticks as u64) {
                    samples
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(sample);
                }
            }
        })
}

struct ServerGuard {
    stop: Option<SyncSender<()>>,
    handle: Option<JoinHandle<io::Result<()>>>,
}

impl ServerGuard {
    fn start(
        listener: TcpListener,
        state: Box<State>,
        stats: Arc<TransportStats>,
    ) -> io::Result<Self> {
        let (stop, receiver) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("tcp-soak-server".into())
            .spawn(move || net::serve_listener_with_stats(listener, state, receiver, stats))?;
        Ok(Self {
            stop: Some(stop),
            handle: Some(handle),
        })
    }

    fn stop(&mut self) -> io::Result<()> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.try_send(());
        }
        self.handle
            .take()
            .expect("server thread exists")
            .join()
            .map_err(|_| io::Error::other("TCP server panicked"))?
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.try_send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn exercise(
    config: TcpSoakConfig,
    address: SocketAddr,
    latest: &Arc<AtomicU64>,
    start: &Arc<AtomicU64>,
) -> io::Result<(Vec<ClientStats>, usize)> {
    let mut clients = Vec::with_capacity(config.clients);
    for index in 0..config.clients {
        clients.push(ClientHandle::connect(
            address,
            index,
            PROFILE_BASE + index as u128 + 1,
        )?);
    }
    for client in &mut clients {
        let _ = client.await_ready()?;
        client.send(&ClientMessage::SetView { radius: 2 })?;
    }
    let nuisance = open_nuisance_peers(address)?;
    let warmup_start = latest.load(Ordering::Acquire).saturating_add(1);
    drive(
        &mut clients,
        latest,
        warmup_start,
        WARMUP_TICKS,
        config.scene,
        false,
    )?;

    let reconnect_stop = Arc::new(AtomicBool::new(false));
    let reconnect_count = Arc::new(AtomicUsize::new(0));
    let reconnect_thread = spawn_reconnects(
        address,
        Arc::clone(&reconnect_stop),
        Arc::clone(&reconnect_count),
    )?;
    // Publish the capture window before its first tick can complete.
    let measured_start = latest.load(Ordering::Acquire).saturating_add(3);
    start.store(measured_start, Ordering::Release);
    let driven = drive(
        &mut clients,
        latest,
        measured_start,
        config.ticks as u64,
        config.scene,
        true,
    );
    reconnect_stop.store(true, Ordering::Release);
    reconnect_thread
        .join()
        .map_err(|_| io::Error::other("reconnect probe panicked"))?;
    driven?;
    // One final second lets durable results/ACKs and pickup events drain while
    // measurements remain exactly the requested steady tick interval.
    wait_until(latest, measured_start + config.ticks as u64 + 50)?;
    drop(nuisance);
    let mut results = Vec::with_capacity(config.clients);
    for client in clients {
        results.push(client.stop()?);
    }
    Ok((results, reconnect_count.load(Ordering::Relaxed)))
}

fn drive(
    clients: &mut [ClientHandle],
    latest: &AtomicU64,
    first: u64,
    ticks: u64,
    scene: TcpScene,
    actions: bool,
) -> io::Result<()> {
    let last = first + ticks - 1;
    let mut observed = first.saturating_sub(1);
    let mut progress_deadline = Instant::now() + Duration::from_secs(10);
    while observed < last {
        let current = latest.load(Ordering::Acquire).min(last);
        if current > observed {
            for tick in observed.max(first.saturating_sub(1)) + 1..=current {
                let offset = tick - first;
                if offset.is_multiple_of(5) {
                    for (index, client) in clients.iter_mut().enumerate() {
                        let [dx, dy, dz] = movement(scene, actions, offset, index);
                        client.send_move(dx, dy, dz)?;
                    }
                }
                if actions && offset > 0 && offset.is_multiple_of(100) {
                    let index = (offset / 100) as usize % clients.len().min(16);
                    clients[index].drop_one()?;
                }
                if actions && offset == 500 {
                    clients[0].edit_support_block()?;
                }
            }
            observed = current;
            progress_deadline = Instant::now() + Duration::from_secs(10);
        } else if Instant::now() > progress_deadline {
            return Err(io::Error::new(
                ErrorKind::TimedOut,
                "server tick stream stopped",
            ));
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }
    Ok(())
}

fn movement(scene: TcpScene, measured: bool, offset: u64, index: usize) -> [f32; 3] {
    if measured || matches!(scene, TcpScene::Clustered) {
        return [
            if (offset / 5 + index as u64).is_multiple_of(2) {
                0.05
            } else {
                -0.05
            },
            0.0,
            0.0,
        ];
    }
    // Clear terrain before diverging into eight regions. This is legal
    // server-authorized movement, not a benchmark-only teleport.
    if offset < 100 {
        return [0.0, 0.5, 0.0];
    }
    if offset >= 500 {
        return [0.0, -0.5, 0.0];
    }
    let (x, z) = match index % 8 {
        0 => (-0.5, 0.0),
        1 => (0.5, 0.0),
        2 => (0.0, -0.5),
        3 => (0.0, 0.5),
        4 => (-0.35, -0.35),
        5 => (0.35, -0.35),
        6 => (-0.35, 0.35),
        _ => (0.35, 0.35),
    };
    [x, 0.0, z]
}

fn wait_until(latest: &AtomicU64, tick: u64) -> io::Result<()> {
    let mut deadline = Instant::now() + Duration::from_secs(10);
    let mut previous = latest.load(Ordering::Acquire);
    while latest.load(Ordering::Acquire) < tick {
        let current = latest.load(Ordering::Acquire);
        if current > previous {
            previous = current;
            deadline = Instant::now() + Duration::from_secs(10);
        }
        if Instant::now() > deadline {
            return Err(io::Error::new(
                ErrorKind::TimedOut,
                "server tick stream stopped",
            ));
        }
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

fn open_nuisance_peers(address: SocketAddr) -> io::Result<Vec<TcpStream>> {
    let mut slow = Vec::with_capacity(16);
    for index in 0..8 {
        slow.push(client::active_slow_peer(
            address,
            PROFILE_BASE + 30_000 + index as u128,
            30_000 + index,
        )?);
    }
    for index in 0..8 {
        let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        socket.set_write_timeout(Some(Duration::from_secs(5)))?;
        protocol::write_client(
            &mut socket,
            &ClientMessage::Hello {
                name: format!("slow-{index}"),
                profile: PROFILE_BASE + 10_000 + index as u128,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )?;
        slow.push(socket);
    }
    for _ in 0..8 {
        let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        socket.write_all(&u32::MAX.to_le_bytes())?;
        let _ = socket.shutdown(Shutdown::Both);
    }
    Ok(slow)
}

fn spawn_reconnects(
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("tcp-soak-reconnect".into())
        .spawn(move || {
            for round in 0..16 {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if client::reconnect_probe(address, PROFILE_BASE + 20_000, 20_000 + round).is_ok() {
                    count.fetch_add(1, Ordering::Relaxed);
                }
                thread::sleep(Duration::from_secs(1));
            }
        })
}
