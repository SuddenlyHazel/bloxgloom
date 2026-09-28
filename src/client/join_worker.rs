//! One retained attempt, one result slot. Cancellation never admits another
//! attempt until the old worker has returned; abandoned results own Network's
//! normal socket/channel retirement. No process-global session registrations.
use super::{Config, Network};
use std::io;
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Progress {
    stage: &'static str,
    cancelled: bool,
    socket: Option<TcpStream>,
}

#[derive(Clone, Default)]
pub(super) struct Control(Arc<Mutex<Progress>>);

impl Control {
    pub(super) fn stage(&self, stage: &'static str) -> io::Result<()> {
        let mut progress = self.0.lock().unwrap();
        if progress.cancelled {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "join cancelled"));
        }
        progress.stage = stage;
        Ok(())
    }

    pub(super) fn attach(&self, socket: &TcpStream) -> io::Result<()> {
        let mut progress = self.0.lock().unwrap();
        if progress.cancelled {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "join cancelled"));
        }
        progress.socket = Some(socket.try_clone()?);
        Ok(())
    }

    fn cancel(&self) {
        let mut progress = self.0.lock().unwrap();
        progress.cancelled = true;
        progress.stage = "cancelling; waiting for preparation worker";
        if let Some(socket) = progress.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    pub(super) fn label(&self) -> &'static str {
        self.0.lock().unwrap().stage
    }
}

pub(super) struct Prepared {
    pub(super) network: Network,
    pub(super) config: Config,
}

pub(super) struct Attempt {
    pub(super) control: Control,
    result: mpsc::Receiver<io::Result<Prepared>>,
    started: Instant,
    cancelled: bool,
    worker: Option<std::thread::JoinHandle<()>>,
    #[cfg(test)]
    stopped: Option<mpsc::Receiver<()>>,
}

impl Attempt {
    pub(super) fn start(
        address: String,
        path: PathBuf,
        prior_writer: Option<super::ConfigWriter>,
    ) -> io::Result<Self> {
        Self::spawn(move |control| {
            if let Some(mut writer) = prior_writer {
                // Even a cancelled attempt must finish this predecessor's save
                // before a later retry may load the same profile/settings file.
                let _ = control.stage("saving previous session settings");
                writer.finish();
            }
            control.stage("loading player settings")?;
            let mut config = Config::load(&path);
            config.ensure_profile(&path)?;
            let network = Network::connect_controlled(
                &address,
                config.view_distance,
                config.profile,
                control,
            )?;
            super::appearance::apply_environment(&network)?;
            Ok(Prepared { network, config })
        })
    }

    fn spawn(
        work: impl FnOnce(&Control) -> io::Result<Prepared> + Send + 'static,
    ) -> io::Result<Self> {
        let control = Control::default();
        control.stage("connecting")?;
        let worker_control = control.clone();
        let (sender, result) = mpsc::sync_channel(1);
        #[cfg(test)]
        let (stopped_tx, stopped) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("client-join".into())
            .spawn(move || {
                let result = work(&worker_control);
                // This clone must not retain a retired session's socket.
                worker_control.0.lock().unwrap().socket = None;
                let _ = sender.send(result);
                #[cfg(test)]
                let _ = stopped_tx.send(());
            })?;
        Ok(Self {
            control,
            result,
            started: Instant::now(),
            cancelled: false,
            worker: Some(worker),
            #[cfg(test)]
            stopped: Some(stopped),
        })
    }

    pub(super) fn cancel(&mut self) {
        self.cancelled = true;
        self.control.cancel();
    }

    /// Only after the window/event loop has closed. Interactive cancellation
    /// uses poll and never waits on a thread, DNS, or filesystem operation.
    pub(super) fn finish(mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    pub(super) fn poll(&mut self) -> Option<io::Result<Prepared>> {
        if self.started.elapsed() >= Duration::from_secs(60) {
            self.cancel();
        }
        // A result is sent just before the OS thread exits. Do not admit a new
        // attempt while even that completion tail still owns a worker slot.
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return None;
        }
        if let Some(worker) = self.worker.take() {
            // is_finished above makes this a non-waiting resource reap.
            let _ = worker.join();
        }
        match self.result.try_recv() {
            Ok(result) => Some(self.complete(result)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(io::Error::other("Join worker stopped")))
            }
        }
    }

    fn complete(&self, result: io::Result<Prepared>) -> io::Result<Prepared> {
        if self.cancelled {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Join cancelled or timed out",
            ))
        } else {
            result
        }
    }

    #[cfg(test)]
    pub(super) fn wait_for_test(&self) -> io::Result<Prepared> {
        self.complete(
            self.result
                .recv_timeout(Duration::from_secs(15))
                .expect("join worker deadline"),
        )
    }

    #[cfg(test)]
    pub(super) fn wait_finished_for_test(&mut self) {
        self.stopped
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .expect("join worker completion deadline");
        self.worker.take().unwrap().join().unwrap();
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) use tests::abandoned_result_retires_transport;
