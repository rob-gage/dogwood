// Copyright Rob Gage 2026

//! Process-wide tracing initialization for Dogwood applications.
//!
//! Debug builds log at `info` by default and accept standard `RUST_LOG` filters.
//! Optimized builds disable both initialization and tracing callsites.

#[cfg(debug_assertions)]
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Installs Dogwood's default process-wide tracing subscriber.
///
/// Returns a guard that must live until process shutdown so queued console output is flushed.
/// If the host already installed a subscriber, this leaves it in place and returns `None`.
#[cfg(debug_assertions)]
pub fn initialize() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter: EnvFilter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let (writer, guard) = tracing_appender::non_blocking(std::io::stderr());
    let console = tracing_subscriber::fmt::layer()
        .compact()
        .with_target(true)
        .with_writer(writer);
    tracing_subscriber::registry()
        .with(filter)
        .with(console)
        .try_init()
        .ok()
        .map(|()| guard)
}

/// Leaves tracing disabled in optimized builds.
#[cfg(not(debug_assertions))]
pub const fn initialize() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    None
}
