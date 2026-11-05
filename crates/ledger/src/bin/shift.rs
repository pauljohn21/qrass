//! `ledger changed` 留给操作者的那个读数：在**两个** run 都失败且详情不同的行里，哪些**朝着**归档移动了、
//! 哪些**远离**了。
//!
//! ```text
//! cargo run -q -p ledger --bin shift -- <base-run> <this-run> [key-prefix] [--db <path>]
//! ```
//!
//! `changed` 打出两侧详情就停在那里；它要求的定性一直到今天还是肉眼比对做出的，而本项目就是这么把一批
//! 816 条形状变了的行说成进步的，尽管它自己簇里有行*更*远离金样了。这条命令把这个拆分变成一个带阳性对照
//! 的读数（`crates/ledger/tests/shift_split.rs`），并且按 key 列出计数不该吞掉的两个桶：远离了的行，和换了
//! **种类**的行（一句原样输出变成算出的值、一个答案变成一个错误）。分类器因此住在 `src/shift_impl.rs`；
//! 它读过去的那条查询见 [`ledger::changed_details`]。

use rusqlite::Connection;
use std::path::PathBuf;

#[path = "../shift_impl.rs"]
mod shift_impl;

use crate::shift_impl::split_of;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // `--db <path>` 把位置参数与它的取值分开；位置参数的形状是
    // `<base> <this> [prefix]`，与 `changed` 自己接受的 scope 约定相同。
    let (positionals, db) = match argv.iter().position(|arg| arg == "--db") {
        Some(at) => match argv.get(at + 1) {
            Some(path) => (
                argv.iter().take(at).cloned().collect::<Vec<String>>(),
                PathBuf::from(path),
            ),
            None => {
                tracing::info!("--db needs a path after it");
                return Ok(());
            }
        },
        None => (argv, PathBuf::from(ledger::DB)),
    };
    let [base, this, prefix @ ..] = positionals.as_slice() else {
        tracing::info!("usage: ledger shift <base-run> <this-run> [key-prefix] [--db <path>]");
        return Ok(());
    };
    let prefix = prefix.first().cloned().unwrap_or_default();
    let conn = Connection::open(&db)?;
    let changed = ledger::changed_details(&conn, base, this, &prefix)?;
    let split = split_of(&changed.rows);
    split
        .farther_keys
        .iter()
        .for_each(|key| tracing::info!(key = %key, "FARTHER"));
    split
        .error_keys
        .iter()
        .for_each(|key| tracing::info!(key = %key, "ERROR"));
    split
        .shape_keys
        .iter()
        .for_each(|key| tracing::info!(key = %key, "SHAPE"));
    tracing::info!(
        base,
        this,
        scope = if prefix.is_empty() {
            "*"
        } else {
            prefix.as_str()
        },
        fails_a = changed.fails_a,
        fails_b = changed.fails_b,
        changed = changed.rows.len(),
        closer = split.closer,
        farther = split.farther,
        tie = split.tie,
        error_expectation = split.error_expectation,
        shape_of_text = split.shape_of_text,
        "distance split of the shape-changed rows"
    );
    Ok(())
}
