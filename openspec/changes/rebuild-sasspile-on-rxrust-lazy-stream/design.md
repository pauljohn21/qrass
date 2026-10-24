# Design: rxrust 惰性流内核

## Context

sasspile 需要把 SCSS（嵌套、变量、`@use`/`@mixin`、`@if`/`@each`/`@for`、值/运算、内置函数、颜色、`@extend`）编译为展开 CSS。行为由 `sass-spec/*.hrx` 金样钉死。技术栈：Rust edition 2024 / toolchain 1.99 / rxrust `=1.0.0-rc.6` / `Local`  everywhere（wasm32 目标，无 `std::thread`）。

## Goals / Non-Goals

- Goals：整条编译是**一条惰性 owned 值流**；所有权安全；wasm 可编译；每步 `regress=0`；机器可审核。
- Non-Goals：不追 dart-sass 源码/文档当架构参照；不做分布式 GC 式对象图；不在 `src/` 内联测试。

## Decisions

### D1 — 惰性求值，终点 `subscribe` 才合并输出 CSS
整条编译是**冷 observable 的描述**：`token_stream → scan_map/reduce_initial(owned 累加子) → flat_map → collect → map(render) → reduce(join)`。值/选择器在流上以**待求值的 owned 值（`Expr` 或闭包）**传递，**不 eagerly 拼串**；只有公开入口**唯一一次 `subscribe(mpsc)`** 触发时，才把描述求值、合并、产出 CSS 文本。
- 理由：SCSS 的语义是"整体 merge 后才成形"（变量后定义可见性、`@extend`、嵌套展开、惰性 `@if`）。急切求值=GC 式"边造边丢"，也逼出物化树。惰性冷流天然对齐，且 `flat_map` 的"每订阅重放"正对应"多播/一次产出多次消费"。
- 备选（拒）：parse→物化 `Vec<Rule>`/`Vec<Node>`→再遍历渲染（=dart-sass GC 心智，被本会话反复证伪）。

### D2 — 消费自身：值 move 进流、消费一次
每步是"吃 owned 值、产出 owned 值"的算子；值被 move、不 clone 进共享堆、不被多方引用。错误以流内 `Result<T, SassError>`（`Err=Infallible`）下传，终点收口。
- 理由：Rust 所有权即线性——这就是"消费自身"。避免 GC 的"对象存活/可达/回收"整套心智。

### D3 — 循环交给 `flat_map`，一到多交给多播
`@each`/`@for`＝`from_iter(items).flat_map(|item| body_substream(item))`——**不攒 body、不手搓重放循环**。一份产出被多个下游消费（模块单例被 N 处 `@use`、一段解析喂选择器与值渲染）＝**多播** `Subject`/`share`/`publish().refCount()`/`behavior_subject`+`peek`（Local=`MutRc`）。
- 理由：`flat_map`/多播本就是 rxrust 里"一对多/循环"的原生表达；另写 `while`/`for_each` 重放=逃离框架。
- 约束：多播只用于**真正的**一份产出→多消费者；线性一次过仍用 `scan_map`/`reduce_initial`，不把 `Subject` 当共享可变的挡箭牌。

### D4 — 状态/深度/env = 算子携带的 owned 累加子
可变状态是 `scan_map` 的 `&mut acc` / `reduce_initial` 的 move acc 里的 owned 数据（帧栈、env、`@if` 组态），**不是 `&mut self` 对象、不是 `Arc<Mutex>` 共享、不是跨元素手维护的可变状态机**。嵌套深度是数据，不是调用栈。

### D5 — 值建模成 `Expr`/`Value`，从结构化 token 一次成型
`$x`→`Var`、`#{…}`→`Interp`、`+ - * / %`、函数调用在**结构化 token** 上折叠成 `Expr`；`eval(Expr, owned Env)` 惰性求 `Value`（`and`/`or`/`if` 短路）；`Value::to_css` 规范化（逗号 `, `、空白折叠、插值紧邻拼接）。**禁** `stmt_text`/`subst` 式"先拼串再 char 扫"。
- 守门：含 `/ * %` 的值跳过（护 `font:12px/1.5`、`grid:1/2` CSS 斜杠）；函数走**白名单** `call_builtin`，未知名→原样兜底（`url/calc/min/max` 等 CSS 原生函数永不当 Sass 内置求值）；比较/布尔动全局 truthiness 的单列一步、先护选择器（`a > b` combinator 不得当比较）。

### D6 — 生长纪律：隔离→金样单测→守门接线→A/B
新算子/函数先做 **pub 独立件 + 金样单测** 钉死、不接主链；再**只在无歧义处触发**接线；每步 `ledger diff <base> <run>` 守 **`regress=0`**，达不到即回退。

### D7 — 机器审核取代自觉
`crates/sasslint` 规则化本契约并做成 CI 闸 + MCP `review` 工具；须能识别"算子外衣下的状态机"（`from_fn`+可变 `VecDeque`/`Vec<Frame>`、`&mut self` 万能 `Acc`），不止 `for/while` 关键字。

### D8 — 行为 oracle 只有金样
`sass-spec/*.hrx` + `element-plus`；**严禁**以 dart-sass 源码/文档推断行为或架构。

## Risks / Trade-offs

- 惰性 + 所有权 + 无物化树 + 循环交 `flat_map` 的组合对控制流（`@each` 嵌套、`@extend`、模块作用域）是硬约束：若某处"非要有手动栈/重放"才能表达 → 判定为**分解错了**，重造数据，而非改造 rxrust。
- 主链重建期可能回归 → 全程 `regress=0` 闸，绿线不破。

## Migration Plan

以现有绿线为行为基线；按 capability 分增量重建主链（先 values/`Expr`，再 parsing/`@if`/`@each`，再 modules/`@extend`），每增量 `ledger diff regress=0` + `sasslint` 0 命中后合并；不可等价表达者登记阻塞项，不退回 std/命令式。

## Open Questions

- `@extend` 的组合子表达式如何在惰性流上"合并后再解析"而不物化全局选择器集合（倾向：`@extend` 作为对 leaf 流的一次后 `scan`，仍在同一 `subscribe` 前完成）。
