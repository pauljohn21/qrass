# sasspile 框架设计(定稿 v2 · rxrust 原生分解)

> 唯一权威设计。输入**只有**两个行为需求源:本地 `sass-spec` 语料与 `element-plus` 真实主题(下述测量值
> 均 2026-10-25 实测);**严禁**以 dart-sass 源码/文档推断行为,也**不以现有实现代码为设计输入**。
> v1 的三处口径被源码级事实推翻,已在 §1/§5/§6 改正:阶段**不得**用"`scan_map` 发 `Vec` 再 `flat_map(from_iter)`"
> 的批量外壳;"并发 merge 后 `sort_by_key`"不再是保序手段;错误**不得**只走"in-stream `Result` + 失败兜底成原文"。
> 属性由 `specs/reactive-stream-architecture/spec.md` + `crates/sasslint` 把关。

## 0. 语料即需求(实测,决定排序与优先级)

| 面 | 实测 | 设计后果 |
|---|---|---|
| 规范总量 | `spec/` 3,039 个 `.hrx`;**14,345** 条 `input.scss/sass`、**11,808** 条 `output.css`;`**/errors/**` 80 个文件是错误期望;`:todo:` 8 | 分阶段基线按目录簇 scoped(§8);`non_conformant`(941)/`libsass-*`/`:todo:` 不入目标 |
| 用例分布 | `core_functions` **9,071**、`values` 1,244、`css` 968、`directives` 904、`libsass-closed-issues` 593、`expressions` 250、`callable` 101、`operators` 37、`variables` 20 | **值与内置函数是主体工作量(63%)**;分解必须让"函数按域注册"成为一等结构 |
| 构造频次 | 标记出现:`@use` 10,414、`@import` 1,481、`@include` 1,453、`@media` 878、`@function` 830、`@extend` 725、`@for` 681、`@mixin` 679、`@forward` 557、`!default` 527、`@supports` 390、`!global` 388、`@at-root` 194、`@each` 95、`@while` 25;值域:`calc(` 2,165、custom property 1,942、`var(--` 720 | 模块系统与可调用是**结构主干**,不是尾部特性 |
| element-plus(theme-chalk) | 146 个 `.scss` / 18,506 行;实到 `@include` 1,520、`@use` 544、`map.get` 393、`!default` 195、`map.merge` 171、custom property 121、`@each` 82、`calc(` 68、`@mixin` 68、`math.div` 15、`@at-root` 12、`!global` 12、`@extend` 5、`@forward` 13、`@content` 13;**`@import` 0** | 端到端跑通 EP 的硬前置 = **map/list/math 函数 + `@use as *`/`@forward` 链 + mixin `@content` + `!global`/`!default` + 选择器插值(BEM)**;`@extend`/`@while`/`@supports` 可后置 |

**结论(排序由语料决定,不由实现便利决定)**:P1 值/单位/calc → P2 选择器模型 → P3 控制流+作用域+at-root →
P4 可调用 → P5 模块(真并发唯一处) → P6 `@extend` → P7 CSS 原生透传 → P8 全色域;诊断(真错误通道)贯穿,
在 P0 就把错误期望计入 harness。

## 1. 主脊:一条冷流 + 每相一个"算子内核"

**框架原生分解口径(v2 核心变更)**:每个编译相都是一个**真算子**,即 rc.6 自己的三件套形状:

```text
struct Phase<S, Params> { source: S, params }          // 惰性数据:只有源与参数,无每订阅状态
struct PhaseObserver<O, State> { observer, state }     // 状态在这里(随 subscribe 诞生/死亡)
impl ObservableType   // Item<'a>(GAT) + Err = SassError
impl CoreObservable<C: Context>  // subscribe(self, ctx): ctx.transform(→PhaseObserver) → source.subscribe(wrapped)
```

- 内核在自己的 `next` 里**逐条 `observer.next(..)`** 发射 0..N 项:一到多(词法一字符出多 token、结构一 token
  出多事件)由内核发射完成,**不再** `scan_map(acc) → Vec<Tok> → flat_map(from_iter)`。
  该外壳是"给流造的批量中转",框架里没有对应物(v1 口径作废)。
- 需要"看全体"的相(`@extend` 不动点、媒体冒泡)**仍**用框架自带的 `collect` 物化成 owned `Vec`,再做一次
  **纯函数** forward fold,然后 `flat_map` 回流:这是唯一的物化点,禁"物化后再递归下降/visitor"。
- 取消与终止:上游句柄一律 `SingleAssignment<C::RcMut<State<U>>>::channel()` **先建槽后订阅**;
  "终止只投一次"用 `Option::take()` / `C::RcCell<..>::compare_exchange`,禁裸布尔标志;
  上游自然终止时**不**取消它(只 retire)。

```
chars ─▶ Lexer<S>            (char → Tok,逐条发射;interp/注释状态在 Observer 内)
      ─▶ Structurize<S>      (Tok → Event::{Open,Decl,Close},逐条发射)
      ─▶ Exec<S>             (Event → StyleRule:scopes 栈 + Env 住在 Observer 里,由 `&mut self` 推进;
                              发射时机 = 开子块冲刷父待定段 / 闭块冲刷残余 —— 见 scss-compilation 规范)
      ─▶ [全局 pass] collect → 纯 fold(媒体/supports 冒泡合并;@extend 不动点)→ flat_map 回流
      ─▶ Render<S>           (StyleRule → CSS 文本叶,逐条发射;此处串行,流序即文档序)
      ─▶ reduce_initial(..)  ─▶ 终点 into_future()  ─▶ Result<String, SassError>
公开面:仅 async fn compile(content) -> Result<String, SassError>
```

**现状(P0′ 已收尾)**:上述四个内核已在 `crates/sasspile/src/phases.rs`,`compile` 链就是上面 6 个环;
`order` 键/`sort_by_key`/`from_stream` 假并发已不存在;真 `Err` 通道已接上且 `UndefinedVariable` 已致命;
选择器/属性名是结构化值(`src/selector.rs`),`&` 组合不再走 `str::replace`;四条铁律已编成 `sasslint`
机器闸并打开 CI 门。harness 基线 595 → **600**(量具已换成全路径 key + 错误消息金样,见 tasks 0′.3)。
**0′.3 只真值 +1** 这件事本身是结论:剩下大量 `error` 用例要的是尚未实现的**消息**(单位代数、
`Expected identifier.`、deprecation),归 P1/P7。P0′ 仅剩 0′.5(共享态 `C::RcMut`/`C::RcCell`)——它当前
无可改之物(四个内核状态全为观察者独有),真正触发点是 P4(`@content` 闭包流)与 P5(模块多播)。

嵌套不是"树",是事件流上的 owned `scopes`;规则在**开子块冲刷父块待定声明段**与**闭块冲刷残余**两个时刻被推出来
(所以一个块可以出多条规则),而不是"等 `Close` 一次性出整块"。除表达式求值(自底向上,天然递归)
外,不构建也不遍历任何 AST。

## 2. 数据模型(owned 值,最小领域值)

- `Tok`(Word/Space/Op/Punct/Var/Interp/Silent)· `Event::{Open, Decl, Close}` —— **都是结构化值**。
- **禁"拼串承载语义"**:`Selector` 是结构化值(compound/combinator/伪类/`&`/插值位),不是 `String`;
  属性名是 `PropName`,不是渲染后的文本;`&` 组合是选择器代数操作,不是 `str::replace`;
  声明值走 `Vec<Tok> → Expr → eval(owned Env) → Value → to_css`,不存在"渲染成串再 char 游标重扫"。
  (v1 代码里 `render→canon→split_whitespace` 与 `child.replace('&', p)` 属违规形态,重构须消除。)
- `Expr`(lit/var/interp/bin/cmp/and/or/if/func-call)· `Value`(Number+单位代数、Calculation(不可化简则**原式
  保留**)、Color(CSS4)、List(space|comma|slash, bracketed)、Map(插入序)、String(引号态)、Bool/Null/Argument/Module)。
- 可调用与模块是一等值:`Mixin`/`SassFunction`/`Module` 入注册表;`@content` 是把 body 作为**子流**传进去的值。
- `Diagnostic{level,id?,message,spans,trace}` —— 诊断格式精确(双源 trace:invocation + declaration)。

## 3. directive = 流上的推式分解(不是分派状态机)

| directive | 推式表达 | 语料量级 |
|---|---|---|
| `@each`/`@for`/`@while` | 内核内 `flat_map`(一项 → body 子流,扇出即循环);`@while` 用生成式子流而非循环变量 | 95 / 681 / 25 |
| `@if`/`@else` | `filter` + 随流 move 的**不可变**分支值(禁可变 `taken` 标志对象) | 367/163 |
| 嵌套 | `Exec` 的 Observer 里 owned `scopes` 数据栈;`Close` 弹栈发射规则 | 全域 |
| `@mixin`/`@include`/`@content` | 定义入注册表;include 把 body 作为子流推出;**一处定义多处消费 = 多播**(`Subject`/`publish().ref_count()`;rc.6 **无 `share()`**) | 679/1,453/152 |
| `@function`/`@return` | 纯 `eval` 内联调用;`meta.get-function/call` 取一等函数值 | 830/522 |
| `@use`/`@forward`/`with`+`!default` | 模块 = 子流,**编译一次多播喂引用方**;`with` 配置在实例化前注入;`!default` 只在未定义时生效 | 10,414/557/527 |
| legacy `@import` | 弃用路径,单独 pass;CSS `@import` 走原生上提排序 | 1,481 |
| `@media`/`@supports` | 全体就绪后一次纯 fold(冒泡 + 逐查询合并) | 878/390 |
| `@at-root`(`with/without`) | `Exec` 内按 `scopes` 数据定作用域根,发射时决定前缀 | 194 |
| `@extend` | 全局不动点(纯 fold over `Vec<StyleRule>`)+ `!optional` 报错走 Err 通道 | 725/46 |
| `@debug`/`@warn`/`@error` | 流内 `Diagnostic` 事件 / Err 通道 | 57/40/37 |

## 4. 唯一允许的"看全体"pass

`@extend`(可传递)与媒体冒泡需要全体规则 —— 这是**数据依赖**,不是 pull。做法:主脊把规则 `collect` 成 owned
`Vec<StyleRule>`,做**一次纯函数** `Vec → Vec`,再 `flat_map` 推回序列化流。"物化" = 流中一个 owned 值集合,
**不是**可变对象图 + visitor。若某相能做成单遍带累加子,则连这次 `collect` 都该省掉(优先如此)。

## 5. 并发与保序(v2 改正:保序靠动词,不靠 sort)

- **真并发只在互不依赖的模块/子图**(P5),`Shared` + `from_stream`/`spawn`。
- **顺序 intrinsic 处一律用框架动词**:串行段天然保序;需要"逐项展开且保文档序"用
  `concat_map` / `concat_all()` / `concat_observables()`(实测 = `merge_all(1)`,保序且不丢值)。
- **`sort_by_key(order)` 不再是并发段的手段**:rc.6 `merge_all` 在并发额度满时会**丢弃仍在排队的内层**期间
  的发射(`ops/merge_all.rs` 测试 `test_merge_all_concurrency`),这是**缺元素**不是乱序,排序无法恢复。
  因此并发分支必须满足其一:① 内层"被订阅后才开始生产"(`defer`/`from_stream`/内核 `subscribe` 时才动);
  ② 直接串行(`concurrent:1`)。`order` 键只保留给**已 collect 的全局 pass 内部**的确定性重排。
- `flat_map(|i| from_stream(..))`(每条 item 各起任务)实测乱序,禁用。
- 跨线程只共享**冻结只读** `Arc<CompiledModule>`;共享可变一律取 `C::RcMut`/`C::RcCell`(由 Context 决定
  `Rc<RefCell>`/`Arc<Mutex>`),业务态永不 `Arc<Mutex<_>>`。
- 多播不可重入:同一 `Subject` 的回调里再 `next/error/complete` 会 panic;需反馈环就 `delay(0)` 造异步边界。

## 6. 错误通道(v2 改正:真 Err 通道,禁兜底)

- 链的 `Err = SassError`,失败经 `Observer::error(self, SassError)` 一路到底,终点 `into_future()` 给出
  `Ok(Err(SassError))`;`compile` 直接映射成 `Err(SassError)`。
- **废止** v1 的"`Err = Infallible` + 流内 `Result` + 失败 `unwrap_or_else(verbatim)`":把求值失败兜底成原文
  **不是错误策略**,它会静默吞掉真错误并掩盖 hrx 的 error 期望。
- 保留的例外:仅"局部可恢复且语义上确需原样透传"的 CSS 原生构造(calc 不可化简、custom property 原样文本、
  未识别 at-rule),这类**由规范定义为透传**,而非由 `Result` 失败降级。
- 落地形态:`const FATAL: &[ErrorKind]` 是唯一的“哪类失败真终止”开关,当前 = `[UndefinedVariable]`。透传例外
  逐个由语料定形状,且只**抑制致命性**、不改求值结果:① 块头部只认 `#{}`(`@for $i` 是绑定目标);
  ② special 调用的参数(`expr::special_call`);③ plain-CSS at-rule 体(`exec::plain_css_body`,
  `@function --a()` 里 `result: $b` 留原文而同体 `#{1 + 1}` 仍出 `2`)。`@mixin --a` 不属透传(语料判它报错)。
- 收口只用框架原生口子:`collect`/`reduce_initial` + `into_future()`、`into_stream()`、或 await `TaskHandle`;
  `std::sync::mpsc` 仅当必须把 `Sender` 交给外部/跨线程时作桥接,禁当终点、禁外套 `block_on`。
- `into_future()` 实测限制:要求**恰好一个值**(空→`Empty`,多→`MultipleValues`)且**丢弃订阅句柄**(不可取消);
  故终点必须先 `reduce_initial`/`collect` 收成单项。

## 7. 入口与出口

- 唯一公开面:`async fn compile(content: impl Into<String>) -> Result<String, SassError>`(+ `SassError`);
  其余(`lexer`/`expr`/`value`/内核)全为 crate 内部,**不为单测 `pub` 泄漏**(行为只经 `compile` 验)。
- 上下文:`Local` 为默认(wasm32/`!Send` 友好);`Shared` 只在 §5 的真并发段。内核靠 `C: Context` 泛型化,
  同一份内核代码两态皆可编译(**不**在 kernel 里写死 `Rc`/`Arc`)。
- 每相用 `tracing::info_span!`(字段 `stage`/`module`/`id`/`elapsed_ms`)在**实际执行的闭包内**记录;禁 `println!`。

## 8. 阶段与验收(oracle 不变)

- P0–P8 顺序由 §0 语料决定;`tasks.md` 逐簇列可验证小步。
- 每阶段:`tests/hrx.rs` 经**唯一出口 `compile`** 跑 `sass-spec`(排除 `non_conformant`/`libsass-*`/`:todo:`)
  产快照给 `ledger`;`regress = 0` **按目录簇 scoped** 判定,已过簇不得回退,整体单调不降。
- 六门:无 warning build / `cargo test` / clippy `-D warnings` / `sasslint count=0` / hrx `regress=0` /
  element-plus 编译不劣化(**EP 只作非回归自基线,绝不用 dart-sass 生 golden**)。
- 重构起点(P0'):把 §1 的"批量外壳"改成内核、把 §5/§6 两条口径落进 `sasspile` 与 `sasslint` 规则,
  再按簇继续 P1 起的语言面。
