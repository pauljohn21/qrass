# sasspile 框架设计(定稿 · 推式流核心)

> 唯一权威设计。取代早前所有局部/打补丁记录,并**明确废弃其中"物化 AST + 多趟遍历"的 pull 骨架**
> (那是 dart-sass 心智)。功能需求以 `specs/<域>/spec.md` 为准;实现约束由 `reactive-stream-architecture`
> spec + `crates/sasslint` 把关。参照唯一 = `sass-spec/*.hrx` + `element-plus`,**严禁** dart-sass 源码/文档。

## 0. 公理:一切是"推"(push),不是"拉"(pull)
- **dart-sass = 拉**:driver 从一棵物化 AST 里递归 `visit(node)`、按需取子节点、就地改可变状态。控制权在
  "消费者何时去取"。
- **rxrust = 推**:`source` 把 owned 值一个接一个**往前喂**给算子链,值流过 `map/scan_map/flat_map/filter`
  就地被搬运/展开/丢弃,终点收口。控制权在"源何时发",**没有回头遍历**。
- **本设计一切按推式建模**:编译 = 一条把 `Tok` 推到 CSS 文本的流。**MUST NOT 出现"先建 AST 再遍历它"**。

## 1. 主脊:一条推式流
```
chars ──from_iter──▶ Tok
   ──scan_map(LexAcc)·flat_map──▶ 结构事件流 (Open{head}/Decl{prop,value}/Close)   ← 分词内联,不旁落 Vec
   ──scan_map(执行累加子)──▶ 规则事件向前流出:
        累加子 Acc = { scopes: Vec<OpenScope>, env, chain_taken, order }   ← 栈是"数据",住在累加子里
        · Open  → 把"祖先选择器前缀 + env 快照"压入 scopes(纯数据 push)
        · Decl  → 就地绑 $var(env) 或累到当前 scope 的 decls
        · Close → 弹栈并 emit 定稿 StyleRule{selector, decls}(值被"推出",不是被 walk 出来)
        · @each/@for/@while → flat_map:一条 item 事件扇出成 body 子流(推)
        · @if/@else        → filter:按随流 move 的不可变 taken 选支(推)
   ──collect(仅在"必须看到全体"处做一次)──▶ Vec<StyleRule>
   ──纯函数 map──▶ 媒体/supports 冒泡与查询合并      (forward fold over the vec)
   ──纯函数 map──▶ @extend 选择器不动点              (全局需全体,见 §4)
   ──flat_map(from_iter)·map(leaf_block)·reduce_initial──▶ expanded CSS
   ──into_future().await──▶ 单次收口
公开面:仅 async fn compile(...) -> Result<String,SassError>
```
**要点**:嵌套不是"树",是"事件流 + `scopes` 数据栈累加子";Close 处规则**被推出来**。除表达式求值外,
**不构建、也不遍历一棵 AST**。

## 2. 数据模型(owned 值,最小;非 dart-sass 类层级)
- `Tok`(Word/Punct/Var/Interp)· 结构事件 `Open/Decl/Close`。
- `SassExpr`(lit/var/interp/concat/bin/cmp/and/or/if/func)→ `eval(Expr, &Env) -> Value`:这是"求一棵
  表达式",天然自底向上求值、非样式树遍历,允许递归;**禁**的是"对样式/规则树做 visitor"。
- `Value`:Number(带单位换算表)、Calculation(不可化简则原式保留)、Color(CSS4 空间+gamut)、
  List(space|comma|slash, bracketed)、Map(插入序)、String(引号态)、Bool/Null/Argument。
- `Selector`:compound/combinator/伪类/`&`;`@extend` 与 `selector.*` 需要(unify/is-superselector)。
- `Env`/`OpenScope`:块作用域(`!global`/`!default`),`scopes` 栈是 Acc 里的 owned 数据。
- `StyleRule{selector, decls, order}`:L3 向前流出的**值**。
- `Diagnostic{level,id?,message,spans,trace}`。

## 3. 每个 directive = 流上的一个推式算子(独立、非分派状态机)
| directive | 推式表达 |
|---|---|
| `@each`/`@for`/`@while` | `flat_map`(一项 → body 子流,扇出即循环) |
| `@if`/`@else` | `filter` + Acc 里随流 move 的不可变 `taken` |
| 嵌套 | Open 压 `scopes`(数据栈)/ Close 弹栈 emit |
| `@mixin`+`@include`+`@content` | 定义入注册表(一等值);include 把 body 作为子流推出;一到多 = `share`/`ref_count` |
| `@function`/`@return` | 纯 `eval` 内联调用 |
| `@use`/`@forward`/`@import` | 模块 = 子流,编译一次多播喂引用方;独立子图是唯一真并发处 |
| `@media`/`@supports`/`@at-root` | 全体就绪后的一次纯 `map` pass(见 §4) |
| `@extend` | 全体就绪后的一次纯 `map` 不动点(见 §4) |
| `@debug`/`@warn`/`@error` | 流内 diagnostic / `Result` 值 |

## 4. 唯一的例外:全局不动点也必须"推",不是"遍历"
`@extend`(可传递)与媒体冒泡需要**看到全体规则**才能定序/合并 —— 但这是**数据依赖**,不等于 pull。
做法:主脊把规则**推**入一个 owned `Vec<StyleRule>`(`collect`),再做**一次纯函数 forward fold**
`Vec<StyleRule> → Vec<StyleRule>`(extend 不动点 / media 合并),然后 `flat_map` 把结果**重新推入**序列化流。
"物化" = 流中一个 owned 值集合(数据),**不是**先建可变 AST 对象图再 visitor 遍历它。
（若某实现能把 @extend 也做成单遍带累加子,则连这次 collect 都可去掉 —— 优先如此。）

## 5. 并发与保序
- **真并发只在模块加载**(独立子图),`Shared`+`from_stream`/`spawn`;其余**串行推式冷流**(值 owned `Send`)。
- 每产物带**确定性 `order` 键**;任何并发 `merge` 后 `sort_by_key(order)` 稳定重排再序列化(expanded CSS
  逐字节 = 文档序)。`from_stream` 单条顺序 poll 保序;`flat_map(|i| from_stream)` 每条起任务→乱序(实测坑)。
- 跨线程只共享**冻结只读** `Arc<Module>`/`Arc<CompiledCss>`;绝不 `Arc<Mutex<业务态>>`。

## 6. rxrust 即引擎(推;不改、不套壳)
`from_iter/scan_map/reduce_initial/flat_map/filter/Subject/box_it/into_future` 就是控制流本身。
**禁**:自造 `Stage<T>`/`fail`/`drain` 编排、`from_fn`+`VecDeque` 手搓状态机、递归 `stream(node)` AST 解释器、
在异步核心外套 `std::sync::mpsc`+`block_on` 同步门面、为单测 `pub` 泄漏内部件。入口 = 那条流。

## 7. 防 pull/GC 反例索引(命中即按出路改写)
| 反例 | 报警 |
|---|---|
| `parse()` 成 AST 后 `walk(node)`/`visitor`/`visit`/递归下降吐叶 | pull 骨架;`no-recursive-descent`/`no-tree` → 事件流 + `scopes` 累加子,`flat_map` 扇出 |
| `Acc{ Vec<Frame>, chain_taken }` 用 `&mut self` 驱动 | `mutable-accumulator-drives` → owned 累加子值随流 move;@if 用不可变 taken |
| `from_fn`+`VecDeque` 消 for | `no-from-fn-driver`/`no-work-queue` → `from_iter().flat_map()` |
| 攒 body 进 Vec 再 for_each 重放 | `no-imperative-loop` → flat_map;一到多 share |
| 中途 collect 排空再命令式加工 | 语义 → 留惰性;确需全局再 collect + 纯 map fold + `sort_by_key(order)` |
| `type Stage`/`fn fail`/`fn drain`;为单测 pub 内部件 | 框架中转/第二真相 → 直接用算子;行为只经 compile 验 |
| dart-sass 式类名/阶段枚举当骨架 | 语义 review → 由 `sass-spec` 需求倒推最小领域值 |

## 8. 测试与验收(oracle,不变)
`tests/hrx.rs` 经**唯一出口 `compile`** 跑 `sass-spec`(排除 `non_conformant`/`libsass-*`/`:todo:`)产快照给
`ledger`;`regress=0` 按目录簇 scoped、整体单调不降;每阶段六门(无 warning build/test/clippy -D/sasslint 0/
scoped regress/src 纯产码·无脚本);element-plus 作**非回归自基线,绝不用 dart-sass 生 golden**;诊断三类别归一化、
`id`/是否报错硬判。

## 9. 从零实现路线图(依赖序)
| 阶段 | 交付 |
|---|---|
| P0 | harness(多文件/`:todo:`/三类别)+ `async compile` 唯一出口 + 推式主脊骨架(chars→Tok,`into_future`) |
| P1 | Value 代数(单位/calc/斜杠 list/map/颜色基础)+ SassExpr 接入声明值(无歧义触发) |
| P2 | 结构事件流 + `scopes` 数据栈累加子(嵌套/`&`/文档序),**非 AST 树遍历** |
| P3 | 控制流 `@if/@each/@for/@while`(filter/flat_map)+ 作用域 `!global`/`!default` |
| P4 | callable:mixin/@content/function/参数绑定(注册表 + 一到多多播) |
| P5 | 模块 `@use/@forward/@import` + 加载器 + 独立子图并发 + order 键 |
| P6 | Selector 模型 + `@extend` 不动点(§4 的纯 fold;`:is`/超选择器/链式) |
| P7 | 媒体/at-root 冒泡合并 + 原生 CSS 透传 + 诊断格式 |
| P8 | CSS Color 4 全空间 + gamut |

## 10. 与旧记录的关系
本文件以"**推**"为公理,**取代** §2/§3 早前"parse→物化 AST→多趟遍历"的 pull 表述;"单一 `Shared` 绝对流"
的旧记忆亦已订正为**推式流为主、全局不动点仅作数据性 collect+纯 fold**。工作树早先 `structure`(树)
`/eval`(递归)属 pull 形态,实现到 P2 时按推式主脊重构(事件流 + `scopes` 累加子),不作为设计基线保留。
