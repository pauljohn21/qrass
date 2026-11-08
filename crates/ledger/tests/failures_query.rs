//! 失败查询必须真的过滤。这里钉的两个 bug 都是静默的：一个坏掉的 `LIKE` 绑定对一个未过滤清单里
//! 明明含有的子串返回零行，而一个被丢弃的查询错误让这与「这个 run 没有这样的失败」无从区分——在
//! 一个测量工具上这是最坏的混淆，因为 `unexpected error` 计数正是抓住发明诊断的方式（一条 FAIL
//! 变成另一条 FAIL 在通过数里是隐形的）。

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

    // 按细节——通过数永远显不出的形状。
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

    // 按 key——指标正因此得以按簇分桶。
    let by_key = hit("other/case");
    assert_eq!(
        by_key.len(),
        1,
        "key filter matched the wrong set: {by_key:?}"
    );
    assert!(by_key[0].starts_with("other/case::b"), "got {by_key:?}");

    // 不带模式列出该 run 的每条失败——通过的从不入库。
    assert_eq!(hit("").len(), 2, "an empty pattern must list all failures");
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}

/// `--count` 背后的参数切分，单独钉住，因为它的失败模式是静默的：若这个 flag 被当成 needle 读，
/// 查询会答「零失败」，于是一个被闸住的步骤会在一次从未发生的测量上放行
/// （`ledger failures <run> "unexpected error"` 正是 1.4/1.5 所闸的读数）。
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
    // 同一条查询不带 flag 时列出行而不是计数。
    assert_eq!(
        ledger::failure_query(&args(&["run", "unexpected error"])),
        (
            "run".to_string(),
            vec!["unexpected error".to_string()],
            false
        ),
    );
    // 一个 run 完全没有模式：没有 needle，而 `--count` 仍不得变成一个。
    assert_eq!(
        ledger::failure_query(&args(&["run", "--count"])),
        ("run".to_string(), vec![], true),
    );
    // 上述的阳性对照：一个真的缺席的 needle 仍作为 needle 被携带，所以上面的「空」意味着查询被
    // 听懂了，而不是 flag 静默地什么都没匹配。
    assert_eq!(
        ledger::failure_query(&args(&["run", "no/such/cluster"])),
        (
            "run".to_string(),
            vec!["no/such/cluster".to_string()],
            false
        ),
    );
    // 两个 needle 就是两个，按顺序——地形读数需要的形状。
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

/// 为什么多个 needle 必须由工具 AND 在一起，而不是手工拼接：把 `"cluster"
/// "expected error"` 拼成一个 needle 是向 SQL 要一个任何行都不含的子串，于是它答**零**——而一个看
/// 起来像测量的零，正是一次会话里整个簇被写成「all of these want a
/// diagnostic」或「none of these do」的方式。
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

    // 只有簇：两条 hwb 失败，另一簇的一条也没有。
    assert_eq!(hit(&["core_functions/color/hwb"]).len(), 2);
    // 只有结果类别时跨簇。
    assert_eq!(hit(&["expected error"]).len(), 2);
    // AND 起来：恰好是那些要诊断的 hwb 行——选择一份切片时用的读数。
    let both = hit(&["core_functions/color/hwb", "expected error"]);
    assert_eq!(both.len(), 1, "got {both:?}");
    assert!(
        both[0].starts_with("core_functions/color/hwb::error/units"),
        "got {both:?}"
    );
    // 这里钉的失败模式：一个装着两个词的 needle 什么也不匹配，却像是一个答案。
    assert_eq!(
        hit(&["core_functions/color/hwb expected error"]).len(),
        0,
        "a concatenated needle must not be the way this is queried"
    );
    // 第二个 needle 在效果上顺序无关：那唯一一行两种顺序都存活。
    assert_eq!(
        hit(&["expected error", "core_functions/color/hwb"]).len(),
        both.len()
    );
    std::fs::remove_dir_all(db.parent().unwrap()).ok();
}
