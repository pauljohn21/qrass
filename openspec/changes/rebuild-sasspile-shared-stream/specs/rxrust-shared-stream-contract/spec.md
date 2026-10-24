# rxrust-shared-stream-contract Specification

## ADDED Requirements

### Requirement: 每一层都递归地"是流"
系统 MUST 在**每一层分解**（不只顶层）都只问「哪条 owned 值流 → 哪个现成 rxrust 算子 → 哪条值流」。一旦某子问题的解出现"先物化结构再遍历"或"给累加子加可变字段/标志去**驱动**流程"，MUST 判为违反本契约（即使 `sasslint` 关键字检查不报），并改回该层用流表达。

#### Scenario: 拒绝帧栈驱动累加子
- **WHEN** 实现用 `scan_map` 携带一个含 `Vec<Frame>` 栈、可变 `chain_taken` 的 `Acc`，靠它在元素间 push/pop 驱动嵌套与分支
- **THEN** 视为"算子外衣下的状态机"，不合规；MUST 改为 `flat_map`/`merge_observables`/`filter` 的流组合

#### Scenario: 子问题也用流
- **WHEN** 处理 env 跨兄弟、`@each` 展开、`@if` 选支、模块共享任一子问题
- **THEN** 每个都用 `flat_map`/`filter`/`Subject`/被 move 的 owned 入参表达，不引入共享可变或手写遍历循环

### Requirement: 块即子流，上下文随流 move
每个块 MUST 由 `block_stream(block, prefix, env_snapshot)` 返回一个 observable；父块用 `flat_map` 把"已组合 selector 前缀 + 该作用域 env 快照（owned 值）"作为参数下传。MUST NOT 用可变栈在父子块间共享/传递结构。

#### Scenario: 嵌套选择器组合下传
- **WHEN** 编译 `a { b { c: d; } }`
- **THEN** 子块 `b` 收到的 prefix 为 `a`、组合出 `a b`，且 env 以值传入，无全局栈

### Requirement: 循环与分支只用高阶算子
`@each/@for` MUST 用 `from_iter(items).flat_map(|item| body_stream(item))`（body 为"以 item 为入参的流工厂"）；MUST NOT 缓冲 body 后重放。`@if/@else if/@else` MUST 用 `filter`/条件返回子流或 `Shared::empty()`；链的"已命中"状态以随兄弟流 move 的**不可变累积值**传递。

#### Scenario: @each 展开
- **WHEN** `@each $i in a, b { .#{$i}{x:1} }`
- **THEN** 由 `items` 流的 `flat_map` 产出两条子流合并，无手写循环

### Requirement: 惰性求值到终点合并
链 MUST 为冷 observable：selector/值以未求值 owned `Expr` 流动，仅在公开入口唯一一次 `subscribe` 的 `map`/`reduce` 中 `eval`→`to_css`→合并为 CSS；MUST NOT 中途急切物化文本或结构。

#### Scenario: 订阅前不产出
- **WHEN** 构造链但未 `subscribe`
- **THEN** 无 CSS 被求值/拼接

### Requirement: 跨线程只共享冻结不可变值
`Shared`/多线程下共享的数据 MUST 是 `Send + Sync + 'static` 的**不可变** owned 值或 `Arc<不可变>`（如已编译模块只读共享）；MUST NOT 用 `Arc<Mutex<业务态>>` 或任何跨线程可变共享。遇 `Send`/`'static` 约束时 MUST 把上下文作为被 move 进闭包工厂的 owned 入参，而非塞进可变 `self`。

#### Scenario: 拒绝共享可变
- **WHEN** 某处需要"多个消费者读写同一状态"
- **THEN** 改为把该状态做成随流 move 的 owned 值或多播的不可变快照；禁止 `Arc<Mutex<_>>` 业务态

### Requirement: 一到多用多播
一份产出喂多消费者 MUST 用 `Shared::subject`/`share`/`publish().ref_count()`/`behavior_subject`+`peek`，而非重复计算或物化后多次遍历。

#### Scenario: 模块单例
- **WHEN** 多个 `@use` 引用同一模块
- **THEN** 编译一次、经多播喂各消费者
