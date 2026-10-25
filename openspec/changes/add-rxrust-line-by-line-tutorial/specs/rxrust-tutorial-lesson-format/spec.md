# Spec Delta — rxrust-tutorial-lesson-format（逐行讲解格式契约）

## Purpose

规定**每一课的写法**：不是"讲概念 + 贴源码"，而是把该课负责的每一行源码逐行讲清——它在做什么、
为什么必须这样写、不这样写会出什么问题、读者自己写时会撞到的编译错。本文件是教程可跟随性的硬闸。

## ADDED Requirements

### Requirement: 每一行都被讲到（逐行覆盖）

每课 MUST 为其负责的**每一行**源码给出一句话讲解（行号锚点 + 该行的作用 + 存在理由），
MUST NOT 出现"这段很明显/略过不表/同理可得"式跳行；重复结构（如宏生成的多个同形 impl、
`impl` 成对的 `next/error/complete/is_closed`）MUST 用"讲透第一个 + 逐条列出其余差异点"的方式覆盖，
不得整段跳过。

参照：`observer.rs:19-43`（四方法）、`observer.rs:117-125` 与 `:171-179`（blanket impl 与宏生成形）。

#### Scenario: 行级台账
- **WHEN** 一课声明覆盖 `ops/map.rs:1-120`
- **THEN** 文内可找到 1–120 每一行的讲解锚点（允许合并相邻同类行，但合并段 MUST 显式标注行区间）

#### Scenario: 宏与生成代码不被跳过
- **WHEN** 源码用 `macro_rules!`（如 `impl_observer_for_box!` `observer.rs:133-169`）
- **THEN** 教程展开该宏在本库中生成的每一种形态，并解释"为什么要宏而不是手写"

### Requirement: 先写出来，再读源码（顺序不可反）

每课 MUST 按此顺序组织：`①本课要解决的问题（含不解决会怎样）→ ②读者自己先试 → ③最小实现完整代码
（可编译）→ ④逐行讲解自己的代码 → ⑤贴出 rc.6 对应源码并逐行对照（含 `文件:行`）→ ⑥差异与设计取舍
→ ⑦编译报错解读 → ⑧练习 + 参考答案`。

#### Scenario: 不允许先贴源码再解释
- **WHEN** 一课直接以 rc.6 源码片段开篇
- **THEN** 不合格，必须重排为"先自写后对照"，否则读者只是在读源码而非在建构它

#### Scenario: 最小实现可编译
- **WHEN** 读者按 ③ 的代码敲进 `minirx`
- **THEN** `cargo build` 与 `cargo test` 通过（作者侧 MUST 用临时 crate 真实验证后才发布该课）

### Requirement: 编译报错是教学内容

每课 MUST 预判并列出零基础读者最可能撞上的 **≥3 个**真实编译错（原文错误码与信息，逐条解释成因与修法），
至少 MUST 覆盖：trait 边界/借用检查、`Send`/`Sync` 与 `Shared` 相关的边界（V3 起）、
object-safety（`dyn` 相关，V1/V6 相关处）。

#### Scenario: 报错复现路径明确
- **WHEN** 教程给出一个报错解释
- **THEN** 同时给出"怎么故意改坏代码来复现它"的一步操作，读者可自行验证

### Requirement: 术语与命名一致性

术语首次出现 MUST 给中文解释 + 英文原词；同一概念全教程 MUST 用同一中文词
（如 `Observer` 统一"观察者/消费者"择一、`Subscription` 统一"订阅"、`Item` 统一"元素"、
`Context` 统一"上下文(线程模型容器)"）；`minirx` 中的标识符 MUST 与 rc.6 同名，
差异 MUST 在"设计取舍"里点名说明，MUST NOT 悄悄改名。

#### Scenario: 术语表可核对
- **WHEN** 完成任一卷
- **THEN** 该卷新增术语已汇总进 `docs/rxrust-from-scratch/GLOSSARY.md`，且可回溯到首现课次

### Requirement: 反例来自本项目真实踩坑

每卷 MUST 至少包含一节"反例"，把本项目真实发生过的 rxrust 误用写成"错误代码 + 为什么它是改造框架
+ 正确写法"，至少覆盖：把状态私藏进自造对象（`scan_map` 误用 vs `scan`）、用配对/多播算子携带
位置状态（实测 `with_latest_from` 配到终值）、`.ok()` 吞错误通道、`collect` 后再排序的收集器思维、
在算子上面再包一层 `Stage`/`box_it` 边界壳。

参照：`crates/sasspile/tests/{pairing.rs,multicast.rs}`（本项目实测断言）。

#### Scenario: 反例可判定
- **WHEN** 一课给出反例
- **THEN** 同时给出"哪条铁律/哪个 `sasslint` 规则会拦下它"，读者能在自己项目里复查
