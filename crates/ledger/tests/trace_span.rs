//! `trace` feature 必须真的产出一条 trace，否则它只是一个 Cargo flag 而已。
//!
//! 它比一般的 feature 测试更要紧，因为失败模式在两个方向上都是静默的：一个从不导出的桥让操作者
//! 既没有输出也没有错误，而一个替换掉普通 subscriber 的桥会藏起本工具存在的理由——那些读数
//! （`NET`/`REGRESS`/`AB diff`）。所以断言是两半都要：一个 span **已经**被导出，而普通的判定行
//! **仍然**打印。

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
