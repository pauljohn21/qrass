# Proposal: 用 Shared 流 + flat_map/merge 重写 sasspile 核心（根除帧栈式状态机）

## Why

上一版核心虽编译绿、`sasslint` 0，但它是 `scan_map` 携带一个可变 `Acc{ 帧栈 Vec<Frame> + env + chain_taken }`、靠 push/pop 与跨条目可变标志来**驱动**流程——即"穿了算子外衣的命令式状态机"，正是"下钻一层就滑回 GC"的老毛病。多线程/`Shared` 约束已解除，现在应当把嵌套、循环、分支、共享**真正表达成流的扇出与合并**（`flat_map` 流中流 + `merge_observables` + `Shared::subject`），让**每一层**都是流，而不是一个累加子在驱动遍历。

## What Changes

- **破坏性**：以 `Shared` 流式核心替换现有 `parse::Acc` 帧栈折叠——`compile_string` 改由"token 事件流 → `create`/`flat_map` 扇出子流 → `merge_observables` 合并 → `Subject` 多播 → 终点一次 `subscribe` 合并出 CSS"构成；不再有 push/pop 帧栈、不再有可变 `chain_taken` 驱动。
- 嵌套 = 子块作为子 observable 经 `flat_map` 挂回主流；`@each/@for` = `from_iter(items).flat_map(item → body 子流)`；`@if/@else` = `filter`/条件选子流；选择器前缀与 env 作为**随流 move 的 owned 值/闭包入参**下传，不放进可变累加子。
- 跨线程共享只放**冻结不可变值**（`Arc<已编译模块>` 等只读），绝不 `Arc<Mutex<业务态>>`。
- 保留既有可复用件：`lexer`（结构化 token）、`value`（`Value`/`to_css`）、`expr`（`Expr` 折叠 + 惰性 `eval`），在其上补 `Bin/Func/And/Or/If` 与算术。

## Impact

- Affected specs: `rxrust-shared-stream-contract`、`sasspile-pipeline`、`conformance-and-audit`。
- Affected code: `crates/sasspile/src/{lib,parse} → 新的流式核心模块`、`expr`/`value` 扩展；`sasslint` 增"可变累加子驱动/帧栈"启发式检查；`ledger` 基线在新核心下重立。
- 风险：核心重写期可能回归 → 全程 `ledger diff regress=0` 为提交闸，达不到回退到当前绿核（`@if` 已在）；`Shared`/`Send` 化会暴露 `'static`/装箱约束，遇阻先假设分解错、不退回累加子驱动。
- 依赖：`rxrust` `=1.0.0-rc.6` 的 `Shared`、`merge_observables`、`Shared::subject`、`create`。
