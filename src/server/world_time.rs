//! Server-owned clock with periodic checkpoints on a dedicated I/O worker.
use crate::daylight::{CYCLE_MS, INITIAL_MS};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(super) struct Clock {
    initial: u64,
    started: Instant,
    last_publish: Instant,
    last_save: Instant,
    checkpoint_pending: bool,
    sender: Option<SyncSender<u64>>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl Clock {
    pub(super) fn open(root: &Path) -> io::Result<Self> {
        let path = root.join("world.time");
        let initial = match fs::read(&path) {
            Ok(bytes) if bytes.len() == 12 && &bytes[..4] == b"BGT1" => {
                let value = u64::from_le_bytes(bytes[4..].try_into().unwrap());
                if value >= CYCLE_MS {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid world time",
                    ));
                }
                value
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid world time",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => INITIAL_MS,
            Err(error) => return Err(error),
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("world-clock-save".into())
            .spawn(move || {
                while let Ok(time) = receiver.recv() {
                    save(&path, time)?;
                }
                Ok(())
            })?;
        let started = Instant::now();
        Ok(Self {
            initial,
            started,
            last_publish: started,
            last_save: started,
            checkpoint_pending: false,
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    pub(super) fn now(&self) -> u64 {
        (self.initial + (self.started.elapsed().as_millis() % u128::from(CYCLE_MS)) as u64)
            % CYCLE_MS
    }

    pub(super) fn poll(&mut self) -> Option<u64> {
        if self.checkpoint_pending || self.last_save.elapsed() >= Duration::from_secs(5) {
            self.checkpoint();
        }
        if self.last_publish.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.last_publish = Instant::now();
        Some(self.now())
    }

    fn checkpoint(&mut self) {
        if self
            .sender
            .as_ref()
            .is_some_and(|sender| sender.try_send(self.now()).is_ok())
        {
            self.last_save = Instant::now();
            self.checkpoint_pending = false;
        }
    }

    fn set(&mut self, time: u64) -> io::Result<()> {
        if time >= CYCLE_MS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid world time",
            ));
        }
        self.initial = time;
        self.started = Instant::now();
        self.last_publish = Instant::now();
        self.checkpoint_pending = true;
        self.checkpoint();
        Ok(())
    }

    pub(super) fn finish(&mut self) -> io::Result<()> {
        if let Some(sender) = self.sender.take() {
            // Shutdown may wait for the worker; normal ticks never wait on I/O.
            let _ = sender.send(self.now());
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("world clock worker panicked"))??;
        }
        Ok(())
    }
}

pub(super) fn set_time(state: &mut super::State, session: u64, time: u64) -> io::Result<()> {
    let Some(client) = state.clients.get(&session) else {
        return Ok(());
    };
    if state.admin_profile != Some(client.profile) {
        client.enqueue(crate::protocol::ServerMessage::EditRejected {
            reason: "Time requires administrator".into(),
        });
        return Ok(());
    }
    state.world_time.set(time)?;
    let elapsed_ms = state.world_time.now();
    for client in state.clients.values() {
        // Advisory updates are repaired by the next periodic sample.
        let _ = client
            .sender
            .try_send(crate::protocol::ServerMessage::WorldTime { elapsed_ms });
    }
    Ok(())
}

impl Drop for Clock {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("world clock save: {error}");
        }
    }
}

fn save(path: &Path, time: u64) -> io::Result<()> {
    use std::io::Write;
    let temporary: PathBuf = path.with_extension("time.tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(b"BGT1")?;
    file.write_all(&time.to_le_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()
}

#[cfg(test)]
#[path = "world_time/tests.rs"]
mod tests;
