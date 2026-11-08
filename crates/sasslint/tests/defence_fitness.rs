//! 防线自己的适应度函数。这里断四件事，每件都对着一个曾在本仓库静默腐坏的论断：
//!
//! 1. **引擎能发出的每道闸都有可用的阳性对照。** id 列表*从引擎自己的表*推导
//!    （`sasslint::SUBSTRING_BANS` / `AND_BANS` / `LINE_START_BANS` / `FILE_RULE_IDS`），从不抄进
//!    本文件——手抄一份就是同一事实住两处，而这样一份手抄正是某道闸 id 从清单上悄悄消失的方式。
//! 2. **被管的树保持干净**，这正是闸不被放宽到超出其所编码教义的原因（本会话量过两次：量具给出
//!    过一个 `0` 读数）。
//! 3. **教义形状的源文件零 findings** —— 下界。没有它，「每条规则都触发」可以由「见什么触发
//!    什么」的规则满足。
//! 4. **缓做闸连同其债一起记录**，让缺口可读而不是隐形。

use std::fs;
use std::path::{Path, PathBuf};

use sasslint::{
    AND_BANS, FILE_RULE_IDS, Finding, LINE_START_BANS, SUBSTRING_BANS, scan_dir, scan_file,
};

/// 引擎能发出的每个 id，从引擎本身读出。
fn engine_rule_ids() -> Vec<&'static str> {
    SUBSTRING_BANS
        .iter()
        .map(|(rule, _)| *rule)
        .chain(AND_BANS.iter().map(|(rule, _)| *rule))
        .chain((!LINE_START_BANS.is_empty()).then_some("no-imperative-loop"))
        .chain(FILE_RULE_IDS.iter().copied())
        .collect()
}

/// 一份携带每条禁令形状的源。每行都按本仓库某个时点违规代码的实际写法写，而不是作为提及规则的
/// 注释。
const PLANTED: &str = "\
pub struct Rule { pub children: Vec<Rule> }
type Stage<T> = Box<dyn Fn(T) -> T>;
fn relay(items: Vec<Rule>) -> Vec<Rule> { items }
fn subst(text: &str) -> String { text.to_string() }
fn resolve(&self, node: Node) -> Value { self.resolve(node.next()) }
fn driver() { let _ = from_fn(|_| Vec::<Item>::new()); }
fn queue(q: VecDeque<Item>) -> VecDeque<Item> { q }
fn drive(&mut self) { let frames = Vec::<Frame>::default(); let _ = self.chain_taken; }
fn order(&mut self) { self.rows.sort_by_key(|row| row.order); }
fn batch(src: i32) { let _ = scan_map(src, |acc, item| -> Vec<Item> { vec![] }); }
fn silent(eval: Option<i32>) { let _ = eval.ok(); }
fn unwrap_to_text(eval: Option<i32>) { let _ = eval.unwrap_or_else(|| 1); }
fn multicast(src: i32) { let _ = merge_all(src).subscribe(|v| { let _ = v; }); }
loop { }
fn to_base(unit: &str) -> f64 { match unit { \"px\" => 1.0, \"in\" => 96.0, \"cm\" => 37.0, \"turn\" => 360.0, _ => 0.0 } }
";

#[test]
fn every_gate_the_engine_can_emit_has_a_positive_control() {
    let dir = std::env::temp_dir().join("sasslint_defence_probe");
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bad.rs"), PLANTED).unwrap();

    let findings: Vec<Finding> = scan_dir(&dir);
    let missing: Vec<&str> = engine_rule_ids()
        .into_iter()
        .filter(|rule| !findings.iter().any(|found| found.rule == *rule))
        .collect();
    assert!(
        missing.is_empty(),
        "these gates fire on nothing — fix the planted shape or the rule, never the list: {missing:?}",
    );

    fs::remove_dir_all(&dir).ok();
}

/// 该闸必须找到**文档注释背后**的第二张表，并报臂行，而不是别的行。它的存在是因为修复的第一版
/// 在剥注释后的字符串里计算臂行，而 `scan_file` 报的是原文件的行号——两行注释就让该闸整个漏掉
/// 这个文件，而当时没有别的 fixture 带注释，所以什么也没失败。
#[test]
fn a_table_behind_comments_is_still_caught_at_its_arm_line() {
    let path = std::env::temp_dir().join("sasslint_commented_table.rs");
    fs::write(
        &path,
        "/// Convertible units and their factors.\n\
         /// Kept locally for speed.\n\
         fn to_base(unit: &str) -> f64 {\n\
         \x20   match unit {\n\
         \x20       \"px\" => 1.0,\n\
         \x20       \"in\" => 96.0,\n\
         \x20       \"cm\" => 37.0,\n\
         \x20       \"turn\" => 360.0,\n\
         \x20       _ => 0.0,\n\
         \x20   }\n}\n",
    )
    .unwrap();
    let hits: Vec<usize> = scan_file(&path)
        .into_iter()
        .filter(|f| f.rule == "single-table")
        .map(|f| f.line)
        .collect();
    assert_eq!(
        hits,
        vec![5],
        "the first arm line of a commented-out table must be the reported line"
    );
}

/// 一道消失的闸和一道从未生效的闸一样静默：覆盖面从引擎推导，所以这里的*计数*被钉住。它若
/// 失败，要么某道闸被删了（恢复它），要么新增了一道（在上面植入它的形状并有意识地更新数字）。
#[test]
fn the_gate_count_is_pinned() {
    let mut ids = engine_rule_ids();
    ids.sort_unstable();
    ids.dedup(); // `require-err-channel` 同时由同行对与文件级检查发出
    assert_eq!(
        ids.len(),
        14,
        "the gate set changed size: {ids:?} — a deleted gate must come back, a new one needs a plant"
    );
}

#[test]
fn the_production_tree_is_clean() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&dir);
    assert!(
        findings.is_empty(),
        "sasspile must stay gate-clean ({} findings): {findings:?}",
        findings.len()
    );
}

/// 一个按教义要求的形状写的源——真实的算子内核、订阅前先停好的 `SingleAssignment` 槽、用 `filter`
/// 分支、单位列在数组里而不是 `=>` 臂上——必须完全不产生 findings。
#[test]
fn a_doctrine_shaped_file_produces_no_findings() {
    let path = std::env::temp_dir().join("sasslint_doctrine_clean.rs");
    fs::write(
        &path,
        "\
struct Lex<S> { source: S }
const NAMES: [&str; 4] = [\"px\", \"cm\", \"deg\", \"turn\"];
impl<C: Context> CoreObservable<C> for Lex<S> {
    fn subscribe(self, ctx: C) {
        let w = ctx.transform(|o| LexObserver { observer: o, state: State::default() });
        self.source.subscribe(w)
    }
}
fn pipeline(src: i32) {
    let slot = SingleAssignment::<LocalBoxedObservable<i32>>::channel();
    slot.subscribe(src);
}
fn is_length(name: &str) -> bool { matches!(name, \"px\" | \"cm\") }
fn failure(&self) -> bool { false }
fn branch(items: Vec<Item>) { items.into_iter().filter(|item| item.keep) }
",
    )
    .unwrap();
    let findings = scan_file(&path);
    assert!(
        findings.is_empty(),
        "a file written the way the doctrine requires was flagged: {findings:?}"
    );
}

/// 本计划规划了却**没有**落地的那一道闸，连同债务规模一起留在记录里。`rust.md` 彻底
/// 禁掉 `if/else if` 链（它唯一的例外是不带 `else` 的单个守卫 `if`），所以现存各址是存量债
/// 而非合法形状——而一根分不清二者的 needle 第一天就会把整棵树标红。等它们被重构时，实现该闸并
/// 删掉本测试；记录的两半任一不再为真，本测试就会失败。计数走的是整棵树，所以某个没人列出的文件
/// 里的新链无法藏在硬编码清单后面；它数 `} else if` 拼写——引擎自己的分支链就是这个样子。
///
/// 这个数字只朝**下**走：还掉一址就在这里连同理由记录（2026-10-29：`loops::for_loop` 的
/// `through`/`to` 阶梯改成对两个词的搜索，7 → 6），而新增一处是断言会抓住的教义违规。在债清零
/// 之前实现 `no-else-if-chain` 会把整棵树标红并招来「跳过这些址」，那是放宽闸。仍存在的六个不是
/// 一类东西：五个是分类器，替换必须对着行量过（`color/read.rs:109` 的 space 选择、
/// `builtins/math.rs:114` 的单位选择，以及 `lexer.rs:199/201/206` 的字符分派），第六个是 fold 累加
/// 子上的形状分支——`expr::space_run` 在问一个组是否已打开——没有任何类别表表达它，所以它需要
/// 重新考虑分解，而不是一次只把阶梯挪到本 needle 看不见的地方的改写。
#[test]
fn the_deferred_branch_gate_stays_recorded() {
    assert!(
        !engine_rule_ids().contains(&"no-else-if-chain"),
        "`no-else-if-chain` now exists — this record is obsolete, delete it and let the positive control cover the gate"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let counted = rust_files(&root)
        .iter()
        .map(|file| {
            fs::read_to_string(file)
                .unwrap_or_else(|e| panic!("{} unreadable: {e}", file.display()))
                .lines()
                .filter(|line| line.contains("} else if"))
                .count()
        })
        .sum::<usize>();
    assert_eq!(
        counted, 6,
        "the recorded else-if debt changed size ({counted}): either some sites were reworked \
         (implement the gate and remove this record) or new ones were added (a doctrine violation \
         that must be fixed, not counted)"
    );
}

/// `dir` 下的每个 `.rs` 文件。仅测试可用：工具可以递归，禁令针对的是编译产物管线走树。债务记录
/// 必须看见整棵树，否则没人列出的文件里的新链会让记录保持绿色。
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .for_each(|path| {
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            });
    }
    files
}
