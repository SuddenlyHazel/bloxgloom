use super::*;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn capture(specification: Option<&str>, emit: impl FnOnce()) -> String {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let (writer, guard) = NonBlockingBuilder::default().finish(Buffer(Arc::clone(&bytes)));
    let (filter, _) = filter(specification);
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .without_time()
        .with_writer(writer)
        .finish();
    tracing::subscriber::with_default(subscriber, emit);
    // Shutdown must drain queued records, including the final failure.
    drop(guard);
    String::from_utf8(bytes.lock().unwrap().clone()).unwrap()
}

#[test]
fn default_filter_preserves_structured_game_events_and_flushes_final_errors() {
    let output = capture(None, || {
        let _span = tracing::info_span!("server", listen_addr = "127.0.0.1:4000").entered();
        tracing::info!(target: "bloxgloom::server", player_id = 7, "player joined");
        tracing::debug!(target: "bloxgloom::client", "frame statistics");
        tracing::info!(target: "wgpu", "adapter details");
        tracing::warn!(target: "wgpu", "device warning");
        tracing::error!(target: "bloxgloom", error = "disk full", "final failure");
    });
    assert!(output.contains("player joined"));
    assert!(output.contains("player_id=7"));
    assert!(output.contains("listen_addr=\"127.0.0.1:4000\""));
    assert!(output.contains("device warning"));
    assert!(output.contains("final failure"));
    assert!(output.contains("error=\"disk full\""));
    assert!(!output.contains("frame statistics"));
    assert!(!output.contains("adapter details"));
}

#[test]
fn scoped_filters_and_invalid_filter_fallback_work_without_global_state() {
    let output = capture(Some("off,bloxgloom::client=debug"), || {
        tracing::debug!(target: "bloxgloom::client", "client details");
        tracing::info!(target: "bloxgloom::server", "server details");
    });
    assert!(output.contains("client details"));
    assert!(!output.contains("server details"));
    assert!(filter(Some("bloxgloom=invalid_level")).1.is_some());
    let fallback = capture(Some("bloxgloom=invalid_level"), || {
        tracing::info!(target: "bloxgloom::server", "fallback event");
    });
    assert!(fallback.contains("fallback event"));
}
