//! 带作用域的 A/B。计划的回归策略是逐簇的（`tasks.md`：每簇维护自己基线），所以 `diff_runs` 必须
//! 回答*那个*问题——否则每一道簇级闸都会在 shell 里用导出 key 列表上的 `sort | comm` 重建，那正是
//! 本 crate 取代的 `reports/hrx-*.tsv` 蔓延，只不过换到 `/tmp` 里。
//!
//! 先钉住的陷阱：一个匹配不到任何 key 的作用域同样报 `regress=0 net=0`。零回归只有在作用域真的含
//! 有用例时才是判定——`fails_a`/`fails_b` 就是为此而在。

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
            // 本增量所属的那一簇：一个用例此前失败……
            "FAIL\tvalues/calculation/calc/no_operator::number/integer\tgot \"calc(1px)\" want \"1px\"".to_string(),
            // ……以及同簇里另一个不受它影响的。
            "FAIL\tvalues/calculation/calc/no_operator::operator\tgot \"a\" want \"b\"".to_string(),
        ],
    )
    .expect("record base");
    record(
        "after",
        &db,
        &[
            // 修好的用例只是从失败集里消失
            "FAIL\tvalues/calculation/calc/no_operator::operator\tgot \"a\" want \"b\"".to_string(),
            // 别处的回归不得污染本簇的判定
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

    // 阳性对照：同样的两次运行，用 key 真正所在的作用域，就看得见那个用例。
    let seen = diff_runs(&conn, "base", "after", "sass/one").expect("query");
    assert_eq!(
        (seen.fails_a, seen.fails_b),
        (1, 1),
        "the positive control must count the case"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
