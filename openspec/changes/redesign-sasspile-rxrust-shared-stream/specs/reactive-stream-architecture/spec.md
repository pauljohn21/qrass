# Spec Delta

## Purpose

约定 `sasspile` 的体系结构属性:它是**流算子 + 全局 pass 的混合式分层管线**。控制流(循环/分支)、
一到多(多播)、模块加载(真并发)用 rxrust 原生算子表达;`@extend`/媒体冒泡这类**天生全局、跨规则**
的关注点,允许在**物化的 owned 结构**上做算子化的多趟 pass。红线是"禁 `Acc` push/pop 驱动、禁递归下降
遍历、禁 `from_fn`+`VecDeque` 手搓状态机、禁在异步核心外套 std `mpsc`+`block_on`",而**不是**
"禁止存在被各相消费的 owned AST"。这些属性由 `crates/sasslint` 确定性把关。

## ADDED Requirements

### Requirement: 入口是一条冷流,收口用 into_future

编译公开入口 MUST 为 `async fn compile(...) -> Result<String, SassError>`;其主体是一条**冷** observable,
终点 MUST 用 rxrust 自带 `into_future().await`(由调用方 runtime 驱动)收口。MUST NOT 在库内保留同步
`block_on`/`std::sync::mpsc`+`recv` 门面包裹响应式核心。中间阶段 MUST NOT 把流反复 drain 成 `Vec`
再命令式二次处理(**有意的全局 pass 除外**,见下条)。

#### Scenario: 无 std 同步壳
- **WHEN** 检视 `compile` 入口与收口
- **THEN** 只有 `async` 入口 + 终点 `into_future().await`;库内无 `OnceLock<Runtime>`/`block_on`/`mpsc` 收口壳

### Requirement: 混合式——流算子 + 物化 owned 结构上的全局 pass

管线的每一相 MUST 以"对哪种 owned 数据、用哪个 rxrust 算子/`fold`/`flat_map`"表达。普通求值(词法/
结构/逐规则求值/渲染)是冷流上的算子;`@extend`、媒体/supports 冒泡这类**跨规则、需回看**的关注点 MUST
在**物化 owned 结构**(AST/规则/模块图)上作为独立 pass 表达。允许"存在被消费的 owned 结构",
MUST NOT 的是"用命令式循环/递归下降/`Acc` 可变标志去**驱动遍历与控制流**"。

#### Scenario: 全局 pass 用算子而非状态机
- **WHEN** 实现 `@extend`(链式 + 选择器伪类并集 + 超选择器)或媒体冒泡合并
- **THEN** 以对 owned 结构的算子/`fold`/`flat_map` pass 表达;源码不出现 `Vec<Frame>` push/pop 驱动、
  可变 `chain_taken`、`from_fn`+`VecDeque`、`fn -> Vec<..>` 命令式中转

#### Scenario: 拒绝帧栈式驱动累加子
- **WHEN** 某实现用携带 `Vec<Frame>` 栈 + 可变 `chain_taken` 经 `scan_map` 驱动嵌套/分支
- **THEN** `sasslint` MUST 判违规(`mutable-accumulator-drives`/`no-tree`/`no-recursive-descent`),
  改回 `flat_map` 扇出 + `filter` 选支 + 独立全局 pass

### Requirement: 循环用 flat_map 扇出、分支用 filter、一到多用多播

`@each`/`@for`/`@while` 与块嵌套 MUST 表达为 `from_iter(..).flat_map(|it| sub_stream(it))`(循环即
`flat_map`);`@if`/`@else` MUST 用 `filter` + 随流 move 的**不可变**链值(非可变标志对象);模块单例、
`@mixin` 定义喂多处 `@include`、一段解析喂多消费者等**一到多** MUST 用 `Shared::subject`/`share`/
`publish().ref_count()`/`behavior_subject`+`peek` 多播,MUST NOT `collect` 进 `Vec` 再 `for_each` 重放。

#### Scenario: 循环即 flat_map
- **WHEN** 展开 `@each $v in list`
- **THEN** 以 `list` 为流、`flat_map` 对每条 `v` 产子 observable 合并,展开顺序 = 文档序

### Requirement: 并发只在模块加载且用 order 键保序

真并发 MUST 仅用于**互不依赖的模块/子图**(P5),经 `Shared`+`from_stream`/`spawn` 并发编译;每个产物
MUST 携带确定性**文档序键**,并发合并后 MUST `sort_by_key(order)`(稳定)再序列化。非并发的普通求值默认
在一条冷流上内联顺序跑(值均 owned `Send`),MUST NOT 为"看似多线程"处处强塞 `Shared`/`Send` 制造假并发。
`Shared::from_stream` 单条顺序 poll 保序;MUST NOT 对每条 item 各起 `from_stream`(并发→乱序)。跨线程只
共享**冻结只读** `Arc<CompiledModule>`,绝不 `Arc<Mutex<业务态>>`。

#### Scenario: 独立模块并发、产物按 order 合并
- **WHEN** 多个独立 `@use` 模块并发编译
- **THEN** 各自在自己作用域产出带 order 键的产物;merge 后按 order 稳定重排,expanded CSS 逐字节 = 文档序

### Requirement: 值/控制建模不变量

声明值与选择器 MUST 从结构化 token 一次折叠成 `Expr` 再惰性求值为 `Value`
(`Vec<Tok> → Expr → eval(owned Env) → Value → to_css`),MUST NOT 拼回字符串再 char 游标机二次扫描。
算术/比较 MUST 仅在无歧义上下文触发(否则误伤 `font: 12px/1.5`、`grid 1/2`、选择器 `a > b`)。
`@if` 链、兄弟选择器前缀、global-env MUST 是**随流 move 的 owned 累加子值**,MUST NOT 是跨元素共享
可变对象/标志。可失败阶段以流内 `Result<T,SassError>`(`Err=Infallible`)传播,终点收口;MUST NOT 用
`panic`/`unwrap` 作控制流,MUST NOT 自造 `Stage<T>`/`fail`/`drain` 框架中转层。`sasslint` 对 `src/` MUST 命中 0。

#### Scenario: 无字符串重扫、无框架层
- **WHEN** 处理插值/`$var`/内置函数
- **THEN** 消费 token/`Expr`,不二次扫字符串、不引入 `Stage/fail/drain` 抽象

#### Scenario: 无歧义才求值
- **WHEN** 值/选择器含 `/ * %` 或 `a > b` 组合子
- **THEN** MUST NOT 当算术/比较求值(保留原样),仅白名单内置上下文才折叠

### Requirement: 唯一对外出口 + 行为只经 compile 验证

除 `compile`/`SassError` 外,`lexer`/`expr`/`value`/`value_parse`/`structure`/`eval`/`builtins` 等 MUST
为 crate 内部(`mod`,非 `pub`);行为只经 `compile` 这一**唯一出口**用 `sass-spec` hrx + `element-plus`
验证。MUST NOT 为"内部件独立可测"而 `pub` 泄漏内部结构或写只测内部件的测试(那是黑盒外的第二套真相)。

#### Scenario: 唯一出口
- **WHEN** 审视 crate 公开面与测试入口
- **THEN** 对外仅 `compile`(及错误类型);所有行为断言经 `compile`,无 `pub` 内部件单测

### Requirement: 起手自检协议(每相动手前必跑)

实现任一相前 MUST 先按 `rust.md`「反回归自查」逐条 grep + 跑 `sasslint`;命中任一 GC 反模式即
**停手改数据分解**(把嵌套/状态/循环/共享重造为流上 owned 值),MUST NOT 继续"加功能"。判据口诀:
循环=`flat_map`;分支=`filter`;状态=被 move 的 owned 累加子;一到多=`Subject`/多播;深度是数据不是
调用栈;收口=终点一次 `into_future().await`。"需要手动栈/缓冲/重放才能表达" ≡ 分解错了。

#### Scenario: 命中即停
- **WHEN** 动手前 grep 命中 `Vec<Frame>`/`chain_taken`/`from_fn`/`while`/`Stage` 等
- **THEN** 停止新增,先把该相重纳为"一条流 + 必要全局 pass",`sasslint` 回 0 后才继续

### Requirement: GC/改造反例索引(本仓库实际踩过的坑,逐条硬禁)

下表每一行都是本迭代真实发生过的错法;任一再现即按对应 `sasslint` 规则拦下并回炉。

`sasspile` 实现 MUST NOT 采用下表任一形态;出现时 MUST 按对应"出路"列改写并经 `sasslint` 归零后才可
合入(不得以"过了测试/涨了分数"给 GC 结构背书)。

| 反例(GC / 改造框架) | 违反不变量 | `sasslint` 规则 | 出路(应用什么) |
|---|---|---|---|
| `Acc{ stack: Vec<Frame>, chain_taken }`+`feed(&mut self)` 驱动嵌套/`@if` | 状态=共享可变对象驱动 | `mutable-accumulator-drives`/`no-tree`/`no-recursive-descent` | 独立块求值 + `@if` 用随流不可变 `taken` |
| `from_fn`+可变 `VecDeque` 工作队列"消掉 for" | 算子外衣下状态机 | `no-from-fn-driver`/`no-work-queue` | `from_iter().flat_map()` |
| 攒 body 进 `Vec` 再 `for_each` 重放(`@each`/`@include`/`@mixin`) | 一到多/循环未用流 | `no-imperative-loop`/`mutable-accumulator-drives` | `flat_map` 扇出;一到多 `share`/`ref_count` |
| 先 `Block::parse` 成树再递归下降 `walk(node)` 吐叶 | 物化-再-命令式遍历 | `no-recursive-descent`/`no-relay-vec` | 求值=对 owned 结构 `flat_map`;仅全局 pass 有意物化 |
| `.collect::<Vec<_>>()` 中途排空再命令式加工 | 流被压回值 | 语义 review | 留惰性算子;确需全局再 collect+`sort_by_key(order)` |
| `flat_map`+全局可变 env | env 非 owned 随流 | `mutable-accumulator-drives` | env 作 owned 入参 move 进子流 |
| 同步门面 `OnceLock<Runtime>::block_on`+`mpsc::recv` | 异步核心外套 std 壳 | 入口 review | 入口 `async`,终点 `into_future().await` |
| 处处强塞 `Shared`/`Send` 冒充多线程 | 假并发 | 语义 review | 默认串行冷流;仅独立模块子图真并发+order 键 |
| `type Stage<T>`/`fn fail`/`fn drain` 自造编排层 | 框架中转 | `no-relay-vec`/语义 | 直接用 rxrust 组合子 |
| 为单测 `pub` 内部件(`token_stream`/`call_builtin`/`Expr`) | 第二套真相/黑盒外泄 | 唯一出口 review | 行为只经 `compile` 验 |

#### Scenario: 反例即拦
- **WHEN** 实现中出现上表任一形态
- **THEN** `sasslint`/review 判违规,按"出路"列改写;不得以"过了测试/涨了分数"给 GC 结构背书
