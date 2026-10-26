# Spec Delta — rxrust-tutorial-verification（覆盖率、字数与可编译验证闸）

## Purpose

把"逐行讲完 2 万行源码"与"总字数 ≥100 万"变成**机器可核对**的账：一个只读统计工具 + 一份行级
覆盖账本 + 每课可编译验证。没有这套账，教程必然退化成"挑重点讲概念"。

## ADDED Requirements

### Requirement: 唯一出口的统计工具 docsizer

项目 MUST 提供 `crates/docsizer`（Rust，只读文件；MUST NOT 使用 Python/shell 脚本处理统计），
它 MUST 支持三类子命令并各以 `tracing` 输出结果（MUST NOT `println!`）：
- `inventory --src <rxrust-src>`：产出可教行清单（`文件:行` + 该行首 tokens），
  MUST 排除 `#[cfg(test)]` 模块内代码与纯 `use`/右花括号占位行，MUST 记录排除计数；
- `coverage --ledger COVERAGE.tsv --inventory <file>`：报告 `已归属行 / 可教行`、未归属行清单、
  重复主归属清单；
- `words --dir docs/rxrust-from-scratch`：报告**非代码字数**（扣除 ``` 代码块与表格分隔符后的
  可见字符数）与总字符数两个口径。

#### Scenario: 覆盖率必须 100%
- **WHEN** 跑 `docsizer coverage`
- **THEN** 输出 `unassigned = 0` 且 `duplicates = 0`；任一阶段交付时未归属行 MUST 被排进后续课

#### Scenario: 排除口径可审计
- **WHEN** `docsizer inventory` 排除了某行
- **THEN** 报告给出排除原因分类计数（test 模块 / 纯 use / 纯括号），使"可教行"总数可复核

### Requirement: 行级覆盖账本

`docs/rxrust-from-scratch/COVERAGE.tsv` MUST 存在且每行为
`<源文件相对路径>\t<起行>-<止行>\t<课号>\t<主/复习>`；MUST 满足：
可教行 100% 有唯一主归属；每课声明的区间与账本一致；账本 MUST 在版本库内可 diff（纯文本）。

参照体量：`rxrust-1.0.0-rc.6/src` 共 29,755 行（`ops` 15,137 / 52 文件、`observable` 3,665、
`subject` 2,064、`subscription` 1,048、顶层 `observable.rs` 2,555、`factory.rs` 1,256、
`scheduler.rs` 1,089、`context.rs` 732、`observer.rs` 379）。

#### Scenario: 一课的完成定义
- **WHEN** 某课被标记完成
- **THEN** 其覆盖区间在账本中登记为"主"，且该课文内每行都有讲解锚点（见 lesson-format spec）

### Requirement: 字数下限与其推导口径

全教程**非代码字数** MUST ≥ 1,000,000 字符；该数字 MUST 由覆盖率推导（约 2 万可教行 × 每行
≥ 55 字符真讲解 + 前置卷/练习/报错解读），MUST NOT 通过复述源码、粘贴原文档、重复段落达成。
`docsizer words` MUST 用作每卷验收闸。

#### Scenario: 反灌水
- **WHEN** 某卷字数达标但逐行锚点缺失（`coverage` 通过而 lesson-format 不通过）
- **THEN** 该卷判定为未达标；字数 MUST NOT 单独作为完成证据

#### Scenario: 每卷字数预期
- **WHEN** 完成 V1–V6 任一卷
- **THEN** 报告该卷非代码字数，并核对落在预期区间（V1 ≥ 12 万、V3–V5 各 ≥ 20 万），异常偏低即视为漏讲

### Requirement: 每课代码真实编译验证

每课发布前，作者 MUST 把该课全部完整代码在**临时 crate**（`target/minirx-check/`，不入库）
依次累积编译：MUST `cargo build` 与 `cargo test` 通过、MUST `cargo clippy -- -D warnings` 干净；
MUST 保留该课的验证结论（通过/失败 + 关键报错）到 `docs/rxrust-from-scratch/VERIFICATION.tsv`。

#### Scenario: 累积性验证
- **WHEN** 第 N 课完成
- **THEN** 验证用的是 1..N 课的累计代码，而非孤立片段（防后课破坏前课）

#### Scenario: 教程内代码与验证代码一致
- **WHEN** VERIFICATION 记录某课通过
- **THEN** 其代码与课文中的代码逐字一致（改动后 MUST 重新验证并更新记录）

### Requirement: 不侵入产品代码

本变更 MUST NOT 修改 `crates/sasspile` 的源码或行为；MUST NOT 引入新的产品对外 API；
`docsizer` MUST 为独立 bin crate 且只被教程验收使用。

#### Scenario: 回归基线不受影响
- **WHEN** 本变更任一阶段交付
- **THEN** `sasspile` 的 `cargo test --all-targets`、`clippy -D warnings`、`sasslint count=0`
  与 hrx `regress=0` 仍然成立

### Requirement: 可读性闸与编译验证同等重要

每课交付 MUST 同时满足三件事：① 教学代码在 workspace 之外**真编译真运行**，课文抄录真实输出
（含真实报错原文）；② `docsizer terms --strict` 对已发布课文目录通过（先用后定义 = 0、超预算 = 0）；
③ 若教具必须使用被项目规则限制的设施——例如给零基础读者看屏幕输出所需的 `println!`——
MUST 在 design 中显式登记该例外及其边界（**仅限 `docs/` 下、在 workspace 外编译的教具**），
不得据此放宽产品 crate 内的同一禁令。

#### Scenario: 只有绿灯但没有过可读性闸

- **WHEN** 课文代码全部编译通过，但正文存在未定义先用的行话
- **THEN** 该课 MUST 判为未交付；`terms --strict` 的非零退出码即证据

#### Scenario: 字数与覆盖率达标仍读不懂

- **WHEN** `words` 与 `coverage` 都通过，`terms` 报出大量先用后定义
- **THEN** MUST 以 `terms` 为准判定教程不合格，并把它当作重写的工作清单
