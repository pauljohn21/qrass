# Spec Delta

## Purpose

约定 `sasspile` 编译流程作为**一条 `Shared` 冷 observable owned 值流**的体系结构属性：每一层都是流、
控制流交给 rxrust 算子、状态是被 move 的 owned 累加子、一到多交给多播、终点单次 `subscribe` 合并；
这些属性由 `crates/sasslint` 确定性把关。

## ADDED Requirements

### Requirement: 整体是一条惰性 `Shared` 冷 observable

`sasspile` 的编译 MUST 表达为一条 `Shared` observable 管线：`Shared` 要求 item/错误/闭包均为
`Send + Sync + 'static`；管线 MUST 为**冷**流（终点订阅才驱动）；CSS MUST 只在**唯一一次** 终点
`subscribe` 处合并产出。公开入口 MUST NOT 在流之外物化整树再命令式遍历求值。

#### Scenario: 单次终点订阅
- **WHEN** 检视 `compile` 公开入口
- **THEN** 整条 `Shared` 链只在末尾出现一次 `subscribe`，中间不提前收口成 `Vec`/字符串再二次处理

#### Scenario: Send/'static 值模型
- **WHEN** token、`Value`、`Expr`、块节点作为流上 owned 值跨算子与线程传递
- **THEN** 它们均满足 `Clone + Send + Sync + 'static`，无需为编译通过而退回引用传递或 `self` 借用

### Requirement: 每一层都递归地"是流"

从词法、结构、求值到渲染，**每一层**子问题 MUST 以"哪条 owned 值流 → 经哪个现成算子 → 变成哪条
值流"表达；MUST NOT 在任一层出现"先物化结构再用 `for`/`while`/`loop`/递归下降遍历"或"给累加子加
可变字段/标志去驱动流程"。

#### Scenario: 拒绝帧栈式驱动累加子
- **WHEN** 某实现试图用携带 `Vec<Frame>` 栈与可变 `chain_taken` 的累加子经 `scan_map` 驱动嵌套/分支
- **THEN** 该写法 MUST 被 `sasslint` 判违规（`mutable-accumulator-drives`/`no-tree`/
  `no-recursive-descent`），改回 `flat_map` 扇出子流 + `filter` 选支

#### Scenario: 拒绝算子外衣下的手搓状态机
- **WHEN** 某实现用 `std::iter::from_fn` + 可变 `VecDeque` 工作队列"消掉 `for/while`"来推进
- **THEN** MUST 被 `sasslint` 判违规（`no-from-fn-driver`/`no-work-queue`），改由算子表达

### Requirement: 嵌套与循环用 `flat_map` 扇出、合并去递归

块嵌套、`@each`/`@for`/`@while` 的循环 MUST 表达为流中流：`from_iter(items).flat_map(|it|
body_stream(it))`；并发分支/兄弟子流 MUST 用 `merge_observables`/`merge_all` 扇出后合并，把调用栈
递归替换为"流的组合"。MUST NOT 缓冲 body 再命令式重放。

#### Scenario: 循环即 flat_map
- **WHEN** 展开 `@each $v in list`
- **THEN** 以 `list` 为流、`flat_map` 对每条 `v` 产一条子 observable 合并，展开顺序与文档序一致

### Requirement: 状态用被 move 的 owned 累加子

跨元素的求值状态（env 变量表、选择器前缀、文档序计数器）MUST 作为随流 move 的 owned 值/被算子
携带的累加子传递（`scan_map` 的 `&mut acc` 或 `reduce_initial` 的 move acc），MUST NOT 用
`Arc<Mutex<业务态>>` 或 `&mut self` 对象承载跨元素共享可变业务数据。

#### Scenario: 变量表随流下传
- **WHEN** 子块求值需要父作用域的变量绑定
- **THEN** 该绑定作为被 move 的 owned env 快照值进入子流，而非放进共享可变结构

### Requirement: 一到多交给多播

一份产出喂多个消费者（模块 `@use` 单例、一段解析同时喂选择器与值）MUST 用 `Shared::subject`/
`share`/`publish().ref_count()`/`behavior_subject`+`peek` 多播；跨线程共享的编译产物 MUST 为
**冻结只读** `Arc<T>`，MUST NOT 为 `Arc<Mutex<_>>` 业务态。

#### Scenario: 模块单例多播
- **WHEN** 同一路径模块被多个文件 `@use`
- **THEN** 该模块只编译一次，结果经多播/`Arc<只读>` 喂给各引用方

### Requirement: 值从结构化 token 一次成型

声明值与选择器 MUST 从结构化 token 一次折叠成 `Expr` 再惰性求值为 `Value`（`Vec<Tok> → Expr →
eval(owned Env) → Value → to_css`），MUST NOT 把 token 拼回字符串再用 char 游标机 `subst`/`stmt_text`
二次扫描。

#### Scenario: 无字符串重扫
- **WHEN** 处理 `#{...}` 插值与 `$var` 替换
- **THEN** 求值消费 token/`Expr`，源码不含对已拼字符串的二次 char 扫描

### Requirement: 错误以流内 Result 传播

可失败阶段 MUST 以流内 `Result<T, SassError>` 传播错误、错误通道类型用 `Infallible`，在终点收口；
MUST NOT 用 `panic`/`unwrap` 作为控制流，MUST NOT 自造 `Stage<T>`/`fail`/`drain` 之类把流压回值的
框架中转层。

#### Scenario: 错误随流到终点
- **WHEN** 某阶段产生 `SassError`
- **THEN** 它作为流内 `Err` 值传播到唯一终点订阅处收口，不在中途 panic
