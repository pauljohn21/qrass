//! `docsizer` CLI shell: tracing setup and argument plumbing; all logic lives in the library.

use tracing::info;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let span = tracing::info_span!("docsizer", cmd = args.first().map_or("", |a| a.as_str()));
    let _guard = span.enter();

    match docsizer::run(&args) {
        Ok(()) => {}
        Err(e) => {
            info!(error = %e, "docsizer failed");
            std::process::exit(1);
        }
    }
}
