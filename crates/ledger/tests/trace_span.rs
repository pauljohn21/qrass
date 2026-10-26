//! The `trace` feature must actually produce a trace, or it is a Cargo flag and nothing more.
//!
//! This matters more than the average feature test because the failure mode is silent in both
//! directions: a bridge that never exports leaves the operator with no output and no error, and a bridge
//! that replaces the ordinary subscriber would hide the reading (`NET`/`REGRESS`/`AB diff`) that the tool
//! exists to produce. So the assertion is both halves: a span **was** exported, and the plain verdict
//! line **still** printed.

#![cfg(feature = "trace")]

use ledger::record;
use std::path::PathBuf;
use std::process::Command;

fn scratch_db() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ledger-trace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("ledger.sqlite")
}

#[test]
fn a_traced_run_exports_a_span_and_still_prints_its_verdict() {
    let db = scratch_db();
    record(
        "one",
        &db,
        &["FAIL\tcalc/cluster::a\tgot \"x\" want \"y\"".to_string()],
    )
    .expect("record one");
    record("two", &db, &["PASS\tcalc/cluster::a".to_string()]).expect("record two");

    let database = format!("--db={}", db.display());
    let traced = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .args(["diff", "one", "two", "calc/cluster", database.as_str()])
        .output()
        .expect("run ledger");
    let stdout = String::from_utf8_lossy(&traced.stdout);

    assert!(
        stdout.contains("Span #"),
        "`--features trace` must export a span, and the exporter labels each one: {stdout}"
    );
    assert!(
        stdout.contains("command"),
        "the root span must carry which command ran: {stdout}"
    );
    assert!(
        stdout.contains("NET calc/cluster::a"),
        "the ordinary reading must survive the bridge: {stdout}"
    );
    assert!(
        traced.status.success(),
        "a traced run must not change the exit status"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
