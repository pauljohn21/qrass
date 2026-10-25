# Proposal

## Why

项目需要一个**面对“从未写过任何代码”读者**的 rxrust 实战教程（读者起点于 2026-10-25 由用户确认）：
读者跟着它一行一行亲手敲，最终写出与 `rxrust = 1.0.0-rc.6` 等价的完整库。

**为何需要重写前置卷**：旧“零基础 V0”五课被用户直判“完全看不明白”，实测坐实：
`docsizer terms` 报出 **147 处行话先用后定义**（第 1 课单课就用了 27 个未定义专词，`trait` 全卷出现 72 次），
每课引入 **16~22 个新概念**（预算 2）。字数、行覆盖、编译全绿却读不懂，说明验收缺了“读者能跟上”这一项。
本变更因此补上可读性闸（`docsizer terms`）与前置重写清单（tasks 的 **V0R** 节）。

而已有的 `docs/rxrust-source-analysis/` 只回答"每个文件是什么"，仍不满足"每一行都讲到、且读者真能跟上"。

更重要的动机是**方法论**：本项目反复失败的根因是"先凭经验写代码、事后才查框架"（把 rxrust 当成
承载自造机器的运输带，甚至改造成一个 GC）。逐行教程强制回答"这一行为什么必须存在、不这样写会怎样"，
它是把"以框架为设计输入"这件事变成可复核工件的唯一手段。

规模由体量推导：rc.6 `src` 共 **29,755 行 / 88 个文件**（`ops` 15,137 行 / 52 文件，`observable` 3,665，
`subject` 2,064，`subscription` 1,048，顶层 `observable.rs` 2,555 / `factory.rs` 1,256 /
`scheduler.rs` 1,089 / `context.rs` 732 / `observer.rs` 379 …）。`docsizer inventory` 实测：
**可教行 14,116**（排除 `#[cfg(test)]` 11,644、空行 1,677、纯 `use` 239、纯括号 2,079，逐类可对账）。
每行平均 ≥70 字真讲解≈ 98.8 万字，加 V0 前置卷与练习/编译错解读，**总字数自然超过 100 万**（预期 ≈135 万）。
因此字数是覆盖率的结果与下限校验，绝不作为灌水目标（见 spec 的反灌水条款）。

## What Changes

- 新增文档产物 `docs/rxrust-from-scratch/`：**7 卷 / 58 课**（`docsizer plan` 实测，`coverage 100.00%`），
  逐行覆盖 rc.6 全部 14,116 行可教源码，终点是读者手上一个功能等价的 `minirx`（含
  `context/factory/observable/observer/ops/rc/scheduler/subject/subscription` 全套骨架与 52 个 `ops` 能力）。
- 新增**覆盖账本** `docs/rxrust-from-scratch/COVERAGE.tsv`：每一行源码唯一归属到一课，
  由新工具 `crates/docsizer`（Rust，禁脚本）机器核对"覆盖率 100% / 无重复归属 / 非代码字数 ≥ 100 万"。
- 新增课程与格式契约（3 份 spec）：零基础可跟随性（术语首现即释、不引用未教的 Rust 特性、
  每课代码必须真实编译通过）、逐行讲解最小要求、反灌水与字数口径。
- 前置草稿 `docs/rxrust-from-scratch/README.md`、`01-*.md` **不视为已完成**：按新契约重写
  （现稿未达逐行密度、也未做可编译验证）。

无 BREAKING：本变更**只新增文档与一个只读统计工具**，不改动 `crates/sasspile` 任何产品代码，
不改变其对外行为（`compile` 仍是唯一出口）。`crates/docsizer` 是新增独立 bin，只读文件、不写产品码。

## Impact

- 新增：`docs/rxrust-from-scratch/**`、`crates/docsizer/**`、workspace `members` 增加一项。
- 不影响：`crates/sasspile`、`crates/sasslint`、`crates/ledger` 的代码与 hrx 基线（`regress=0` 仍成立，
  本变更不触碰其源码）。
- 资源：58 课× 每课 ≈ 240 可教行（由 `--target 300 --max 600` 预分配），每课 2–3 万字；
  每阶段以 `docsizer` 报告为闸（不达标不进入下一阶段）。

## Open Questions（应用阶段开始前需定稿）

1. `minirx` 是否入库：默认**不入库**（教程代码用 `target/` 下临时 crate 编译验证，避免与产品码混淆）；
   若你希望读者有"参考实现"可对照，则改为新增 `crates/minirx` 并保持零依赖、每课一个 commit。
2. 是否包含 rc.6 的 `mut_ref` 家族（`Observer<&mut T>` / `BoxedObserverMutRef*`）与
   `AsyncSubject`/`ReplaySubject`：默认**包含**（"完整 rxrust"的字面要求），但排在最后两卷。
