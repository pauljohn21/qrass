# 跨会话防 GC 防线（S1+S2）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把"防 GC / 防证据失效"从教条阅读变成机器闸与新鲜上下文审计：给 `sasslint` 补三条真缺的结构规则，并建立 `sasspile-start` skill、`gc-auditor` 子代理和瘦身后的 always-on 规则。

**Architecture:** 三层落地——(1) `crates/sasslint` 的确定性规则（文本/文件级判据，每条新规则必须自带植入反例阳性对照 + 合法形状不误伤下界）；(2) `.qoder/skills/sasspile-start`（新会话起手式：跑命令、打印读数、要求复述）；(3) `.qoder/agents/gc-auditor`（新鲜上下文重审 diff，只允许引用 `file:line` 与金样 case 名）。always-on 的 `.qoder/rules/rust.md` 从长文缩到 ≤40 行，把反例索引与工具级红线移入按需加载的文件。

**Tech Stack:** Rust edition 2024（toolchain 1.99）、`cargo test -p sasslint`、Qoder Skill/Subagent Markdown 约定（frontmatter 见 Task 5/4）。无新依赖、不开新 crate。

**Spec:** `docs/superpowers/specs/2026-10-28-cross-session-anti-gc-regression-design.md`（F1–F6 失效分类、五层防线、每层拦不住什么、冷启动判据）

## Global Constraints

- 不用 Python 或任何脚本语言做工具；测试一律 `#[test]` 且放在 `tests/`（`src/` 保持纯生产码）。
- 生产码禁 `println!/eprintln!`，一律 `tracing` 宏；单文件 ≤ 500 行（源码/测试分别计）。
- **不放松任何现有规则、不靠改名绕闸**；新规则只能收紧或新增。
- 每条新规则必须同时具备：**植入反例测试**（内嵌源码字符串，断言该规则命中）与**下界测试**（合法形状不得命中）；缺一不合入。
- `sasslint` 现有 12 条规则名（真实读自 `crates/sasslint/src/lib.rs`）：`no-tree`、`no-relay-vec`、`no-string-machine`、`no-recursive-descent`、`no-from-fn-driver`、`no-work-queue`、`mutable-accumulator-drives`、`no-sort-for-order`、`no-batch-shell`、`require-err-channel`、`require-slot-before-subscribe`、`no-imperative-loop`。**已覆盖的形状不得重造。**
- 本计划属"行为无涉"改造：完成后必须 `ledger diff <上一个 run> <新 run>` 得 **`net=0 regress=0`**（唯一允许的非零来自 `sasspile` 生产码未被这些任务触碰以外的事实——本计划不碰 `crates/sasspile/src`）。
- 文档中英混合：叙述中文，代码/命令/文件名/标识符 English。
- 不动 git 配置、不改 `.qoder/` 之外的宿主设置（属 S5，需单独批准）。
- 公共 API 现状（写测试要用真名）：`sasslint::{Finding, scan_file, scan_dir, review_report}`，`Finding { file: String, line: usize, rule: &'static str, text: String }`，`scan_file(&Path) -> Vec<Finding>`。

---

### Task 1: `no-stage-layer` 与 `no-else-if-chain` 两条文本规则

**Files:**
- Modify: `crates/sasslint/src/lib.rs`（规则表：现有条目形状 `("rule-id", &["needle", …])`，同行多 needle 表示 AND）
- Modify: `crates/sasslint/tests/lint.rs`（加植入反例 + 下界断言）

**Interfaces:**
- Consumes: `sasslint::scan_file(&Path) -> Vec<Finding>`
- Produces: 规则 id `"no-stage-layer"`、`"no-else-if-chain"`（Task 3 的植入反例汇总测试会引用这两个名字）

- [ ] **Step 1: 写失败测试**

在 `crates/sasslint/tests/lint.rs` 末尾追加：

```rust
/// 自造框架层（`type Stage` / `fn drain` / `fn fail`）与 `else if` 链是本仓库两次返工的根形状。
/// 这条测试是**阳性对照**：植入源码必须被抓到，否则规则是假的。
#[test]
fn planted_framework_layer_and_branch_chain_are_caught() {
    let path = std::env::temp_dir().join("sasslint_planted_layer.rs");
    fs::write(
        &path,
        "type Stage<T> = Box<dyn Fn(T) -> T>;\n\
         fn drain(&mut self) -> Vec<Item> { }\n\
         fn fail<T>(m: &str) -> Result<T, Error> { }\n\
         fn pick(x: i32) -> i32 { if x > 1 { 2 } else if x > 0 { 1 } else { 0 } }\n",
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(has(&f, "no-stage-layer"), "Stage/drain/fail not caught: {f:?}");
    assert!(has(&f, "no-else-if-chain"), "else-if chain not caught: {f:?}");
}

/// 下界：框架自己的算子内核与单个无 `else` 的 guard 是合法用法，不得误伤。
#[test]
fn operator_kernel_and_single_guard_are_not_flagged() {
    let path = std::env::temp_dir().join("sasslint_legal_layer.rs");
    fs::write(
        &path,
        "impl<C: Context> CoreObservable<C> for Lex {\n\
         \x20   fn subscribe(self, ctx: C) { let w = ctx.transform(|o| LexObserver { observer: o }); }\n\
         }\n\
         fn go(x: i32) { if x == 0 { return; } run(x); }\n",
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(!has(&f, "no-stage-layer"), "kernel misflagged: {f:?}");
    assert!(!has(&f, "no-else-if-chain"), "single guard misflagged: {f:?}");
}
```

- [ ] **Step 2: 跑到它失败**

Run: `cargo test -p sasslint --test lint -- planted_framework_layer_and_branch_chain_are_caught`
Expected: FAIL，两条 assert 都报 "not caught"（规则 id 尚不存在）。

- [ ] **Step 3: 加规则**

在 `crates/sasslint/src/lib.rs` 的表里，`("no-from-fn-driver", &["from_fn("]),` 之后插入：

```rust
    // A hand-rolled orchestration layer is the shape that bent rxrust into running the
    // project's own control flow (the rework root). The needles are the layer's own names;
    // `subscribe`/`CoreObservable` never appear on the same line, so real kernels pass.
    ("no-stage-layer", &["type Stage", "fn drain", "fn fail"]),
    // `if/else if` chains stand in for the branch the stream should express with `filter`.
    ("no-else-if-chain", &["} else if", "else if"]),
```

在 `line_findings`/`scan_line` 的判定处确认：单 needle 条目按"该行含此串"生效；两 needle 的 `no-else-if-chain` 因 `"} else if"` 与 `"else if"` 同行共现（Rust 写法必然如此），不会放过 `} else if` 也漏掉裸 `else if`。若现有实现要求"不同串"，改为单 needle `("no-else-if-chain", &["else if"])`。

- [ ] **Step 4: 跑到它通过**

Run: `cargo test -p sasslint`
Expected: `test result: ok.`，新增两测试通过，原 12 条规则测试仍绿。

- [ ] **Step 5: 确认生产树仍干净**

Run: `cargo run -q -p sasslint --bin sasslint -- lint crates/sasspile/src; echo "exit=$?"`
Expected: 末行 `count=0`、`exit=0`。若新规则打到现存合法代码：按"下界"做法**收窄 needle**（例如要求同行同时出现 `Vec<`），并在 `crates/sasslint/tests/lint.rs` 补一条"该合法形状不被标记"的断言；**不得**把规则删掉或跳过该文件。

- [ ] **Step 6: Commit**

```bash
git add crates/sasslint/src/lib.rs crates/sasslint/tests/lint.rs
git commit -m "feat(sasslint): flag hand-rolled stage layers and else-if chains, with a lower-bound test"
```

---

### Task 2: `single-table` —— 同一事实的第二张常量表

**Files:**
- Modify: `crates/sasslint/src/lib.rs`（`FileFacts` 加一个文件级事实 + 一条规则）
- Modify: `crates/sasslint/tests/lint.rs`

**Interfaces:**
- Consumes: `FileFacts::of(&content)`（现有文件级事实载体）与 `line_findings(&self, file, line_no, raw)`
- Produces: 规则 id `"single-table"`

- [ ] **Step 1: 写失败测试**

```rust
/// 单位换算表被写过两次（`unit::UNITS` 与 `builtins/math.rs` 的十位小数长度表），第二次直接让
/// `hypot(3cm, 4mm * 10, 5q * 40, …)` 无法回答。植入 4 个单位字面量的文件必须被抓到。
#[test]
fn a_second_unit_table_is_flagged_outside_the_shared_one() {
    let path = std::env::temp_dir().join("sasslint_planted_table.rs");
    fs::write(
        &path,
        "fn to_base(unit: &str) -> Option<f64> {\n\
         \x20   match unit {\n\
         \x20       \"px\" => Some(1.0),\n\
         \x20       \"in\" => Some(96.0),\n\
         \x20       \"cm\" => Some(37.7952755906),\n\
         \x20       \"turn\" => Some(360.0),\n\
         \x20       _ => None,\n\
         \x20   }\n}",
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(has(&f, "single-table"), "second unit table not caught: {f:?}");
}

/// 下界：`unit.rs` 里那张**唯一**表用的是数组字面量 + `f64` 因子，不是 `match`，且它本身就是真源，
/// 绝不该被自己的规则标记。
#[test]
fn the_shared_table_is_not_flagged() {
    let findings = scan_file(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src/unit.rs"),
    );
    assert!(
        !findings.iter().any(|f| f.rule == "single-table"),
        "the single source of truth must not be flagged: {findings:?}"
    );
}
```

- [ ] **Step 2: 跑到它失败**

Run: `cargo test -p sasslint --test lint -- a_second_unit_table_is_flagged_outside_the_shared_one`
Expected: FAIL（`single-table` 不存在）。

- [ ] **Step 3: 加文件级事实与规则**

在 `crates/sasslint/src/lib.rs` 中，`FileFacts` 附近加常量与计数（沿用 `FileFacts::of` 里现存的 `content.lines().filter(..).count()` 形状）：

```rust
/// Unit names that must appear in exactly one table. A file that branches on four of them is
/// a second convertible-unit table — the shape that made `math.hypot(…, 5q * 40, …)` unanswerable
/// while `unit::UNITS` already held `q`.
const UNIT_LITERALS: [&str; 12] = [
    "\"px\"", "\"in\"", "\"cm\"", "\"mm\"", "\"pt\"", "\"pc\"", "\"q\"",
    "\"deg\"", "\"grad\"", "\"rad\"", "\"turn\"", "\"ms\"",
];

/// Distinct unit literals present in the file.
fn unit_literals(content: &str) -> usize {
    UNIT_LITERALS
        .into_iter()
        .filter(|needle| content.contains(needle))
        .count()
}
```

在 `FileFacts` 增字段 `unit_literals: usize`（`of()` 里填 `unit_literals(&content)`）；在 `line_findings` 里，对**首个** `match` 行报：

```rust
        // One fact, one place: a `match` over four or more unit literals is a second table, and the
        // shared one lives in `unit.rs` as an array — never as a `match`.
        if self.unit_literals >= 4 && file != "unit.rs" && raw.contains("match") {
            out.push(finding(file, line_no, "single-table", raw));
        }
```

- [ ] **Step 4: 跑到它通过**

Run: `cargo test -p sasslint`
Expected: ok；两条新测试通过（含 `unit.rs` 下界）。

- [ ] **Step 5: 生产树与全工作区**

Run: `cargo run -q -p sasslint --bin sasslint -- lint crates/sasspile/src 2>&1 | tail -1; cargo test -p sasslint 2>&1 | grep -a "^test result"`
Expected: `count=0`；`test result: ok.` 后跟 `N passed; 0 failed`。**注意**：必须读全部 `test result` 行再判定，单个 `tail -1` 会读到 doc-test 的 `0 passed` 并误报。

- [ ] **Step 6: Commit**

```bash
git add crates/sasslint/src/lib.rs crates/sasslint/tests/lint.rs
git commit -m "feat(sasslint): single-table gate for a second convertible-unit table"
```

---

### Task 3: 防线自身的 fitness 测试（植入反例必须全抓、真源必须干净）

**Files:**
- Create: `crates/sasslint/tests/defence_fitness.rs`
- Modify: `crates/sasslint/src/lib.rs`（若某个 id 未被自己的植入文件抓到才需要修）

**Interfaces:**
- Consumes: `sasslint::{scan_dir, scan_file, Finding}`
- Produces: 一条把"每条规则都有阳性对照"变成测试事实的清单（后续 S3 `guard gc_scan` 直接复用这里的 id 列表）

- [ ] **Step 1: 写测试**

```rust
//! The defence's own fitness function: every rule id must fire on a planted counterexample, and
//! the production tree must stay clean. A rule nobody plants is a rule that can silently rot —
//! this session measured two zero-readings from broken instruments.

use std::fs;
use std::path::PathBuf;

use sasslint::{Finding, scan_dir, scan_file};

/// Every gate the defence relies on, newest first.
const RULES: [&str; 15] = [
    "no-tree", "no-relay-vec", "no-string-machine", "no-recursive-descent", "no-from-fn-driver",
    "no-work-queue", "mutable-accumulator-drives", "no-sort-for-order", "no-batch-shell",
    "require-err-channel", "require-slot-before-subscribe", "no-imperative-loop",
    "no-stage-layer", "no-else-if-chain", "single-table",
];

/// One planted source that carries every shape: the test asserts each id appears.
#[test]
fn every_rule_has_a_positive_control() {
    let dir = std::env::temp_dir().join("sasslint_defence_probe");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("bad.rs"),
        "pub struct Rule { pub children: Vec<Rule> }\n\
         fn relay(items: Vec<Rule>) -> Vec<Rule> { items }\n\
         fn subst() { let _ = stmt_text(); }\n\
         fn render(&self, r: &Rule) { self.render(&r.children[0]); }\n\
         fn drive() { let q = from_fn(|_| VecDeque::<Item>::new()); }\n\
         fn step(&mut self) { let s = Vec::<Frame>::default(); let _ = self.chain_taken; }\n\
         fn order(&mut self) { self.rows.sort_by_key(|r| r.order); }\n\
         fn batch<T>(s: T) -> Vec<T> { vec![] } \n\
         fn flat() { let _ = flat_map(|i| from_iter(vec![i])); }\n\
         fn eval(v: Option<i32>) { let _ = v.ok().unwrap_or_else(|| 1); }\n\
         fn hi(x: i32) -> i32 { if x > 1 { 2 } else if x > 0 { 1 } else { 0 } }\n\
         fn spin() { loop { } }\n\
         fn pipe() { let _ = merge(a, b).subscribe(|v| { let _ = v; }); }\n\
         type Stage<T> = Box<dyn Fn(T) -> T>;\n\
         fn drain() {} \n\
         fn to_base(u: &str) -> f64 { match u { \"px\" => 1.0, \"in\" => 96.0, \"cm\" => 37.0, \"turn\" => 360.0, _ => 0.0 } }\n",
    )
    .unwrap();
    let findings: Vec<Finding> = scan_dir(&dir);
    let missing: Vec<&str> = RULES
        .into_iter()
        .filter(|rule| !findings.iter().any(|f| f.rule == *rule))
        .collect();
    assert!(missing.is_empty(), "these gates have no working positive control: {missing:?}");
}

/// 真源不得被自己的规则标记：`crates/sasspile/src` 必须 `count=0`。
#[test]
fn production_tree_is_clean() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&dir);
    assert!(findings.is_empty(), "sasspile must stay gate-clean: {findings:?}");
}
```

- [ ] **Step 2: 跑到它通过**

Run: `cargo test -p sasslint --test defence_fitness`
Expected: PASS。若 `missing` 非空：说明某条规则的 needle 与植入形状不符——按缺失项调整植入源码**或**收窄规则，两者都要在 `crates/sasslint/tests/lint.rs` 留下对应断言；不许把该 id 从 `RULES` 里删掉来"过"。

- [ ] **Step 3: 全工作区 + 无行为变化证明**

Run:
```bash
cargo test --workspace --no-fail-fast 2>&1 | grep -a "^test result"
HRX_RUN=defence-s1s2 cargo test -p sasspile --test hrx 2>&1 | grep -a "hrx run measured"
cargo run -q -p ledger -- diff readonly-module defence-s1s2 2>&1 | grep -a "AB diff"
```
Expected: 全部 `test result: ok.`；`pass=1841`（与本计划开始时逐字相同）；`regress=0 net=0`。若 `pass` 变化 = 触碰了 `crates/sasspile/src`，回退。

- [ ] **Step 4: Commit**

```bash
git add crates/sasslint/tests/defence_fitness.rs
git commit -m "test(sasslint): the defence's own fitness function — every gate needs a planted counterexample"
```

---

### Task 4: `gc-auditor` 子代理（新鲜上下文重审）

**Files:**
- Create: `.qoder/agents/gc-auditor.md`（**位置需先核实**，见 Step 1）
- Modify: `.qoder/rules/rust.md`（Task 6 会引用这个入口）

**Interfaces:**
- Consumes: `git diff` 输出、`sasslint::scan_dir`（CLI：`cargo run -p sasslint --bin sasslint -- lint crates/sasspile/src`）、`sass-spec/**/*.hrx` 原文
- Produces: 结构化 finding：`file:line` + 规则/铁律条目 + 金样 case 名

- [ ] **Step 1: 核实宿主约定的位置与 frontmatter**

Run: 用 Skill 工具调用 `create-subagent`（它知道 Qoder 子代理的存放目录与字段），问法："为本仓库新增一个只做架构/铁律/证据纪律审查的子代理，输出格式与路径按宿主约定。"
Expected: 得到真实路径（下文按 `.qoder/agents/gc-auditor.md` 写；若宿主返回别的路径/文件名，用宿主给的那个，**不保留两个副本**）。

- [ ] **Step 2: 写定义（内容逐字可用）**

```markdown
---
name: gc-auditor
description: Review sasspile changes for GC/imperative structure regressions and evidence-discipline failures (F1–F6). Use after any batch that touches crates/sasspile/src, before committing, and before claiming a feature works. Does not edit code.
tools: Read, Grep, Glob, Bash
---

# gc-auditor

你是新鲜上下文的审查者，不是作者。你只看证据，不接受叙述。

## 输入（调用方必须给全）

1. `git diff` 的完整输出（或改动文件列表 + diff 片段）。
2. 相关 `sass-spec` 金样原文（case 名 + `<===>` 边界内的 input/output/error）。
3. 本次改动声称达成的读数（`ledger diff` 的 `net`/`regress`）。

缺任一项 → 直接返回 `INSUFFICIENT EVIDENCE` 并列出缺什么，不要开始审。

## 审查清单（逐条判定，禁止凭印象）

- **F4 结构**：`Vec<Rule>`/`.children`/递归自调用；`from_fn` 与 `VecDeque`/`Vec<Frame>` 同现；
  `type Stage`/`fn drain`/`fn fail`；行首 `for`/`while`/`loop`；`else if` 链；`sort_by_key` 为保序而存在；
  `.ok()`/`unwrap_or_else` 出现在求值路径；`Arc<Mutex<`、`&mut self` 被闭包共享。
- **F4 单一真源**：同一事实是否被写了两份（单位表、比较规则、数字打印律、色彩 route）。
  新写的 helper 必须能指向既存的那一份，否则算违规。
- **F2/F3 字面量**：测试与注释里每个 fixture 文本、case 名、期望值 —— 必须能在给定金样原文里逐字找到；
  找不到就是伪造。**没有金样的规则不许被钉成语料断言**（那是"我的选择"）。
- **F1/F5 读数**：声称的 `net`/`pass` 是否来自本次贴出的命令输出；有无引用旧 dump/旧摘要当证据。
- **F6 一致性**：改动是否声明了"哪几条用例因此变了"，并给出规范依据；只说"过了"不算。

## 输出格式（每条都要引用）

```
FINDING <n>
  kind: F1|F2|F3|F4|F5|F6
  where: <file>:<line>
  evidence: <引用原文或命令输出片段>
  golden: <sass-spec case 名，或 NONE>
  fix: <改数据分解/改发射时机/删第二份真源/补阳性对照 …>
```

无问题也要输出 `NO FINDINGS`，并列出你**实际读过**的文件与跑过的命令（`sasslint` CLI 必须跑过）。

## 禁止

- 不提这三类建议：放松/删除规则、改名以绕过规则、"先记 TODO 以后再改"。
- 不给无 `file:line` 的结论；不引用你自己的记忆当事实。
- 不修改任何文件（只读审查者）。
```

- [ ] **Step 3: 自检一次（真实调用）**

Run: 用 Agent 工具以 `gc-auditor` 审 `git diff HEAD~1..HEAD -- crates/sasspile/src`（当前 HEAD 属文档提交，预期 diff 为空）
Expected: 返回 `INSUFFICIENT EVIDENCE` 或 `NO FINDINGS` + 读过的文件清单。若它开始给无引用结论 → 收紧提示词中的输出格式段，再重跑。

- [ ] **Step 4: Commit**

```bash
git add .qoder/agents/gc-auditor.md
git commit -m "feat(qoder): gc-auditor subagent — fresh-context gate with citation-only findings"
```

---

### Task 5: `sasspile-start` skill（新会话起手式 = 机器读数 + 复述）

**Files:**
- Create: `.qoder/skills/sasspile-start/SKILL.md`
- Create: `.qoder/skills/sasspile-start/reference/gc-counterexamples.md`（从 `rust.md` 迁入的反例索引）
- Create: `.qoder/skills/sasspile-start/reference/evidence-discipline.md`（F1/F2/F3/F5 的核对清单）

**Interfaces:**
- Consumes: `cargo run -q -p ledger -- …`、`cargo run -q -p sasslint --bin sasslint -- lint crates/sasspile/src`、`openspec status --change … --json`、`crates/sasspile/tests/hrx.rs` 的 `SPEC_DIRS`
- Produces: 起手读数块（run 名、`total/pass/pass_substantive/pass_empty_expectation/pass_rate`、`unexpected error` 计数、`gc_scan` 命中数、下一个未做增量）

- [ ] **Step 1: 核实 frontmatter 形状**

Run: `head -12 .qoder/skills/openspec-apply-change/SKILL.md`
Expected: 看到真实字段 `name`/`description`/`allowed-tools`/`license`/`compatibility`/`metadata.author`，新 skill 用同一套（不发明字段）。

- [ ] **Step 2: 写 `SKILL.md`**

frontmatter（按 Step 1 的字段）内容：`name: sasspile-start`；`description` 写明"新会话/压缩上下文之后开始任何 sasspile 工作前使用；用户说『继续』或 /sasspile-start 时先跑它"；`allowed-tools: Bash(cargo:*) Bash(openspec:*) Read Grep`。

正文必须命令化（不是劝告）：

```markdown
## 起手四步（缺一不可，且必须复述读数）

1. 基线现读（禁止引用记忆里的数字）：
   `cargo run -q -p ledger -- runs --db=$PWD/reports/hrx.db`
2. 意外错误计数（FAIL→FAIL 只有它看得见）：
   `cargo run -q -p ledger -- failures <最新 run> "unexpected error" --count --db=$PWD/reports/hrx.db`
3. 结构闸：`cargo run -q -p sasslint --bin sasslint -- lint crates/sasspile/src`（期望 `count=0`）
4. 下一增量：`openspec status --change redesign-sasspile-rxrust-shared-stream --json` +
   `tasks.md` 最后一条已实测编号之后的第一条未做项。

然后**用一段话复述**：run 名、`pass`、`pass_substantive`、`pass_empty_expectation`、`pass_rate`、意外错误计数、
`gc_scan` 命中数。复述不上 = 没读，回去重跑。

## 动手前的三条判据

- 先选动词：`.qoder/skills/rxrust-reactive/SKILL.md` 的清单之外没有 `share()`/`and_then`；查不到动词 = 阻塞项，上报，不自造机制。
- 单一真源：写任何 helper 之前先 grep 既存真源（单位表 `unit::UNITS`、比较 `builtins/math.rs::compare`、
  route `builtins/color/modify.rs::route`、打印 `value.rs::format_number`）。
- 证据等级：只有本会话的工具输出算证据；旧摘要/旧 dump/记忆里的数字不算。

## 收尾

批次结束 → 跑 `gc-auditor` 子代理（引用 `file:line` 才收） → `ledger diff <base> <run>` 要 `regress=0` →
提交（feat 与 docs 分开 `git add`）。
```

- [ ] **Step 3: 迁两份 reference（移，不是复制）**

把 `.qoder/rules/rust.md` 的「反回归自查」条目原样搬进 `reference/gc-counterexamples.md`；把「验收读数也会骗人」整节搬进 `reference/evidence-discipline.md`。两份文件保留全部现有实测样本与数字（它们是唯一记录），迁移后 rust.md 里不得再有副本（Task 6 删）。

- [ ] **Step 4: 自检 skill 可用**

Run: 用 Skill 工具调用 `sasspile-start`
Expected: 它跑出四条命令的真实输出并被复述；复述数字与 `runs` 输出逐字一致。

- [ ] **Step 5: Commit**

```bash
git add .qoder/skills/sasspile-start
git commit -m "feat(qoder): sasspile-start skill — machine-read the baseline before touching code"
```

---

### Task 6: `rust.md` 瘦身（always-on 只留不可协商的三条）

**Files:**
- Modify: `.qoder/rules/rust.md`（长节已迁往 `.qoder/skills/sasspile-start/reference/`）
- Modify: `README.md`、`docs/sasspile-framework-design-report.md`（指向新位置，避免"同一律两处措辞"）

**Interfaces:**
- Consumes: Task 5 的两个 reference 路径
- Produces: ≤40 行的 always-on 规则头（新会话每次加载的就是这一份）

- [ ] **Step 1: 量现状**

Run: `wc -l .qoder/rules/rust.md; grep -n "^## " .qoder/rules/rust.md`
Expected: 得到节标题清单（决定哪几节可整体迁走）。实测写计划时为 **474 行**；开工时以命令输出为准，不拿这个数当现状。

- [ ] **Step 2: 迁移而非删除**

对每个迁走的节：把整节原文追加进对应 reference 文件（Task 5 已建的两份不够就新增
`reference/tooling-redlines.md`），然后在 rust.md 原位置留一行指针，格式固定：
`见 .qoder/skills/sasspile-start/reference/<file>.md 的「<节名>」`。
迁走对象（按 Step 1 清单）：「反回归自查」细目、「验收读数也会骗人」全部子条、色彩/math 的长篇定律复述
（**基线数字那一段留在 rust.md**：它是单一真源的唯一副本）。

- [ ] **Step 3: 写瘦身后的头部**

保留内容（逐字）：技术栈两行；基线数字段（现行 `pass=1841 / substantive=1676 / 16.0% / 意外错误 18 / run readonly-module`）；三条不可协商——
① 动手前必过 `sasspile-start` 起手四步并复述读数；② 禁 Python/`println!`、单文件 ≤500 行、`src/` 无内联测试；
③ 批次后必跑 `sasslint`（`count=0`）与 `gc-auditor`（finding 必须带 `file:line`），**不得放松闸或改名绕闸**；
外加"读数只属于产生它的那条命令 / 陈旧摘要不是证据"两行与指向 `design.md`+`tasks.md`+source-analysis+防线设计的指针。
总行数用 `wc -l` 验证 ≤ 40。

- [ ] **Step 4: 一致性检查（防"两处漂"）**

Run: `grep -rn "反回归自查\|验收读数也会骗人" .qoder README.md docs openspec | head`
Expected: 每个节名只出现在它的**新家**与被更新的那一处指针里；`README.md`/设计报告若仍在正文复述细节，改成指向 reference 路径（本计划自己遵守"一个事实一处"）。

- [ ] **Step 5: 门禁与无行为变化**

Run:
```bash
cargo test --workspace --no-fail-fast 2>&1 | grep -a "^test result"
cargo run -q -p sasslint --bin sasslint -- lint crates/sasspile/src 2>&1 | tail -1
openspec validate redesign-sasspile-rxrust-shared-stream --strict
```
Expected: 全 `ok`；`count=0`；`is valid`。

- [ ] **Step 6: Commit**

```bash
git add .qoder/rules/rust.md README.md docs/sasspile-framework-design-report.md
git commit -m "docs(rules): shorten the always-on contract and move the counter-example index to sasspile-start"
```

---

## 后续（不在本计划内，各自单独成计划）

- **S3 `crates/guard` MCP server**：依赖本计划的 `RULES` 清单（Task 3）与 ledger/sasslint 的真实读数面。
- **S4 `plugins/sasspile-discipline/`**：把 S1/S2/S3 打包成可分发插件 + `validate.sh`。
- **S5 hook 接线**：属宿主设置改动，需你单独批准（设计里已声明不擅自动 git/宿主配置）。

**冷启动验收（S1+S2 完成的判据）**：全新上下文只给一句"继续 sasspile"，连续三次都必须——先跑 `sasspile-start`、
复述的基线与 `ledger runs` 输出逐字相同、批次后触发 `gc-auditor` 且每条 finding 带 `file:line`。任一失败 = 补闸，
不写"下次注意"。
