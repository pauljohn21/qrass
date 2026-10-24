# rxrust-lazy-stream-contract Specification

## ADDED Requirements

### Requirement: 自底向上的流式分解
sasspile 的任一阶段 MUST 从"**哪条 owned 值流、经过哪个算子、变成哪条值流**"起手设计；MUST NOT 先物化中间结构（AST/树/`Vec<Node>`/`Vec<Rule>`）再"想办法"用算子遍历它。若设计里出现"先建结构、后处理结构"，即判为分解错误，MUST 重造数据而非改造 rxrust。

#### Scenario: 禁止先建 AST 再遍历
- **WHEN** 一个实现把输入 parse 成一棵树，然后写函数递归/循环遍历该树来产出结果
- **THEN** 该设计不符合契约，MUST 改为让值/块作为流上的 owned 数据、由算子就地消费

#### Scenario: 判据可 grep
- **WHEN** `sasslint` 扫描源码命中 `Vec<Rule>`、`.children` 递归遍历、`fn -> Vec<..>` 中转
- **THEN** 判定为"物化结构再遍历"违规并阻断合入

### Requirement: 惰性求值，终点订阅才合并输出
编译 MUST 是一条**冷 observable 描述**；值与选择器以**待求值的 owned 值（`Expr`/闭包）**在流上传递，MUST NOT 在流水线中途急切求值并拼接字符串。CSS 文本 MUST 只在公开入口**唯一一次 `subscribe`** 触发、把整条描述合并时才生成。

#### Scenario: 订阅前不产出
- **WHEN** 构造了编译 observable 但尚未 `subscribe`
- **THEN** 无任何 CSS 被求值或拼接，`Value`/文本未被物化

#### Scenario: 唯一收口
- **WHEN** 审查 `compile_string`
- **THEN** 恰有一次 `subscribe`（喂 `mpsc`）作为终点，其上全部为算子组合

### Requirement: 消费自身（线性所有权）
流经算子的每个值 MUST 被 move、被下游**消费至多一次**；MUST NOT 把业务数据放进 `Arc<Mutex<_>>` 或跨元素共享可变对象来"供多方随时读"。错误 MUST 以流内 `Result<T, SassError>`（`Err = Infallible`）下传、终点收口。

#### Scenario: 值下传不 clone 回堆
- **WHEN** 某值需要进入下一算子
- **THEN** 它被 move 进算子，而非克隆进一个被多方持有的堆结构

#### Scenario: 错误随流下传
- **WHEN** 某声明求值失败（如插值引用未定义变量）
- **THEN** 以 `Result::Err` 在流内传递、由后续算子丢弃该声明，不 panic、不中断整条流

### Requirement: 循环交给 flat_map，一到多交给多播
`@each`/`@for` 的展开 MUST 用 `flat_map`（对条目流按项 fan-out 子流），MUST NOT 缓冲 body 后手写 `for`/`while`/`for_each` 重放。确需"一份产出喂多个消费者"（模块单例被多处引用、一段解析同时喂选择器与值渲染）MUST 用 rxrust 多播（`share`/`publish().refCount()`/`Subject`/`behavior_subject`+`peek`）。多播 MUST NOT 被当作"共享可变状态"的挡箭牌——线性一次过的状态仍用 `scan_map`/`reduce_initial` 的 owned 累加子。

#### Scenario: 循环用 flat_map
- **WHEN** 处理 `@each $v in a, b, c { .#{$v}{…} }`
- **THEN** 以 `from_iter(items).flat_map(|it| body_substream(it))` 展开，源码中无手写循环

#### Scenario: 模块单例多播
- **WHEN** 一个被 `@use` 的模块需被多个引用方消费
- **THEN** 用多播让一次编译结果喂多个下游，而非重复遍历或 `Arc<Mutex>` 共享

### Requirement: 状态为算子携带的 owned 累加子
可变状态（帧栈、`env`、`@if` 组态、深度）MUST 是 `scan_map` 的 `&mut acc` 或 `reduce_initial` 的 move 累加子里的 owned 数据；MUST NOT 用 `&mut self` 方法对象、`from_fn`+可变 `VecDeque`/`Vec<Frame>` 等"手搓状态机"驱动处理。嵌套深度 MUST 是数据、不是调用栈。

#### Scenario: 无算子外衣的状态机
- **WHEN** 实现用 `std::iter::from_fn` 驱动一个内部 `step()`+可变 work-queue 来"消掉 while"
- **THEN** `sasslint` 以 `no-from-fn-driver`/`no-work-queue` 判为违规（它是换了外衣的命令式状态机）

### Requirement: 行为 oracle 与架构参照分离
`sass-spec/*.hrx` 金样与 `element-plus` 语料是行为的**唯一**权威；MUST NOT 以 dart-sass 的源码/文档推断**行为**，更 MUST NOT 以其"物化 AST→遍历→GC"的心智作为**架构**参照。

#### Scenario: 不抄 dart-sass 架构
- **WHEN** 设计某阶段的分解方式
- **THEN** 依据本契约的所有权/惰性/多播原则决定，而非模仿 dart-sass 的 AST/visitor 结构
