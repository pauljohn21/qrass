//! The reading `ledger changed` leaves to the operator: of the rows that fail in **both** runs with a
//! different detail, which moved **toward** the archive and which moved **away**.
//!
//! ```text
//! cargo run -q -p ledger --bin shift -- <base-run> <this-run> [key-prefix] [--db <path>]
//! ```
//!
//! `changed` prints both details and stops there; the qualification it demands was until now made by
//! eyeballing, which is how one batch of this project described 816 shape-changed rows as progress while
//! rows of its own cluster had gone *further* from the golden. This command turns the split into a reading
//! with a positive control (`crates/ledger/tests/shift_split.rs`), and it lists by key the two buckets a
//! count must not absorb: rows that moved away, and rows that changed **kind** (a verbatim that became a
//! computed value, an answer that became an error). The classifier lives in `src/shift_impl.rs` for that
//! reason; see [`ledger::changed_details`] for the query it reads through.

use rusqlite::Connection;
use std::path::PathBuf;

#[path = "../shift_impl.rs"]
mod shift_impl;

use crate::shift_impl::split_of;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // `--db <path>` splits the arguments from the positionals; the positional shape is
    // `<base> <this> [prefix]`, the same scope convention `changed` itself takes.
    let (positionals, db) = match argv.iter().position(|arg| arg == "--db") {
        Some(at) => match argv.get(at + 1) {
            Some(path) => (
                argv.iter().take(at).cloned().collect::<Vec<String>>(),
                PathBuf::from(path),
            ),
            None => {
                tracing::info!("--db needs a path after it");
                return Ok(());
            }
        },
        None => (argv, PathBuf::from(ledger::DB)),
    };
    let [base, this, prefix @ ..] = positionals.as_slice() else {
        tracing::info!("usage: ledger shift <base-run> <this-run> [key-prefix] [--db <path>]");
        return Ok(());
    };
    let prefix = prefix.first().cloned().unwrap_or_default();
    let conn = Connection::open(&db)?;
    let changed = ledger::changed_details(&conn, base, this, &prefix)?;
    let split = split_of(&changed.rows);
    split
        .farther_keys
        .iter()
        .for_each(|key| tracing::info!(key = %key, "FARTHER"));
    split
        .error_keys
        .iter()
        .for_each(|key| tracing::info!(key = %key, "ERROR"));
    split
        .shape_keys
        .iter()
        .for_each(|key| tracing::info!(key = %key, "SHAPE"));
    tracing::info!(
        base,
        this,
        scope = if prefix.is_empty() {
            "*"
        } else {
            prefix.as_str()
        },
        fails_a = changed.fails_a,
        fails_b = changed.fails_b,
        changed = changed.rows.len(),
        closer = split.closer,
        farther = split.farther,
        tie = split.tie,
        error_expectation = split.error_expectation,
        shape_of_text = split.shape_of_text,
        "distance split of the shape-changed rows"
    );
    Ok(())
}
