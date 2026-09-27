//! Opt-in process-relative timestamps across admission, replication and display.
use std::sync::OnceLock;
use std::time::Instant;

pub(crate) fn event(message: std::fmt::Arguments<'_>) {
    static START: OnceLock<Option<Instant>> = OnceLock::new();
    if let Some(start) =
        START.get_or_init(|| std::env::var_os("BLOXGLOOM_TRACE_EDITS").map(|_| Instant::now()))
    {
        eprintln!(
            "edit-trace {:>10.3}ms {message}",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
