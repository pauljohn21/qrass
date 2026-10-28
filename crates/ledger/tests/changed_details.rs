//! The reading `ledger diff` structurally cannot give: a case that fails in **both** runs, but with a
//! different detail. Neither pass count nor `regress`/`net` moves for it, yet it is where an honest
//! verbatim row becomes a wrong value, or a wrong value becomes an invented error — the exact blind spot
//! that let one batch of this project add 6 invalid errors while reporting `regress=0`. Pinned here so the
//! gap is closed by a command, not by remembering to eyeball two listings.

use ledger::record;
use rusqlite::Connection;
use std::path::PathBuf;

fn scratch_db(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ledger-changed-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("ledger.sqlite")
}

#[test]
fn only_a_case_failing_both_runs_with_a_different_detail_is_listed() {
    let db = scratch_db("pair");
    record(
        "a",
        &db,
        &[
            "FAIL\tabs::simplification\tgot \"abs(1px + 2px)\" want \"2px\"".to_string(),
            "FAIL\tabs::overridden\tgot \"abs(-2)\" want \"-2\"".to_string(),
            "PASS\tabs::positive".to_string(),
        ],
    )
    .expect("record a");
    record(
        "b",
        &db,
        &[
            // detail changed: this is the row the reading must catch.
            "FAIL\tabs::simplification\tgot \"abs(2px)\" want \"2px\"".to_string(),
            // unchanged detail: an honest verbatim that stayed honest.
            "FAIL\tabs::overridden\tgot \"abs(-2)\" want \"-2\"".to_string(),
            // newly failing: that is `net`, and it is not this reading's business.
            "FAIL\tabs::positive\tunexpected error \"not a number\"".to_string(),
        ],
    )
    .expect("record b");
    let conn = Connection::open(&db).expect("open");

    let changed = ledger::changed_details(&conn, "a", "b", "").expect("query");
    assert_eq!(changed.rows.len(), 1, "got {:?}", changed.rows);
    assert!(
        changed.rows[0].starts_with("abs::simplification"),
        "got {:?}",
        changed.rows
    );
    // Both sides' text is in the line, so the reader can judge the *kind* of shape change.
    assert!(
        changed.rows[0].contains("abs(1px + 2px)") && changed.rows[0].contains("abs(2px)"),
        "got {}",
        changed.rows[0]
    );
    // The scope totals travel with the answer: they are what make a `changed = 0` mean something.
    assert_eq!(
        (changed.fails_a, changed.fails_b),
        (2, 3),
        "got {changed:?}"
    );

    // Scoped: the same answer inside the cluster, nothing outside it.
    assert_eq!(
        ledger::changed_details(&conn, "a", "b", "abs::")
            .expect("scoped")
            .rows
            .len(),
        1
    );
    assert_eq!(
        ledger::changed_details(&conn, "a", "b", "other/")
            .expect("unmatched scope")
            .rows
            .len(),
        0
    );
    // The guard the counts exist for: a scope where both runs fail **identically** reports zero changes
    // while still showing rows compared — the reading that distinguishes "no shape change" from "this
    // scope held nothing to look at", which is how a clean `regress=0` was once misread.
    let unchanged =
        ledger::changed_details(&conn, "a", "b", "abs::overridden").expect("same-detail scope");
    assert!(unchanged.rows.is_empty(), "got {:?}", unchanged.rows);
    assert_eq!(
        (unchanged.fails_a, unchanged.fails_b),
        (1, 1),
        "a matched scope must report its own totals, not silence"
    );
    // The positive control for the empty answer: comparing a run with itself must be zero because the
    // details are identical, not because the query found nothing to look at.
    let self_pair = ledger::changed_details(&conn, "b", "b", "").expect("self-pair");
    assert!(self_pair.rows.is_empty(), "got {:?}", self_pair.rows);
    assert_eq!((self_pair.fails_a, self_pair.fails_b), (3, 3));
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
