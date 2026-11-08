//! hrx 账本测试台——把 `sass-spec/*.hrx` 用例经唯一公共出口 `sasspile::compile`
//! 运行，并为 `ledger` A/B 发出 `STATUS\tkey[\tdetail]` 快照。
//! 它不对单个用例做通过/失败断言（编译器是故意不完整的）；它产生稳定的逐用例
//! 通过/失败集合，以便在功能接入时 `ledger diff <base> <run>` 给出 `regress = 0`。
//! 测试脚手架位于 `tests/`（产品代码留在 `src/`）。
//!
//! 两个环境旋钮决定一次运行触及什么：
//! * `HRX_RUN=<name>`（加可选 `HRX_DB=<path>`）把判定集合记入账本；
//! * `HRX_CASE=<key 子串>` 把运行收窄到账本 key 含它的用例——这是查看**单个**
//!   用例 trace 的方式，因为 11,533 个 span 不是诊断视图。它与 `HRX_RUN`
//!   互斥：部分测量被写进记录会冒充可比的运行总量（见下方守卫）。

#[path = "common/tracing_bridge.rs"]
mod tracing_bridge;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sasspile::compile;

/// 在域内的 oracle 目录。按策略排除：`non_conformant`（记录规范本身称为
/// 非一致的行为）、`libsass*`（旧引擎问题）、`helpers`（非测试）。
///
/// `operators`、`parser` 与 `variables` 直到 P1.3 都不在此处，尽管该变更自身的
/// 任务 1.6 引 `operators/**` 为其 golden 来源——79 个用例从未被测量，这正是覆盖
/// 缺口如何变成不可证伪的“regress=0”。加入它们是范围变更：总量会移动，验收规则
/// 是任何先前覆盖的用例不得从 PASS 移到 FAIL。
const SPEC_DIRS: [&str; 9] = [
    "core_functions",
    "directives",
    "values",
    "expressions",
    "css",
    "callable",
    "operators",
    "parser",
    "variables",
];

/// 一个解析后的 Hyrax 用例：以 `.hrx` 内的目录路径为键。
#[derive(Default)]
struct Case {
    input: Option<String>,
    output: Option<String>,
    /// 用例的 `error` 段落，逐字（用例期望 CSS 时为 `None`）
    error: Option<String>,
    extra_inputs: usize,
    has_options: bool,
}

fn walk_hrx(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_hrx(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("hrx") {
                out.push(path);
            }
        }
    }
}

/// 把 `.hrx` 拆成 `(case_key, Case)`。一个段落是 `<===> <path>` 直到下一个
/// `<===>`；裸 `<===>` + `====` 分隔符不带路径，被忽略。
fn parse_hrx(text: &str) -> BTreeMap<String, Case> {
    let mut cases: BTreeMap<String, Case> = BTreeMap::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        match line.strip_prefix("<===>") {
            Some(rest) => {
                let path = rest.trim();
                i += 1;
                let mut body = String::new();
                while i < lines.len() && !lines[i].starts_with("<===>") {
                    body.push_str(lines[i]);
                    body.push('\n');
                    i += 1;
                }
                if path.is_empty() || path.starts_with("====") {
                    continue; // group separator
                }
                let (key, file) = split_case(path);
                let case = cases.entry(key).or_default();
                match file {
                    "input.scss" => {
                        if case.input.is_some() {
                            case.extra_inputs += 1;
                        }
                        case.input = Some(body);
                    }
                    "output.css" => case.output = Some(body),
                    "error" => case.error = Some(body),
                    "options.yml" => case.has_options = true,
                    _ => {
                        if file.ends_with(".scss") {
                            case.extra_inputs += 1;
                        }
                    }
                }
            }
            None => i += 1,
        }
    }
    cases
}

/// `unitless/to_unit/input.scss` → (`unitless/to_unit`, `input.scss`)。
fn split_case(path: &str) -> (String, &str) {
    match path.rsplit_once('/') {
        Some((dir, file)) => (dir.to_string(), file),
        None => (String::new(), path),
    }
}

/// 一个用例的账本 key：其相对 `spec/` 的路径（去掉 `.hrx`）加上文件内的用例目录。
///
/// 它曾是 `<basename>.hrx::<case>`，那会静默合并不同用例：语料库有
/// `css/plain/function.hrx`、`css/function.hrx`、`css/supports/syntax/function.hrx`、…
/// 全都折叠到 `function.hrx::…`。`ledger` 按用例为运行加键，所以被合并的对会让
/// 一个文件的 PASS 掩盖另一个的 FAIL——实测：`ledger diff` 报告 `regress=0 net=266`，
/// 而逐用例集合 diff 是 `+385 / -3`。因此关于回归的任何结论都必须来自唯一 key。
fn case_id(root: &Path, file: &Path, key: &str) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file).with_extension("");
    format!("{}::{}", rel.display(), key)
}

/// 失败用例的 oracle 消息：其 `error` 段落首条非空行去掉前导 `Error:`——
/// 语料库在该行声明消息，把源 span 放在下面
/// （`css/comment.hrx::error/loud/interpolation/failure`）。
///
/// 不以 `Error:` 开头的段落（`DEPRECATION WARNING` 或 `Warning:`）有我们尚不能
/// 产生的消息；P7 7.2 带来诊断，所以这样的用例必须计为不匹配。这正是使 `error`
/// 用例成为真实断言之物：在此之前，测试台对**任何**错误都通过，并实测 385 个 385
/// 翻转的 FATAL“获得”用例，其 oracle 是不相关的消息。
fn oracle_header(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    line.strip_prefix("Error:")
        .map(|message| message.trim().to_string())
}

/// 规范化比较用 CSS：逐行 trim，丢弃空行，以 `\n` 拼接。
fn norm(css: &str) -> String {
    css.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn hrx_run() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sass-spec/spec");
    let _span = tracing::info_span!("hrx").entered();
    tracing_bridge::init();
    // 单用例选择器，读取一次。`Option<str>` 使下面的循环能以同一问题询问而不需要
    // 特例：`None` 测量一切。
    let only = std::env::var("HRX_CASE").ok();
    let selected = |id: &str| {
        only.as_ref()
            .is_none_or(|needle| id.contains(needle.as_str()))
    };
    let mut files = Vec::new();
    SPEC_DIRS
        .iter()
        .for_each(|d| walk_hrx(&root.join(d), &mut files));

    let mut rows: Vec<String> = Vec::new();
    let (mut total, mut pass, mut skipped) = (0usize, 0usize, 0usize);
    // 在通过中，有多少是对**空**期望输出赢得的。它们合法地正确（Sass 对例如
    // `@if true {}` 确实什么都不输出），但它们也是什么都不产出的编译器的得分，
    // 所以绝不可被读作功能有效的证据——实测总体 635 中 176，而在循环/分支存在
    // 之前 `directives/{{if,for,while}}` 是 39 分之 39。
    let mut trivial_empty = 0usize;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut collision: Vec<String> = Vec::new();

    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (key, case) in parse_hrx(&text) {
            // 只可作为单输入、单输出（或单错误）用例运行。
            if case.extra_inputs > 0 || case.has_options || case.input.is_none() {
                skipped += 1;
                continue;
            }
            if !(case.output.is_some() || case.error.is_some()) {
                skipped += 1;
                continue;
            }
            let id = case_id(&root, file, &key);
            // `HRX_CASE` 之外的用例不被测量：它既不进 `total` 也不进行，
            // 所以任何读数字的东西都不会把过滤运行误当完整运行。
            if !selected(&id) {
                continue;
            }
            total += 1;
            if !seen.insert(id.clone()) {
                collision.push(id.clone());
            }
            tracing::info!(stage = "hrx", id = %id, input_len = case.input.as_ref().map_or(0, |s| s.len()), "case start");
            let result = compile(case.input.unwrap()).await;
            let want_error = case.error.as_deref().and_then(oracle_header);
            match (result, &case.output, want_error) {
                (Ok(css), Some(expected), None) if norm(&css) == norm(expected) => {
                    pass += 1;
                    if norm(expected).is_empty() {
                        trivial_empty += 1;
                    }
                    rows.push(format!("PASS\t{id}"));
                }
                // 一个 error 用例只由 oracle 声明的消息满足，绝不靠任何错误满足。
                (Err(e), _, Some(want)) if e.message == want => {
                    pass += 1;
                    rows.push(format!("PASS\t{id}"));
                }
                (Err(e), _, Some(want)) => rows.push(format!(
                    "FAIL\t{id}\tgot error {:?} want {:?}",
                    e.message, want
                )),
                (Err(e), _, None) => {
                    rows.push(format!("FAIL\t{id}\tunexpected error {:?}", e.message))
                }
                (Ok(css), Some(expected), None) => rows.push(format!(
                    "FAIL\t{id}\tgot {:?} want {:?}",
                    norm(&css),
                    norm(expected)
                )),
                (Ok(css), _, Some(want)) => rows.push(format!(
                    "FAIL\t{id}\texpected error {:?}, got {:?}",
                    want,
                    norm(&css)
                )),
                (Ok(_), None, None) => rows.push(format!("FAIL\t{id}\tno expected output")),
            }
        }
    }

    // 每个失败也作为事件发出。记录（`ledger failures <run>`）仍是持久读取，但在
    // `trace` 下 got/want 对应出现在它发生之处，紧邻产生它的 span——这正是无需
    // 手工重新推导 oracle 就能诊断用例的方式。
    rows.iter().for_each(|row| {
        let (status, rest) = row.split_once('\t').unwrap_or(("?", ""));
        if status == "FAIL" {
            let (id, detail) = rest.split_once('\t').unwrap_or((rest, ""));
            tracing::warn!(stage = "hrx", id, verdict = detail, "case failed");
        }
    });

    // 记录是账本数据库，绝不是文件。`HRX_RUN=<name>` 把本次运行直接写入
    // `reports/hrx.db`（`HRX_DB=<path>` 把它指向另一个数据库，强制的全新空库
    // A/B 复检就是这样做的）；没有 `HRX_RUN` 时测试台只测量并记录日志，不留产物。
    // 这替代了旧的 `target/hrx-snapshot.tsv` + `ledger import <file>` 两步，它强制
    // 每次 A/B 持两个文件——并让我曾用 `git stash` 重建“前”侧，这个操作一次撞上
    // `index.lock`，在转录里留下一个过期数字。
    //
    // 过滤运行与记录运行不能共存。`run.total` 是使 `ledger diff` 意为“同一群体，
    // 两次”之物；把单用例测量存到运行名下会交给下一次 A/B 一个看起来可比其实不可比
    // 的数字，这正是本项目反复吃亏的失败模式。用 `HRX_CASE` 诊断，用完整运行记录。
    assert!(
        !(only.is_some() && std::env::var("HRX_RUN").is_ok()),
        "HRX_CASE selects a subset for diagnosis and must not be combined with HRX_RUN, which would \
         record a partial measurement as a comparable run (run.total would stop meaning the whole \
         population). Drop HRX_CASE to record, or drop HRX_RUN to trace."
    );
    let recorded = std::env::var("HRX_RUN").ok().map(|name| {
        let db = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(std::env::var("HRX_DB").unwrap_or_else(|_| "../../reports/hrx.db".into()));
        let (stored_total, stored_pass) = ledger::record(&name, &db, &rows)
            .unwrap_or_else(|e| panic!("ledger record {name}: {e}"));
        // 数据库现在是唯一记录，所以它存储的必须等于所测量的：静默不匹配会让
        // 之后每次 `ledger diff` 比较错误的集合。
        assert_eq!(
            stored_total,
            rows.len(),
            "ledger stored a different total for {name}"
        );
        assert_eq!(
            stored_pass, pass,
            "ledger stored a different pass count for {name}"
        );
        name
    });

    // 健全性：测试台发现并运行了用例。单个失败是预期的（编译器不完整）；回归
    // 跟踪经 `ledger diff`，不是此断言。配合 `HRX_CASE`，这兼任选择器所需的阳性
    // 对照：否则一个拼写错误会报告“0 之 0”，读作空的无害运行。
    assert!(
        total > 0,
        "harness found no runnable cases{} under {root:?}",
        only.as_ref()
            .map(|needle| format!(" matching HRX_CASE {needle:?}"))
            .unwrap_or_default()
    );
    // 防上面缺陷的守卫：两个用例共享 key 使 `ledger diff` 能掩盖回归，
    // 所以运行无效，而不只是有噪声。
    assert!(
        collision.is_empty(),
        "{} duplicate case keys, first few: {:?}",
        collision.len(),
        collision.iter().take(5).collect::<Vec<_>>()
    );
    tracing::info!(
        stage = "hrx",
        total,
        pass,
        // 选择器把群体收窄到什么，使部分运行不能被读作整体。
        selector = only.as_deref().unwrap_or("<none>"),
        // 一轮必须真正移动的数：对非空期望输出赢得的通过。
        pass_substantive = pass - trivial_empty,
        pass_empty_expectation = trivial_empty,
        skipped,
        pass_rate = format!("{:.1}%", 100.0 * pass as f64 / total as f64),
        recorded_run = recorded.as_deref().unwrap_or("<not recorded>"),
        "hrx run measured"
    );
}
