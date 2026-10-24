# Design: Shared 流式核心（每层都是流，扇出+合并去递归）

## Context

约束变更：wasm32 不再是唯一目标，允许多线程与 `Shared`。上一版核心是 `scan_map` + 可变 `Acc{Vec<Frame> 栈, env, chain_taken}` 驱动——编译绿但本质是"算子外衣下的状态机"，是"下钻即 GC"的产物。本次把结构（嵌套/循环/分支/共享）**改写成流的扇出与合并**，让每一层都只用 rxrust 既有算子表达。

## Goals / Non-Goals

- Goals：核心 = 一条 `Shared` 冷 observable 链；嵌套/`@each`/`@if`/多模块分别是 `flat_map`/`merge_observables`/`filter`/`Shared::subject`；env 与选择器前缀**作为随流 move 的入参/闭包捕获**，不是可变累加子字段；终点一次 `subscribe`；`sasslint` 0 + `regress=0`。
- Non-Goals：不为"能编译"退回累加子驱动 / 帧栈 / AST 解释器；不用 `Arc<Mutex>` 共享业务态。

## Decisions

### D1 — 每一层都套同一问句（治本）
不只顶层"是条流"：每解一个子问题（env 跨兄弟、块展开、分支、共享）都再问"这里流什么 owned 值、用哪个现成算子"。判据（强制）：一旦想"先攒结构再遍历"或"给累加子加一个可变字段/标志来驱动"——**即使 `sasslint` 不报**——即视为失守，改回该层用流表达。

### D2 — 块即子流：嵌套与选择器前缀随流下传
`fn block_stream(block, prefix: SelectorPrefix, env: Env) -> SharedBoxedObservable<Css>`。父块对每个子块 `flat_map`：把"已组合的 selector 前缀 + 该作用域 env 的**快照值**"作为参数 move 进子流。无 `Vec<Frame>` 栈 push/pop。

### D3 — 循环 = `from_iter(items).flat_map(item → body_stream)`
`@each/@for`：把 body 写成"以 item 为入参的流工厂"，`items` 作为流，`flat_map` 逐条展开合并；循环变量绑定进传入 body 的 env 快照（函数式、按值），不缓存 body 再重放。`@for` 的整数区间 = `from_iter(range)`。

### D4 — 分支 = `filter` / 条件产出子流或 `empty()`
`@if/@else if/@else`：按条件真值，选中支返回其子流、未选支返回 `Shared::empty()`。`@else` 链的"是否已被前面分支命中"用一个**随兄弟流 move 的不可变标志值**（由 `scan`/fold 产出的累积量传递），不是 `Acc` 上的可变 `chain_taken`。

### D5 — 兄弟顺序 + env 文档序 = 一次性 fold 出"带快照的兄弟序列"再 `flat_map`
为保变量文档序可见，一个块内先对该层 `Vec<Stmt>` 做**一次纯 fold**（输入 `Vec`、输出 `Vec<(EnvSnapshot, Stmt)>`，fold 内按序累积绑定）——fold 是数据变换、不是驱动状态机；随后 `from_iter(...).flat_map(...)` 展开。并发分支处用 `merge_observables`。

### D6 — 共享 = `Shared::subject` / `Arc<不可变>`
模块单例、一段解析喂多消费者：`Shared::subject`/`share`/`publish().ref_count()`；跨线程共享的编译产物用 `Arc<T>`（只读冻结），MUST NOT `Arc<Mutex<业务态>>`。

### D7 — 惰性 + 终点合并
整链冷 observable：`Expr`/selector 以未求值 owned 值流动，`Value`/CSS 文本仅在终点 `subscribe` 的 `map` 里 `eval`→`to_css`；`reduce`/`collect` 合并成字符串，一次 `subscribe`。

### D8 — `Send`/`'static`/装箱按 rxrust 正解处理
`Shared` 要求 `Item/Err/闭包: Send+Sync+'static`：值模型保持 `Clone + Send`（`Value`、`Token`、`Stmt` 皆 owned 数据，天然满足）；跨签名 `SharedBoxedObservable`；遇 `'static` 困扰时把"需要共享的上下文"作为被 move 的 owned 入参喂进闭包工厂（`create`/`flat_map` 的闭包），而非塞 `self`。若某处逼出 `Arc<Mutex<mut>>` = 分解错，重想。

## Risks / Trade-offs

- 大改：现 `@if`/嵌套绿核会被替换，中途或回归 → `regress=0` 闸 + 达不到回退。
- 并发合并的**输出顺序**需与 expanded CSS 文档序一致：合并后按稳定 key 排序或以"带序号的流"保序（`flat_map` 顺序 + 不并行乱序），必要时先 `Local` 顺序、`Shared` 仅用于真需要并发的独立子图。

## Migration Plan

保留 `lexer`/`value`/`expr`；新增/替换流式核心于 `compile`；与旧 `parse::Acc` 并行到 parity（同一批 `tests/compile.rs` 用例），绿则删旧核心、切 `compile_string`；重立 `ledger` 基线后按 hrx 目录簇继续 `regress=0` 增长。

## Open Questions

- 保序与并发的边界：默认顺序 `flat_map`，仅在独立 `@use` 子图用 `merge`/多线程。
- `@extend` 的"合并后解析"用一次终端 `scan`/`reduce` 承接，不引外部遍历。
