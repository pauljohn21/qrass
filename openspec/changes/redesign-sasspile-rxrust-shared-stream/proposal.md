# Proposal

## Why

本变更最初是"把 `parse::Acc` 帧栈换成 `Shared` 流"的**局部**重设计。把整个 `sass-spec`
系统性吃透后确认:问题不是某个累加子,而是**缺一份覆盖全语言的框架设计**——值是带单位/calc/
全色彩空间的代数,选择器需结构化才能 `@extend`/`selector.*`,模块/mixin/function 是独立子系统,
`@media` 要冒泡合并,诊断(warn/deprecation/error)也是行为面。此前"来一个 directive 加一个"
每加一个横切能力就动一次骨架 → 反复返工。故本变更**升格为全语言框架设计**:`design.md` 给出数据
模型 + 分层管线 + 各子系统契约 + 并发/保序 + 分阶段实现计划;实现按依赖序 P0–P8 推进、每阶段以
`sass-spec` hrx 回归 `regress=0` 为门。行为唯一参照 = `sass-spec/*.hrx` + `element-plus`,
**不**参照 dart-sass 源码/物化-遍历心智,也**不**参照已删除的 qrass。

## What Changes

- **框架先于实现**:先定稿 `design.md`(全语言数据模型 + 6 相管线 + 子系统契约 + 并发/保序 + 阶段),
  不再局部戳补丁。详见 `design.md`。
- **入口契约**:`async fn compile(...) -> Result<String, SassError>`(携诊断),终点 `into_future().await`;
  无同步 `block_on`/`mpsc` 壳;runtime 由调用方提供。**仅模块加载用真并发**(独立子图 + order 键保序)。
- **执行边界诚实化**:承认真流/假并发之分——非模块求值是"对物化 owned 结构的算子/`fold`/`flat_map`
  pass";`@extend`/`@media` 是**全局多趟 pass**(非 `Acc` 状态机、非递归下降遍历)。
- **现状处置**:未提交核心重构(`lib.rs` 的 `Shared` `compile`、`structure`/`eval` 独立块求值 +
  `@each/@for`/`@if`、`tests/hrx.rs` harness、`ledger` 基线)是 **P1/P3 可复用子集**,非丢弃重写;
  旧 `parse.rs`/`render.rs` 已删。未提交的一阶 `@extend`(带 borrow 错)归入 P6 正确实现。
- **能力面按阶段补全**(依赖序):P0 harness(多文件/`options.yml`/诊断断言)→ P1 值/单位/calc/color →
  P2 结构化 selector → P3 管线/控制流/媒体冒泡/作用域 → P4 callable(mixin/@content/function)→
  P5 模块(use/forward/import + 并发 + 多播)→ P6 `@extend` → P7 原生 CSS 透传 + 诊断 → P8 CSS Color 4。

## Capabilities

### New Capabilities
- `scss-compilation`：SCSS→expanded CSS 的对外行为契约——嵌套/`&`、变量、插值、控制流、mixin/
  function/extend、`@use` 模块、内置函数与值类型、运算符与单位、注释处理、错误形态；每条断言可回溯
  到具体 `sass-spec` 金样与 `element-plus` 语料。
- `reactive-stream-architecture`：系统作为**流 + 全局 pass 的混合式分层管线**的结构/质量属性契约——
  循环=flat_map、状态=owned 累加子、一到多=多播、真并发仅模块加载、终点 `into_future` 收口、
  禁止 `Acc` 驱动与递归下降遍历;由 `sasslint` 确定性把关。
- `sasspile-conformance`：验收闸契约——行为只由 `sass-spec` + `element-plus` 决定;每阶段
  `ledger diff regress=0`;`sasslint` 命中 0;测试为 `#[test]` 置于 `crates/sasspile/tests/`。

### Modified Capabilities
<!-- openspec/specs/ 为空(无归档主 spec),本变更全部为新增能力。 -->

## Impact

- Affected specs(新建):`scss-compilation`、`reactive-stream-architecture`、`sasspile-conformance`。
- Affected code:`crates/sasspile/src/`(`lib` = `Shared` `compile` 入口;`lexer`/`value`/`expr`/
  `value_parse`/`builtins`/`structure`/`eval` 为私有内部件,按 P1–P8 扩成全语言模型/管线)。
- Affected tools:`crates/sasslint`(铁律 CI 闸)、`crates/ledger`(hrx A/B 基线 + `regress=0`)。
- Reference source(唯一):`sass-spec/`、`element-plus/`。
- 风险:阶段内允许 pass 数波动但**不得**把已过例改坏(`regress=0`);诊断文本达标比"出对 CSS"更难,
  单列一相;并发保序用 order 键 + 稳定合并(见 design「并发/保序」节),不靠 `from_stream` 侥幸。
