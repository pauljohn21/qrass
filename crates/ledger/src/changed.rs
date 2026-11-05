//! `diff` 给不了的那个 A/B 读数：**两个** run 都失败、但详情*不同*的行。
//!
//! `diff` 回答的是“什么开始失败或不再失败”。一行前后都失败，两个计数都不动，可它的失败种类可以变——
//! 一句诚实的原样输出变成一个错值，或者一个错值变成一句语料从没说过的错误。本项目就被这件事抓住过：
//! 有一批报 `regress=0 net=11`，而它自己的簇已经从“保留源文本”变成“算出金样说用户自己那个函数该返
//! 回的值”，另一批在同样干净的读数背后多加了六条无效错误。所以这个比较是一条命令，不是肉眼比对两份列表
//! 的习惯：`ledger changed <a> <b> [key-prefix]`。

use rusqlite::params;
use rusqlite::{Connection, Result as QResult};
use tracing::info;

/// 一次形状比较，作为调用方能直接断言的数据：那些有差异的行，加上**每个 run 在 scope 内的失败行数**。
/// 这两个计数不是装饰——没有它们，`changed = 0` 就和“scope 什么都没匹配上”分不开，而 [`crate::Diff::fails_a`]
/// /`fails_b` 就是为了同一个混淆而存在的。
#[derive(Debug)]
pub struct Changed {
    pub rows: Vec<String>,
    pub fails_a: usize,
    pub fails_b: usize,
}

/// 两个 run 都失败、存下的详情不同的 key，输出成 `key | a: <detail> | b: <detail>` 行，按 key 前缀限定
/// scope（空 = 整份记录），与 [`crate::diff_runs`] 完全一致，好让一次限定 scope 的校验不会悄悄在读空。
pub fn changed_details(conn: &Connection, a: &str, b: &str, prefix: &str) -> QResult<Changed> {
    let (ra, rb) = (crate::run_id(conn, a)?, crate::run_id(conn, b)?);
    let pattern = format!("{prefix}%");
    let mut st = conn.prepare(
        "SELECT c.key || ' | a: ' || COALESCE(da.text, '') || ' | b: ' || COALESCE(db.text, '')
         FROM run_fail fa
         JOIN run_fail fb ON fb.run_id = ?2 AND fb.case_id = fa.case_id
         JOIN case_key c ON c.id = fa.case_id
         LEFT JOIN fail_detail da ON da.id = fa.detail_id
         LEFT JOIN fail_detail db ON db.id = fb.detail_id
         WHERE fa.run_id = ?1
           AND c.key LIKE ?3
           AND COALESCE(da.text, '') <> COALESCE(db.text, '')
         ORDER BY c.key",
    )?;
    let rows = st
        .query_map(params![ra, rb, pattern], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<String>, _>>()?;
    let mut counting = conn.prepare(
        "SELECT COUNT(*) FROM run_fail f JOIN case_key c ON c.id = f.case_id
         WHERE f.run_id = ?1 AND c.key LIKE ?2",
    )?;
    let mut fails_in =
        |run: i64| -> QResult<usize> { counting.query_row(params![run, pattern], |r| r.get(0)) };
    Ok(Changed {
        rows,
        fails_a: fails_in(ra)?,
        fails_b: fails_in(rb)?,
    })
}

/// `ledger changed <a> <b> [key-prefix]`：每条形状变了的失败一行，然后把计数**挨着两个 scope 总量**打出来——
/// 一个 `0` 必须能和“scope 什么都没匹配上”区分开，`diff` 那次干净的判定就是这样被误读过的。
pub fn run_cli(conn: &Connection, args: &[String]) -> QResult<()> {
    let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
        info!("usage: ledger changed <a> <b> [key-prefix]");
        return Ok(());
    };
    let prefix = args.get(2).cloned().unwrap_or_default();
    let changed = changed_details(conn, a, b, &prefix)?;
    changed
        .rows
        .iter()
        .for_each(|line| info!(target: "ledger", "CHANGED {line}"));
    info!(
        target: "ledger",
        a,
        b,
        scope = if prefix.is_empty() { "*" } else { prefix.as_str() },
        fails_a = changed.fails_a,
        fails_b = changed.fails_b,
        changed = changed.rows.len(),
        "failure-shape diff"
    );
    Ok(())
}
