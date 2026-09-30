//! One process-wide subscriber for the client, server and their worker threads.

use std::io::IsTerminal;
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "warn,bloxgloom=info";

/// Keep the returned guard alive until all client/server workers have stopped.
pub(crate) fn init() -> Result<WorkerGuard, Box<dyn std::error::Error + Send + Sync>> {
    let specification = std::env::var("RUST_LOG").ok();
    let (filter, invalid_filter) = filter(specification.as_deref());
    let stderr = std::io::stderr();
    let ansi = stderr.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    // A bounded, lossy queue keeps a slow terminal from stalling gameplay.
    let (writer, guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(4_096)
        .lossy(true)
        .thread_name("bloxgloom-logging")
        .finish(stderr);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(ansi)
        .with_thread_names(true)
        .with_writer(writer)
        .try_init()?;
    if let Some(error) = invalid_filter {
        tracing::warn!(%error, fallback = DEFAULT_FILTER, "invalid RUST_LOG; using default filter");
    }
    Ok(guard)
}

fn filter(
    specification: Option<&str>,
) -> (EnvFilter, Option<tracing_subscriber::filter::ParseError>) {
    let builder = EnvFilter::builder().with_regex(false);
    match builder.parse(specification.unwrap_or(DEFAULT_FILTER)) {
        Ok(filter) => (filter, None),
        Err(error) => (EnvFilter::new(DEFAULT_FILTER), Some(error)),
    }
}

#[cfg(test)]
mod tests;
