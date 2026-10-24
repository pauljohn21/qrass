# Proposal

## Why

当前 `crates/sasspile` 的核心（`parse.rs` 的 `Acc{ 帧栈 Vec<Frame> + env + chain_taken }`
经 `scan_map` 携带、靠 push/pop 与可变标志**驱动**遍历）虽编译绿、已过 §4a/§4b 用例，但本质是
"穿了算子外衣的命令式状态机"——正是本项目反复返工的"下钻一层即滑回 GC"根因，且已被 `sasslint`
的 `mutable-accumulator-drives` 规则判为违规。同时，功能面远未覆盖参照语料：`sass-spec` 有约
2100+ 个非 libsass/non_conformant 的 `.hrx` 金样（core_functions / directives / values / css /
expressions / callable / operators / parser / variables），`element-plus` 有 316 个真实 `.scss`
待端到端编译；现核心仅覆盖嵌套/变量/插值/`@if`。

精读 rxrust `=1.0.0-rc.6` 源码后（结论沉淀在 `docs/rxrust-source-analysis/`）确认：`flat_map`
（流中流）、`Shared::subject`/`publish().ref_count()`（多播）、`scan_map`/`reduce_initial`
（owned 累加子）、`create`/工厂、`box_it`（类型擦除）、终点单次 `subscribe` 这些**内置原语本身就是
引擎**。因此应据此把整体架构一次重设计到位，而不是再在其上补状态机。

## What Changes

- **BREAKING** 核心重写：`compile` 入口改为**全程 `Shared` 冷 observable**——词法→结构化 token 流；
  解析把每个块/子流建成 owned 数据，其求值/渲染用 `flat_map` 扇出子 observable、`merge_observables`
  并发合并、`filter` 选支，env/选择器前缀作为随流 move 的 owned 入参下传；CSS 只在**唯一一次**
  终点 `subscribe` 合并产出。**移除** `parse::Acc` 帧栈折叠与可变 `chain_taken` 驱动。
- **BREAKING** 执行模型：采用 `Shared` + 多线程（tokio multi-thread）；并行扇出用 owned 文档序序号
  (`order`) 随流下传、终点按序确定重组，保证 expanded CSS 与 `sass-spec` 逐字节一致（见 design D3）。
- 一到多交给多播：模块 `@use` 单例一次编译喂 N 个引用方用 `Shared::subject`/`publish().ref_count()`；
  跨线程共享**冻结只读** `Arc<CompiledModule>`，绝不 `Arc<Mutex<业务态>>`。
- 能力面按参照补全：`@each`/`@for`/`@while`、`@mixin`/`@include`、`@function`/调用、`@extend`、
  `@media`/`@supports`、`@use`/`@forward`、内置函数与值类型（Number/List/Map/Color/Bool/Null）、
  运算符与单位、字符串/插值——逐项以 `sass-spec` 对应目录 `.hrx` + `element-plus` 钉行为。
- 保留可复用件：`lexer`（结构化 token 流）、`value`（`Value`/`to_css`）、`expr`（`Expr` 折叠 +
  惰性 `eval`）；在其上补 `Bin/Func/And/Or/If`、算术与内置函数白名单。
- 唯一行为参照 = `sass-spec/*.hrx`（`input.scss`→`output.css`/`error`）+ `element-plus`；
  **不**参照 dart-sass 源码/文档/物化-遍历心智，**不**参照旧 `qrass`。

## Capabilities

### New Capabilities
- `scss-compilation`：SCSS→expanded CSS 的对外行为契约——嵌套/`&`、变量、插值、控制流、mixin/
  function/extend、`@use` 模块、内置函数与值类型、运算符与单位、注释处理、错误形态；每条断言可回溯
  到具体 `sass-spec` 金样与 `element-plus` 语料。
- `reactive-stream-architecture`：系统作为**一条 Shared 惰性 owned 值流**的体系结构/质量属性契约——
  每一层都是流、循环=flat_map、状态=owned 累加子、一到多=多播、终点单次 subscribe、禁止物化-再-遍历
  与手搓驱动；由 `sasslint` 确定性把关。
- `sasspile-conformance`：验收闸契约——行为只由 `sass-spec` + `element-plus` 决定；每个增量
  `ledger diff regress=0`；`sasslint` 命中数 0；测试为 `#[test]` 置于 `crates/sasspile/tests/`。

### Modified Capabilities
<!-- openspec/specs/ 目前为空（无已归档主 spec），本变更全部为新增能力，无既有能力需求被修改。 -->

## Impact

- Affected specs（新建）：`scss-compilation`、`reactive-stream-architecture`、`sasspile-conformance`。
- Affected code：`crates/sasspile/src/`（`lib`/`parse` 重写为 `Shared` 流核心；新增块求值/渲染流模块；
  `expr`/`value` 扩展算术与内置函数；`render`/`lex` 适配），可能新增 `tokio`（rt-multi-thread）依赖。
- Affected tools：`crates/sasslint`（规则已含 `mutable-accumulator-drives`/`no-from-fn-driver`/
  `no-work-queue`/`no-tree`/`no-recursive-descent`/`no-relay-vec`/`no-string-machine`，作为 CI 闸）；
  `crates/ledger`（在新核心下重立基线，再按 hrx 目录簇 `regress=0` 增长）。
- Reference source（唯一）：`sass-spec/`、`element-plus/`。
- 风险：核心重写期回归 → 全程 `regress=0` 提交闸，达不到即回退绿色基线（`@if` 已在）；`Shared` 的
  `Send+Sync+'static` 与并发保序是新约束，遇阻先假设分解错，不退回累加子驱动（见 design Risks）。
