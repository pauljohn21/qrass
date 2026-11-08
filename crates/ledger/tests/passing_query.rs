//! 「今天哪些过了」的读数。它必须存在，因为本库**只存失败**：一个 run 的通过集合没有行可列——而
//! 在没有那份清单的情况下写出的行为变更，可能毁掉一个本已通过的对（`no_operator::function/css`
//! 正是这个形状：语料自己要的是原样的 `calc(c())` 壳）。
//!
//! 在这里钉住，按每一条当初会骗到我的顺序：
//! • **阳性对照** —— 一个已知通过的用例必须被列出、一个已知失败的必须缺席；两样都返回空的查询
//!   读作「一切正常」（现实现排除了未知 key 被写入的 `0` 哨兵，所以一个 `0` id 在这里会显示为一个
//!   空 key）；
//! • **作用域陷阱** —— 一条 PASS 行只登记它的 key、不新增自己的行，所以一个被别的 run 量过（而本
//!   run 从没见过）的用例，与一个通过的用例无从区分。这就是为什么命令要求一个具名簇前缀而不是
//!   列出整个库；测试以记录本身唯一允许的形式陈述这个陷阱（是不是一个 run 覆盖了它，由语料说了
//!   算，不是本库）。

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
    // 阳性对照的另一半：一个谁都没记录过的名字必须什么也不返回，所以上面那一命中不可能是
    // 「什么都能匹配」的假象。
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
    // 一个 harness 量了两个用例；后一个从它的目录列表里丢了 `operators`（0.6 正是这段历史：
    // 它前一个 run 从没扫过这个簇）。
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
