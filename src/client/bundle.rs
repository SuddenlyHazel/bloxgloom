//! Session artifact installation only: no extraction, image decode or script
//! execution. The cache holds one immutable, already verified bundle, independent
//! of the session. Reconnects re-offer identity; process restart downloads again.
use crate::protocol::{self, BundleIdentity, ClientMessage, MAX_BUNDLE_PART, ServerMessage};
use crate::server::client_bundle::ClientBundle;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod memory;
mod recovery;

static CACHE: Mutex<Option<Arc<ClientBundle>>> = Mutex::new(None);

// Tests asserting single-entry cache reuse must exclude concurrent production
// connects, which legitimately replace that entry with a different artifact.
#[cfg(test)]
pub(crate) static TEST_CACHE_LOCK: Mutex<()> = Mutex::new(());

/// The process cache intentionally retains an immutable artifact after session
/// retirement; only additional runtime references indicate a leaked worker.
#[cfg(test)]
pub(crate) fn session_references_released(bundle: &std::sync::Weak<ClientBundle>) -> bool {
    let cache = CACHE.lock().unwrap();
    let cached = cache
        .as_ref()
        .is_some_and(|cached| bundle.ptr_eq(&Arc::downgrade(cached)));
    bundle.strong_count() == usize::from(cached)
}

pub(super) fn install(
    socket: &mut TcpStream,
    identity: BundleIdentity,
    control: &super::join_worker::Control,
) -> io::Result<Arc<ClientBundle>> {
    let cached = CACHE.lock().unwrap().clone();
    let bundle = receive_progress(socket, identity, cached, |received, cached| {
        control.download(received, identity.total_len, cached)
    })?;
    *CACHE.lock().unwrap() = Some(Arc::clone(&bundle));
    Ok(bundle)
}

/// Publishes and acknowledges only a complete canonical verified artifact. A
/// failed/incomplete transfer never changes the cache or returns session data.
#[cfg(test)]
pub(crate) fn receive(
    socket: &mut TcpStream,
    identity: BundleIdentity,
    cached: Option<Arc<ClientBundle>>,
) -> io::Result<Arc<ClientBundle>> {
    receive_progress(socket, identity, cached, |_, _| Ok(()))
}

pub(crate) fn receive_progress(
    socket: &mut TcpStream,
    identity: BundleIdentity,
    cached: Option<Arc<ClientBundle>>,
    mut progress: impl FnMut(u32, bool) -> io::Result<()>,
) -> io::Result<Arc<ClientBundle>> {
    identity.validate()?;
    identity.require_supported_runtime()?;
    let mut stream = DeadlineStream {
        socket,
        deadline: Instant::now() + Duration::from_secs(30),
    };
    let bundle = if let Some(bundle) = cached.filter(|bundle| {
        bundle.cache_key() == identity.key && bundle.bytes().len() == identity.total_len as usize
    }) {
        progress(0, true)?;
        bundle
    } else {
        // Download storage, the canonical retained bytes and decoded payload
        // coexist during verification. Concurrent join workers share this
        // reservation; admission occurs before asking the server for bytes.
        let _memory = memory::reserve(identity.total_len as usize).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "client bundle download {}: {error}",
                    identity.key.cache_name()
                ),
            )
        })?;
        progress(0, false)?;
        protocol::write_client(&mut stream, &ClientMessage::BundleRequest { identity })?;
        let total = identity.total_len as usize;
        let mut bytes = Vec::with_capacity(total);
        while bytes.len() < total {
            let ServerMessage::BundlePart {
                offset,
                bytes: part,
            } = protocol::read_server(&mut stream)?
            else {
                return Err(invalid("expected bundle part before play"));
            };
            // Full-sized parts bound traversal as well as retained bytes: at
            // most ceil(MAX_BUNDLE_BYTES / MAX_BUNDLE_PART) frames.
            if offset as usize != bytes.len()
                || part.len() != (total - bytes.len()).min(MAX_BUNDLE_PART)
            {
                return Err(invalid("out-of-order or oversized bundle part"));
            }
            bytes.extend_from_slice(&part);
            progress(bytes.len() as u32, false)?;
        }
        Arc::new(
            recovery::decode(&CACHE, || ClientBundle::decode_verify(&bytes, identity.key))
                .map_err(|error| {
                    io::Error::other(format!(
                        "client bundle verification {}: {error}",
                        identity.key.cache_name()
                    ))
                })?,
        )
    };
    protocol::write_client(&mut stream, &ClientMessage::BundleReady { identity })?;
    Ok(bundle)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Per-read socket timeouts alone permit an indefinitely dribbling sender.
/// Recompute the remaining absolute budget before every OS read/write instead.
struct DeadlineStream<'a> {
    socket: &'a mut TcpStream,
    deadline: Instant,
}

impl DeadlineStream<'_> {
    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "client bundle transfer timed out")
            })
    }
}

impl Read for DeadlineStream<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(bytes)
    }
}

impl Write for DeadlineStream<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}
