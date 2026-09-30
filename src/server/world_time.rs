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
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    pub(super) fn now(&self) -> u64 {
        (self.initial + (self.started.elapsed().as_millis() % u128::from(CYCLE_MS)) as u64)
            % CYCLE_MS
    }

    pub(super) fn poll(&mut self) -> Option<u64> {
        if self.last_save.elapsed() >= Duration::from_secs(5)
            && self
                .sender
                .as_ref()
                .is_some_and(|sender| sender.try_send(self.now()).is_ok())
        {
            self.last_save = Instant::now();
        }
        if self.last_publish.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.last_publish = Instant::now();
        Some(self.now())
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
