//! tracing subscriber bootstrap: logs to **stderr** so stdout stays pure CSS.

use std::sync::Once;

static INIT: Once = Once::new();

/// Install the fmt subscriber (env-filter driven, stderr writer). Idempotent.
pub fn init() {
    INIT.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("qrass=info"));
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .init();
    });
}
