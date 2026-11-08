//! `ledger diff` 在结构上给不出的读数：一个在**两个** run 里都失败、但细节不同的用例。通过数、
//! `regress`/`net` 都不为它动，而它正是「一个诚实的逐字行变成错值、或一个错值变成发明的错误」
//! 的所在地——本项目曾有一批在这样的盲区里加了 6 条无效错误还报着 `regress=0`。在这里钉住，
//! 好让缺口由一个命令闭合，而不是靠记着去目测两份清单。

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
            // 细节变了：这正是该读数必须抓住的那行。
            "FAIL\tabs::simplification\tgot \"abs(2px)\" want \"2px\"".to_string(),
            // 细节未变：一个保持诚实的逐字行。
            "FAIL\tabs::overridden\tgot \"abs(-2)\" want \"-2\"".to_string(),
            // 新失败的：那是 `net`，不属本读数的事。
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
    // 行里带着两侧的文本，读者才得以判断形状变化的*种类*。
    assert!(
        changed.rows[0].contains("abs(1px + 2px)") && changed.rows[0].contains("abs(2px)"),
        "got {}",
        changed.rows[0]
    );
    // 作用域总数随答案一起给：正是它们让 `changed = 0` 有意义。
    assert_eq!(
        (changed.fails_a, changed.fails_b),
        (2, 3),
        "got {changed:?}"
    );

    // 带作用域：同一答案只含簇内，簇外为零。
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
    // 这些计数存在的理由所在：一个两次运行都*完全相同*失败的作用域报告零变化，而仍显示比较过的行数
    // ——这正是区分「没有形状变化」与「这片作用域本就无可看」的读数，也是一次干净的 `regress=0` 曾
    // 被误读的方式。
    let unchanged =
        ledger::changed_details(&conn, "a", "b", "abs::overridden").expect("same-detail scope");
    assert!(unchanged.rows.is_empty(), "got {:?}", unchanged.rows);
    assert_eq!(
        (unchanged.fails_a, unchanged.fails_b),
        (1, 1),
        "a matched scope must report its own totals, not silence"
    );
    // 空答案的阳性对照：拿一个 run 与它自己比较必须为零——因为细节相同，而不是因为查询没找到东西
    // 可看。
    let self_pair = ledger::changed_details(&conn, "b", "b", "").expect("self-pair");
    assert!(self_pair.rows.is_empty(), "got {:?}", self_pair.rows);
    assert_eq!((self_pair.fails_a, self_pair.fails_b), (3, 3));
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
