---
name: sasspile-start
description: Machine-read the ground truth before touching sasspile. Use at the start of any session (or after context compaction) before implementing, measuring, or claiming that a feature works, and whenever the user says "继续" on the sasspile project. It re-reads the hrx baseline from the ledger, re-runs the deterministic gates, prints the next unfinished task, and requires the readings be recited verbatim.
allowed-tools: Bash(cargo:*), Bash(git:*), Bash(openspec:*), Read, Grep, Glob
license: MIT
compatibility: Requires cargo, the `ledger` and `sasslint` workspace crates, and reports/hrx.db.
metadata:
  author: qrass
---

# 起手式：先取真值，再动手

本仓库的失败史告诉我们：**不是忘了规则，是拿了不该信的东西当依据**（本会话实测六类：陈旧摘要当证据、
伪造字面量、把"我的选择"钉成语料律、结构退回 GC、读数来自不产生它的命令、深会话尾部判断力衰减）。
所以本 skill 不重复教条，只做一件事：**把真值变成一次命令，并要求你复述它**。

`always-on` 的文件有两个：`.qoder/rules/rust.md`（技术栈、基线数字、过程门禁、硬性禁止、铁律、指针）
与 `.qoder/rules/codegraph.md`（代码查询先走 CodeGraph），**两份都很短，起手时全文读完**。其余细则全在本目录按需加载。（本行以前还写这两份各多少行 —— 已删：行数每次改动就漂移，是又一处会过期的副本。）

## 起手四步（缺一不可，跑完必须复述读数）

1. **基线现读**（禁止引用记忆里的数字）：

   ```bash
   cargo run -q -p ledger -- runs --db=$PWD/reports/hrx.db | tail -5
   ```

   它给的是 run 名、`pass/total`、百分比。**`pass_substantive` 不在 `runs` 里** —— 它只由
   `cargo test -p sasspile --test hrx` 的读数行输出（不设 `HRX_RUN` 时只量不写、不留产物，约几秒），
   要引用它就现跑一次，而不是引用旧行。

2. **意外错误计数**（"FAIL 变成另一种 FAIL"只有它看得见）：

   ```bash
   cargo run -q -p ledger -- failures <最新 run> "unexpected error" --count --db=$PWD/reports/hrx.db
   ```

3. **确定性闸**（期望 `count=0`，以退出码为准，不许用无条件 `echo` 冒充通过）：

   ```bash
   cargo run -q -p sasslint --bin sasslint -- lint crates/sasspile/src
   ```

4. **下一增量**：

   ```bash
   openspec status --change redesign-sasspile-rxrust-shared-stream --json | tail -20
   ```

   再读本目录 `reference/state-of-the-port.md` 与
   `openspec/changes/redesign-sasspile-rxrust-shared-stream/tasks.md` 里最后一条**已实测**编号之后的第一条未做项。

**然后复述**：run 名、`total`、`pass`、`pass_rate`（来第 1 步）、`pass_substantive` 与
`pass_empty_expectation`（来现跑的 hrx 读数行）、意外错误计数（第 2 步）、`sasslint` 命中数（第 3 步）、
下一条要做什么（第 4 步）。复述与命令输出对不上 = 没读，回去重跑。

## 动手前的三条判据

- **先选动词**：控制流必须用 rxrust 自己的算子表达（见 `.qoder/skills/rxrust-reactive/SKILL.md`）。
  rc.6 没有 `share()`/`and_then`/`take_until_by`；查不到对应动词就是**阻塞项，上报，不许自造机制**。
- **单一真源**：写任何 helper 之前先确认既存的那一份 ——
  单位换算表 `crate::unit::UNITS`、数字比较 `builtins/math.rs::compare`/`compare_named`、
  色彩通道写法 `builtins/color/modify.rs::route`、无单位拒语 `builtins/math.rs::unitless`、
  数字打印 `value.rs::format_number` + `Unit::calc_product`、带命名空间的名字 `lexer::Tok::MemberVar`。
  **新增第二份副本就是回归本身**（`sasslint` 的 `single-table` 会拦一部分，剩下的靠你查）。
- **证据等级**：只有本次会话里某条命令的输出算证据。旧摘要、旧 dump 的记忆、"上次看到"的数字都不算；
  任何"为 0"的读数必须先过一条阳性对照（先确认这个 0 是量具能测出来的那种 0）。

## 按需加载的细则

| 什么时候读 | 文件 |
|---|---|
| 写/改管线代码之前，以及自查是否退回命令式 | `reference/gc-counterexamples.md`（反例索引，逐条 grep） |
| 要报任何数字（通过率、net、行数、命中数）之前 | `reference/tooling-redlines.md`（读数陷阱、A/B 记录、量具覆盖面） |
| 决定"下一步做什么"、某特征是否已实现 | `reference/state-of-the-port.md`（移植现状长叙述） |

## 收尾

1. 全量测试：`cargo test --workspace --no-fail-fast`（看 `exit`，不要只看最后一条 `test result` 行）。
2. `HRX_RUN=<增量名> cargo test -p sasspile --test hrx` → `cargo run -q -p ledger -- diff <base> <增量名>`，
   要求 **`regress=0`**；行为变更必须点名**哪几条用例**变了及其金样依据，并在**另一个空库**复算一次。
3. `cargo run -q -p ledger -- changed <base> <增量名> [簇前缀]`：接住 `diff` 结构上看不见的一类——
   **两侧都失败、但失败详情变了**。每个 `CHANGED` 逐条定性：“got 靠齐了该行自己 `expected error` 引用的
   拼写”（可接受，仍 FAIL），还是“逐字 → 错值”/“发明错误”（不可接受 → 整批回退）。本会话两次实证：
   `regress=0 net=11` 与“`abs::overridden` 算出金样明令相反的值”共存，只有这条读数看得见。
4. 跑 `gc-auditor`（`.qoder/agents/gc-auditor.md`）审本批次 diff：finding 必须带 `file:line`，
   涉及行为的必须带金样 case 名；它的"没问题"结论不替代第 2、3 步的读数。
5. 提交：feat 与 docs 分开 `git add`；推送用 `git push github main`。
