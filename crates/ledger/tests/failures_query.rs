//! The failure query must actually filter. Both bugs this pins are silent: a broken `LIKE` binding
//! returned zero rows for a substring that the unfiltered listing plainly contained, and the discarded
//! query error made that indistinguishable from “this run has no such failure” — on a measuring tool the
//! worst possible confusion, since `unexpected error` counts are how invented diagnostics are caught
//! (a FAIL that becomes another FAIL is invisible in the pass count).

use ledger::record;
use rusqlite::Connection;
use std::path::PathBuf;

fn scratch_db(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ledger-failures-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("ledger.sqlite")
}

#[test]
fn failing_matches_key_or_detail() {
    let db = scratch_db("query");
    let rows = vec![
        "PASS\tsorted/case::a".to_string(),
        "FAIL\tmath/div::bad\tunexpected error \"Undefined variable.\"".to_string(),
        "FAIL\tother/case::b\tgot \"x\" want \"y\"".to_string(),
    ];
    record("r", &db, &rows).expect("record");
    let conn = Connection::open(&db).expect("open");
    let hit = |needle: &str| ledger::failing(&conn, "r", needle).expect("query");

    // By detail — the shape the pass count can never show.
    let by_detail = hit("unexpected error");
    assert_eq!(
        by_detail.len(),
        1,
        "detail filter matched the wrong set: {by_detail:?}"
    );
    assert!(
        by_detail[0].starts_with("math/div::bad"),
        "got {by_detail:?}"
    );

    // By key, which is how the metric gets bucketed per cluster.
    let by_key = hit("other/case");
    assert_eq!(
        by_key.len(),
        1,
        "key filter matched the wrong set: {by_key:?}"
    );
    assert!(by_key[0].starts_with("other/case::b"), "got {by_key:?}");

    // No pattern lists every failure of the run — passes are never stored.
    assert_eq!(hit("").len(), 2, "an empty pattern must list all failures");
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
