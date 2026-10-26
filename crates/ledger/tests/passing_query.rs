//! The “what passes today” reading. Needed because the ledger stores **only failures**, so a run's
//! passing set has no row to list — and a behaviour change authored without that list can destroy a case
//! that already passes (the exact shape of `no_operator::function/css`, where the corpus itself wants the
//! untouched `calc(c())` shell).
//!
//! Pinned here, in the order each one would have fooled me:
//! • a **positive control** — a case known to pass must be listed and a known failure must be absent; a
//!   query that returns nothing for both reads as a clean slate (the current implementation excludes the
//!   `0` sentinel an unknown key gets written with, so a `0` id would surface as an empty key here);
//! • the **scope trap** — a PASS row registers its key but grows no row of its own, so a case another run
//!   measured (and this run never saw) is indistinguishable from a passing one. That is why the command
//!   takes a required cluster prefix instead of listing the whole ledger; the test states the trap in the
//!   only form the record itself allows (the corpus, not the ledger, decides whether a run covered it).

use ledger::{passing, record};
use rusqlite::Connection;
use std::path::PathBuf;

fn scratch_db(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ledger-passing-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("ledger.sqlite")
}

#[test]
fn a_currently_passing_case_is_listed_and_a_failing_one_is_not() {
    let db = scratch_db("verdicts");
    record(
        "base",
        &db,
        &[
            "PASS\tcalc/cluster::already_right".to_string(),
            "FAIL\tcalc/cluster::to_fix\tgot \"x\" want \"y\"".to_string(),
            "PASS\tother/cluster::untouched".to_string(),
        ],
    )
    .expect("record base");
    let conn = Connection::open(&db).expect("open");

    let listed = passing(&conn, "base", "calc/cluster").expect("query");
    assert_eq!(
        listed,
        vec!["calc/cluster::already_right".to_string()],
        "the passing case must be listed and the failing one must not: {listed:?}"
    );
    // The positive control's other half: a name nothing ever recorded yields nothing, so the single hit
    // above cannot be an “everything matches” artefact.
    assert!(
        passing(&conn, "base", "calc/nothing_here")
            .expect("query")
            .is_empty(),
        "an unrecorded key must not be reported as passing"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}

#[test]
fn a_key_recorded_by_another_run_reads_as_passing_here() {
    let db = scratch_db("scope");
    // One harness measured both cases; the later one lost `operators` from its directory list (0.6 is
    // exactly this history: the run before it never scanned the cluster).
    record(
        "wide",
        &db,
        &[
            "PASS\toperators/plus::syntax/a".to_string(),
            "FAIL\toperators/minus::syntax/b\tgot \"x\" want \"y\"".to_string(),
        ],
    )
    .expect("record wide");
    record(
        "narrow",
        &db,
        &["FAIL\tcalc/cluster::c\tgot \"x\" want \"y\"".to_string()],
    )
    .expect("record narrow");
    let conn = Connection::open(&db).expect("open");

    assert_eq!(
        passing(&conn, "narrow", "operators/plus").expect("query"),
        vec!["operators/plus::syntax/a".to_string()],
        "the ledger cannot tell ‘passed’ from ‘never measured’ — this is why the cluster prefix is \
         mandatory and coverage stays the caller's question to answer from the corpus"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
