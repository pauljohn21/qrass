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

/// The argument split behind `--count`, pinned on its own because its failure mode is silent: if the flag
/// were read as the needle, the query would answer “zero failures” and a gated step would pass on a
/// measurement that never happened (`ledger failures <run> "unexpected error"` is exactly the reading
/// 1.4/1.5 are gated on).
#[test]
fn count_flag_is_a_flag_and_never_a_pattern() {
    let args = |words: &[&str]| {
        words
            .iter()
            .map(|w| (*w).to_string())
            .collect::<Vec<String>>()
    };

    assert_eq!(
        ledger::failure_query(&args(&["run", "unexpected error", "--count"])),
        (
            "run".to_string(),
            vec!["unexpected error".to_string()],
            true
        ),
    );
    // The same query without the flag lists rows instead of counting them.
    assert_eq!(
        ledger::failure_query(&args(&["run", "unexpected error"])),
        (
            "run".to_string(),
            vec!["unexpected error".to_string()],
            false
        ),
    );
    // A run with no pattern at all: no needles, and `--count` still must not become one.
    assert_eq!(
        ledger::failure_query(&args(&["run", "--count"])),
        ("run".to_string(), vec![], true),
    );
    // The positive control for the above: a needle that really is absent is still carried as a needle, so
    // “empty” above means the query was understood, not that the flag silently matched nothing.
    assert_eq!(
        ledger::failure_query(&args(&["run", "no/such/cluster"])),
        (
            "run".to_string(),
            vec!["no/such/cluster".to_string()],
            false
        ),
    );
    // Two needles are two needles, in order — the shape the terrain readings need.
    assert_eq!(
        ledger::failure_query(&args(&[
            "run",
            "core_functions/color/hwb",
            "expected error",
            "--count"
        ])),
        (
            "run".to_string(),
            vec![
                "core_functions/color/hwb".to_string(),
                "expected error".to_string()
            ],
            true
        ),
    );
}

/// Why multiple needles must be ANDed by the tool rather than concatenated by hand: joining `"cluster"
/// "expected error"` into one needle asks SQL for a substring no row contains, so it answers **zero** — and a
/// zero that looks like a measurement is how a whole cluster got written off as “all of these want a
/// diagnostic” or “none of these do” in the same session.
#[test]
fn every_narrower_needle_keeps_only_rows_matching_both() {
    let db = scratch_db("and");
    let rows = vec![
        "FAIL\tcore_functions/color/hwb::three_args\tgot \"a\" want \"b\"".to_string(),
        "FAIL\tcore_functions/color/hwb::error/units\texpected error \"Error: $whiteness\""
            .to_string(),
        "FAIL\tcore_functions/color/mix::alpha\texpected error \"Error: 2 arguments\"".to_string(),
        "PASS\tsorted/case::a".to_string(),
    ];
    record("r", &db, &rows).expect("record");
    let conn = Connection::open(&db).expect("open");
    let hit = |needles: &[&str]| -> Vec<String> {
        let owned = needles
            .iter()
            .map(|n| (*n).to_string())
            .collect::<Vec<String>>();
        ledger::failing_any(&conn, "r", &owned).expect("query")
    };

    // The cluster alone: both hwb failures, and nothing from another cluster.
    assert_eq!(hit(&["core_functions/color/hwb"]).len(), 2);
    // The outcome class alone crosses clusters.
    assert_eq!(hit(&["expected error"]).len(), 2);
    // Anded: exactly the hwb rows that want a diagnostic — the reading a slice is chosen with.
    let both = hit(&["core_functions/color/hwb", "expected error"]);
    assert_eq!(both.len(), 1, "got {both:?}");
    assert!(
        both[0].starts_with("core_functions/color/hwb::error/units"),
        "got {both:?}"
    );
    // The failure mode this pins: one needle holding both words matches nothing, and looks like an answer.
    assert_eq!(
        hit(&["core_functions/color/hwb expected error"]).len(),
        0,
        "a concatenated needle must not be the way this is queried"
    );
    // A second needle is order-insensitive in effect: the same single row survives either way.
    assert_eq!(
        hit(&["expected error", "core_functions/color/hwb"]).len(),
        both.len()
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
