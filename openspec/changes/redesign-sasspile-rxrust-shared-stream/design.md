# Design

## Context

现绿核（`944b13e`/`1a7717a`）：`compile_string` 是一条 `Local` 算子链，但结构段 `parse::Acc` 用
`scan_map` 携带可变 `Acc{ Vec<Frame> 帧栈, env, chain_taken }` push/pop 驱动嵌套与 `@if` 选支——被
`sasslint::mutable-accumulator-drives` 判违规。已具备可复用件：`lexer`（结构化 token 流）、`value`
（`Value`/`to_css`/`parse_number`/`is_falsey`）、`expr`（`Expr` 折叠 + 惰性 `eval`，含算术/内置函数
白名单/惰性 `if`）。动机见 `proposal.md - Why`；能力与验收见三份 spec。

关键事实（来自 `docs/rxrust-source-analysis/` 对 rc.6 源码的精读）：
- 非时间算子在 `subscribe` 内**同步内联跑完**；`Shared` 只换 `Send+'static` 约束、不改执行模型。
- `flat_map = map(f).merge_all(concurrent)`：`concurrent=usize::MAX` 并发、`1` 顺序保序（`concat_all`）。
- 多播：`Shared::subject`/`publish().ref_count()`/`behavior_subject`+`peek`；`merge_observables`/
  `merge_all` 扇出合并；`box_it` 类型擦除且**保留 scheduler**。
- 收口 MUST NOT 自造 `drain`/`Stage`：用 `collect::<Vec<_>>()` 末段 + `.into_future()` 取终值，
  或唯一一次 `subscribe` + `mpsc`（`mpsc` 是 rxrust 之外唯一允许的跨线程收口，非"框架中转层"）。

## Goals / Non-Goals

**Goals:**
- 核心 = 一条 `Shared` 冷 observable；嵌套/循环/分支/共享全部只用 rxrust 算子表达；`sasslint` 0 命中。
- 并行扇出下 expanded CSS 仍与 `sass-spec` **逐字节文档序一致**（确定性重组）。
- 分能力簇以 `regress=0` 增量长（先复用件后接线）。

**Non-Goals:**
- 不为"能并发"引入 `Arc<Mutex<业务态>>`；不引入外部遍历/物化-再-walk。
- 不在本变更内追求 `non_conformant`/`libsass-*` 通过；不以 dart-sass 行为为架构蓝本。

## Decisions

### D1 — 分层流管线（每层都是 `Shared` observable）
`token_stream(source)`（`Shared`，结构化 `Tok`）→ `structure`（把 token 折成 owned `Block`/`Stmt`
**数据**，见 D2）→ `emitt`（`fn stmt_stream(stmt, ctx) -> SharedBoxedObservable<Leaf>`，用 `flat_map`
下钻、`merge_observables` 合兄弟/分支）→ 末段按 D3 定序 → 唯一终点 `subscribe`。
理由：把"驱动"交还给算子，`sasslint` 无 `Vec<Frame>`/`chain_taken`/`from_fn` 可报。
替代：保留帧栈 `Acc`（否决，正是违规源）；`Local` 单线程（用户选 `Shared`，见 D5）。

### D2 — 解析产出 owned 数据，但"遍历"交给 `flat_map` 而非递归下降
需要一个 owned 的块结构（`Block { head_tokens, decls, children }`）承载嵌套——这是**数据**，不是
被 `for`/递归消费的"控制流引擎"。展开时 `children` 经 `from_iter(children).flat_map(child_stream)`
扇出，父选择器前缀与 env 快照作为 move 入参进入子流。判据：结构存在可以，**命令式 walk 该结构不行**；
`stmt_stream` 返回 observable、由 `flat_map` 递归组合，`BoxedObservable` 承接 `Send+'static` 递归签名。
替代：完全流式无中间块（token 边读边吐）——`@use`/`@extend`/媒体冒泡需回看，物化 owned 数据更稳且
不违反"不物化再遍历"（遍历用算子）。

### D3 — 并行扇出 + 文档序确定重组（Shared 路线的核心难点）
`Shared` 多线程 `merge_all(usize::MAX)` 会乱序。方案：每条 `Leaf` 携带 owned 文档序键
`order: (usize, usize, ...)`（源位置/祖先路径序号元组，随流 move 不共享）；末段 `collect` 后按 `order`
**稳定排序**再拼接。顺序敏感的簇（如变量文档序、`@extend` 合并）默认用 `concat_map`/`merge_all(1)`
保证顺序，仅**独立子图**（互不影响的兄弟块、不同 `@use` 模块）用并发。
理由：既满足"多线程扇出合并"，又保 `sass-spec` 逐字节一致。
替代：全并发再靠全局原子序号——引入共享可变（否决）。

### D4 — `@if`/`@else` 用 `filter` + 不可变链标志随流 move
分支选择用 `filter`/条件产出子流或 `Shared::empty()`；`@else` 链"是否已被命中"由**随兄弟流 move 的
不可变累积值**（`scan`/fold 产出）传递，不是 `Acc` 上的可变 `chain_taken`。理由：消除可变标志驱动。

### D5 — 执行模型：`Shared` + tokio multi-thread（用户决策）
公开 `compile` 在 tokio multi-thread runtime 上 `block_on` 一条 `Shared` 链；item/错误/闭包
`Send+Sync+'static`。跨线程只共享冻结 `Arc<CompiledModule>` 只读产物，绝不 `Arc<Mutex<业务态>>`。
收益：独立模块/子图真并发；`Shared::subject` 多播 `@use` 单例。
代价与对策：`Send+'static` 逼值模型全 owned+`Clone`（可接受，本就是 owned 数据）；保序见 D3。

### D6 — 一到多 = `Shared::subject`/`publish().ref_count()`
模块单例编译一次喂 N 引用方：`Shared::subject`/`share`；`behavior_subject`+`peek` 供"最新值即取"。
理由：落实 spec「一到多交给多播」，杜绝 `collect` 缓存再重放。

### D7 — 值一次成型 + 惰性终点求值
`Vec<Tok> → Expr（fold 一次）→ eval(owned Env) → Value → to_css`；`Expr`/selector 以未求值 owned
值随流流动，`Value`/CSS 仅在末段求值。算术走单位规则、`/ * %` 跳过护 CSS；内置函数走白名单
`call_builtin`，未知名 `Err → 上层原样`（`url/calc/...` 不当 Sass 内置）。比较只入条件上下文，
`a > b` 组合子不当比较。理由：延续现 `expr`/`value` 已验证的安全语义。

### D8 — 收口不造框架层
终点唯一 `subscribe` 把 `Result<String, SassError>` 经 `mpsc` 交给 `block_on` 外层；或末段
`collect().into_future()`。`Err` 通道 `Infallible`，错误随流内 `Result` 传播。MUST NOT
`Stage<T>`/`fail`/`drain` 中转（违反 `sasslint` 与 spec）。

## Risks / Trade-offs

- 核心重写期回归 → 与旧绿核并行、跑同一批 `tests/compile.rs` 到 parity 再切 `compile_string`、删
  `parse::Acc`；每步 `ledger diff regress=0`，不达标回退。
- 并发保序错 → D3 以 order 键 + 顺序敏感簇 `concat_map`；用 hrx 全量回归兜底。
- `Shared+'static` 递归签名膨胀 → 全程 `box_it`/`SharedBoxedObservable`；遇 `'static` 困扰把共享上下文
  作为 move 入参喂闭包工厂，不塞 `self`、不 clone 凑数。
- tokio runtime 引入与 wasm 目标冲突 → 本变更锁定 native 多线程；wasm 产出留待后续单独 change
  （`Shared::into_local` 路径），不在此实现。

## Migration Plan

1. 先补 `expr`/`value` 独立件（算术/内置/比较/惰性 if），金样单测、不接主链。
2. 新建 `Shared` 流核心模块（`structure`+`stmt_stream`+D3 定序+D8 收口），与旧 `parse::Acc` 并行。
3. 同一批 `compile.rs` + 目标簇 hrx 在两条链对拉到 parity → 切 `compile_string` 到新核心、删旧核。
4. `sasslint` 命中归 0、`ledger` 重立基线；此后按 hrx 目录簇以 `regress=0` 逐个增量长。
回退策略：任一步 `regress>0` 或 `sasslint` 命中，`git checkout` 到绿色基线，不产坏提交。

## Open Questions

- element-plus 端到端的参照产物如何取得（是否已有一份基准 CSS 供 `ledger` 比较）？——影响 conformance
  task 的基线来源，但不改本设计的架构与 spec，留待实现该簇时确认。
- 独立子图并发的粒度阈值（哪些兄弟块/模块值得并发、哪些直接 `concat_map` 更省）——性能调优项，
  默认先顺序、验证正确后再局部并发，可后置。
