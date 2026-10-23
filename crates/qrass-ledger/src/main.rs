//! Ledger CLI thin shell: tracing subscriber setup + argument plumbing; all
//! logic lives in the `qrass_ledger` library (shared with the hrx runner).

use rusqlite::Connection;
use std::path::Path;
use tracing::info;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = args
        .iter()
        .find(|a| a.starts_with("--db="))
        .map_or(qrass_ledger::DB, |a| a.trim_start_matches("--db="));
    let argv: Vec<String> = args.iter().filter(|a| !a.starts_with("--db=")).cloned().collect();
    match Connection::open(Path::new(db)).and_then(|conn| qrass_ledger::run(&conn, &argv)) {
        Ok(()) => {}
        Err(e) => info!(?e, "ledger error"),
    }
}
