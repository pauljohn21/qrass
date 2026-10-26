//! Scoped A/B. The plan's regress policy is per cluster (`tasks.md`: 每簇维护自己基线), so `diff_runs`
//! must answer *that* question — otherwise every cluster-level gate gets rebuilt in the shell with
//! `sort | comm` over exported key lists, which is the `reports/hrx-*.tsv` sprawl this crate replaced,
//! just in `/tmp`.
//!
//! The trap pinned first: a scope that matches no key reports `regress=0 net=0` too. Zero regressions is
//! only a verdict if the scope actually contained cases, which is what `fails_a`/`fails_b` are for.

use ledger::{diff_runs, record};
use rusqlite::Connection;
use std::path::PathBuf;

fn scratch_db(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ledger-scoped-diff-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("ledger.sqlite")
}

#[test]
fn a_scope_restricts_the_verdict_to_one_cluster() {
    let db = scratch_db("cluster");
    record(
        "base",
        &db,
        &[
            // the cluster this increment owns: one case fails before…
            "FAIL\tvalues/calculation/calc/no_operator::number/integer\tgot \"calc(1px)\" want \"1px\"".to_string(),
            // …and another one in the same cluster that is untouched by it.
            "FAIL\tvalues/calculation/calc/no_operator::operator\tgot \"a\" want \"b\"".to_string(),
        ],
    )
    .expect("record base");
    record(
        "after",
        &db,
        &[
            // the fixed case is simply absent from the failure set
            "FAIL\tvalues/calculation/calc/no_operator::operator\tgot \"a\" want \"b\"".to_string(),
            // a regression somewhere else must NOT pollute this cluster's verdict
            "PASS\tvalues/calculation/calc/no_operator::number/integer".to_string(),
            "FAIL\tcore_functions/math/div::unit\tsomething broke here".to_string(),
        ],
    )
    .expect("record after");
    let conn = Connection::open(&db).expect("open");

    let whole = diff_runs(&conn, "base", "after", "").expect("whole diff");
    assert_eq!(whole.net.len(), 1, "one case gained overall: {whole:?}");
    assert_eq!(
        whole.regress.len(),
        1,
        "the other cluster's regression must show up unscoped: {:?}",
        whole.regress
    );

    let scoped = diff_runs(&conn, "base", "after", "values/calculation").expect("scoped diff");
    assert_eq!(
        scoped.net,
        vec!["values/calculation/calc/no_operator::number/integer".to_string()],
        "the cluster's own gain, and nothing else: {:?}",
        scoped.net
    );
    assert!(
        scoped.regress.is_empty(),
        "this cluster regressed nothing: {:?}",
        scoped.regress
    );
    assert_eq!(
        (scoped.fails_a, scoped.fails_b),
        (2, 1),
        "the in-scope failing-set sizes are what make regress=0 mean something"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}

#[test]
fn an_unmatched_scope_is_not_mistaken_for_a_clean_verdict() {
    let db = scratch_db("empty-scope");
    record(
        "base",
        &db,
        &["FAIL\tsass/one::a\tgot \"x\" want \"y\"".to_string()],
    )
    .expect("record base");
    record(
        "after",
        &db,
        &["FAIL\tsass/one::a\tgot \"x\" want \"y\"".to_string()],
    )
    .expect("record after");
    let conn = Connection::open(&db).expect("open");

    let quiet = diff_runs(&conn, "base", "after", "sass/nothing_here").expect("query");
    assert!(
        quiet.regress.is_empty() && quiet.net.is_empty(),
        "a scope with no keys reports nothing: {quiet:?}"
    );
    assert_eq!(
        (quiet.fails_a, quiet.fails_b),
        (0, 0),
        "zero counts inside the scope are the only signal that this reading measured nothing"
    );

    // Positive control: the same runs, with the scope the keys actually live in, do see the case.
    let seen = diff_runs(&conn, "base", "after", "sass/one").expect("query");
    assert_eq!(
        (seen.fails_a, seen.fails_b),
        (1, 1),
        "the positive control must count the case"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
