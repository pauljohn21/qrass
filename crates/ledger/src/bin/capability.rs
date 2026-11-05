//! `ledger capability <run>`——**哪个能力挡住哪些行**，从语料数出来，而不是从我正好在动的那个缺陷猜。
//!
//! 这个工具为什么存在：按*目录*看失败分布（`core_functions/color` = 5,732 行）说的是红在哪里，不是
//! *为什么*，于是每个会话都接着从上一根 needle 里挑下一个增量、在局部修复上反复争执。本命令走一遍某个
//! 具名 run 的每一个失败 key，从它出自的那份 `.hrx` 归档里取出该用例自己的 `input.scss`，再按下面的特征
//! 谓词分桶。
//!
//! 每个特征两个数字，因为缺了另一个就会误导：**rows**（输入里只要提到这个特征——一个会话能靠造出它解锁
//! 多少，上界）与 **sole**（输入里*只*提到这一个特征——不被别的东西挡住的下界）。按 `rows` 排是奖品的规模；
//! 按 `sole` 排是今天真正挡住一行的是什么。
//!
//! 归档是唯一的 oracle：一个解析不出的 key 会被计数并报告，而不是悄悄丢掉。

use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 一个谓词和它点的名字。这些字符串拿去匹配该用例的输入文本；`error golden` 匹配的是账本自己的失败
/// 详情，因为期望是错误消息的那些行是被错误那摊工作挡住的，不是被它们输入里写的特征挡住的。
fn predicates() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "module: local @use",
            vec![
                "@use \"core_functions",
                "@use 'core_functions",
                "@use \"sass-spec",
            ],
        ),
        ("module: sass: @use", vec!["@use \"sass:", "@use 'sass:"]),
        ("module: @forward", vec!["@forward"]),
        ("module: @import", vec!["@import"]),
        ("mixin: @include", vec!["@include"]),
        ("mixin: @mixin def", vec!["@mixin "]),
        ("function: @function", vec!["@function"]),
        ("control: @each", vec!["@each"]),
        ("control: @while", vec!["@while"]),
        ("control: @if/@else", vec!["@if ", "@else"]),
        ("control: @extend", vec!["@extend"]),
        ("control: @media", vec!["@media"]),
        ("control: @supports", vec!["@supports"]),
        ("control: @content", vec!["@content"]),
        ("control: @at-root", vec!["@at-root"]),
        (
            "color: Color-4 spaces",
            vec![
                "color(",
                "to-space",
                "from-space",
                "to-gamut",
                "is-in-gamut",
                "$space:",
                "oklab(",
                "oklch(",
                "lab(",
                "lch(",
                "color.hwb",
            ],
        ),
        ("args: rest arglist", vec!["..."]),
        ("interp: #{", vec!["#{"]),
        ("calc: calc()", vec!["calc("]),
        ("ns: map.", vec!["map."]),
        ("ns: math.", vec!["math."]),
        ("ns: meta.", vec!["meta."]),
        ("ns: selector.", vec!["selector."]),
        ("ns: string.", vec!["string."]),
        ("ns: list.", vec!["list."]),
        ("ns: color.", vec!["color."]),
        ("ns: function.", vec!["function."]),
        ("ns: mixed.", vec!["mixed."]),
        ("value: custom property", vec!["--"]),
        ("value: url/var fallback", vec!["var("]),
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let usage = || tracing::info!("usage: ledger capability <run> [--db <path>]");
    let (db, run) = match argv.split_first() {
        // run 名是位置参数，`--db` 的值在下一个参数里，与 `ledger` 二进制自己被调用的方式一致
        // （`--db` 带空格，不是 `--db=`）。
        Some((run, rest)) => {
            let explicit = rest
                .iter()
                .position(|arg| arg == "--db")
                .and_then(|index| rest.get(index + 1));
            if !rest.is_empty() && explicit.is_none() {
                usage();
                return Ok(());
            }
            (
                explicit.map_or_else(|| PathBuf::from(ledger::DB), PathBuf::from),
                run.clone(),
            )
        }
        None => {
            usage();
            return Ok(());
        }
    };
    let spec = Path::new("sass-spec/spec");
    let conn = Connection::open(&db)?;
    let rows = ledger::failing_any(&conn, &run, &[])?;
    let mut hits: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut sole: BTreeMap<&'static str, usize> = BTreeMap::new();
    let (mut unresolved, mut classified) = (0usize, 0usize);

    for row in &rows {
        let key = row.split(" | ").next().unwrap_or("").to_string();
        let detail = row.splitn(2, " | ").nth(1).unwrap_or("").to_lowercase();
        let Some(input) = input_of(&key, spec) else {
            unresolved += 1;
            continue;
        };
        classified += 1;
        let matched = predicates()
            .into_iter()
            .filter(|(name, needles)| {
                if *name == "error golden (P7)" {
                    detail.starts_with("expected error")
                } else {
                    needles.iter().any(|needle| input.contains(needle))
                }
            })
            .collect::<Vec<_>>();
        for (name, _) in &matched {
            *hits.entry(name).or_default() += 1;
        }
        if matched.len() == 1 {
            *sole.entry(matched[0].0).or_default() += 1;
        }
    }

    // 错误期望是它自己的一个桶：一行想要的若是 `Error: …`，就不可能靠实现它输入里写的语法落地，所以不能把它
    // 算成语法奖品。
    let errors = rows
        .iter()
        .filter(|row| {
            row.splitn(2, " | ")
                .nth(1)
                .unwrap_or("")
                .to_lowercase()
                .starts_with("expected error")
        })
        .count();
    *hits.entry("error golden (P7)").or_default() = errors;

    let mut ranked = hits.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    tracing::info!(
        run = %run,
        failing = rows.len(),
        resolved = classified,
        unresolved = unresolved,
        "capability buckets"
    );
    for (name, count) in &ranked {
        tracing::info!(
            capability = %name,
            rows = count,
            sole = sole.get(name).copied().unwrap_or_default(),
            "rows / sole"
        );
    }
    Ok(())
}

/// 该用例自己的输入文本：key 形如 `<path under spec>::<case name>`，前半是 spec 之下的路径；而在归档里
/// 段头写作 `<===> <case name>/input.scss`。任何一半找不到都返回 `None`，调用方把它报成 `unresolved` 而不是
/// 悄悄丢掉这一行。
fn input_of(key: &str, spec: &Path) -> Option<String> {
    let (path, case) = key.split_once("::")?;
    let file = spec.join(format!("{path}.hrx"));
    let text = std::fs::read_to_string(file).ok()?;
    let header = format!("<===> {case}/input.scss");
    let start = text.lines().position(|line| line == header)?;
    Some(
        text.lines()
            .skip(start + 1)
            .take_while(|line| !line.starts_with("<===>"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}
