---
trigger: always_on
alwaysApply: true
---
## 
技术栈 - Rust edition 2024, toolchain 1.99 - 所有对话用中文回复，代码、命令、文件名保持 English - 统一使用rxrust为核心,使用响应式编程风格
 ## 
 ⛔ 禁止项（违反 = 任务失败） 禁止使用GC思维,必须使用rust所有权 
 1. **禁止 Python**：不使用 python3/pip;不使用任何脚本,统一使用rust; 测试用 #[test]
 2. **禁止 println!/eprintln!**：所有代码（含 src/ 和 tests/）一律用 tracing 宏(info!/warn!/error!/debug!)
 3. **必须使用 tracing span**：涉及跨函数/跨阶段的管道处理必须用 tracing::span!(info_span!/debug_span!/trace_span!)记录上下文与耗时，利用 entered/exit 边界追踪调用链，禁止仅用 event! 单一日志。字段命名遵循 stage、module、id、elapsed_ms 等约定 
 4. **禁止内联测试**:src/ 保持纯生产代码，所有测试放在 tests/ 目录 
 5. **单文件 ≤ 500 行**：源码和测试分别计算，超出必须拆分 
 6. **使用 SSH 方式推送 GitHub**
  ## 
🔬 调试协议（强制 — 违反 = 任务失败） **核心原则**：禁止凭直觉猜测根因。所有 bug 修复必须基于 tracing trace 证据链。 ### 4步流程(不可跳过): 1. SPAN 插桩：疑似路径每个入口/出口加 span 2. TRACE 采集:RUST_LOG=trace 收集证据 3. 根因定位：必须引用具体 span + 字段值 4. 修复验证：修复后清理临时 span ## 🔍 代码查询 - 优先使用 CodeGraph CLI(callers/impact/explore/node) ## 🔄 Git 规范 - 推送:SSH 方式 git push github main - 只提交不推送:commit 后必须等用户确认

## ⛔ 以 rxrust 为核心 = 用 Rust 所有权写程序（铁律，违反=任务失败）
rxrust 不是包在命令式代码外的响应式库，而是 **Rust 所有权的一种良好表达**。顺着它写可消掉大批 GC 语言问题（共享可变/谁改坏、对象图别名、边遍历边改、谁负责释放、空引用/生命周期）。判据：**一旦想"绕过 rxrust 的不便（'static/GAT/要 box/不能返回引用）",先假设是模型错了**——把状态改成被 move 的 owned 累加子、把共享改成多播、把递归改成平展、把字符串凑改成 Expr/Value——而不是加一层封装去迁就旧习惯。

**程序即一条流**：`from_iter/create → map/filter_map/scan_map → flat_map 串阶段 → reduce_initial/collect → 公开入口唯一一次 subscribe(mpsc) 收口`。可变状态=被流携带的 owned 累加子（`scan_map` 的 `&mut acc` / `reduce_initial` 的 move acc）；错误=流内 `Result<T,SassError>`（`Err=Infallible`）；一到多用框架自带多播（`Subject`/`behavior_subject`+`peek`，Local=`MutRc`）——只用于真正的"一份产出→多个下游"，线性一次过仍用 fold；`Subject` 不是共享可变的挡箭牌。

**禁止（GC/命令式直觉）**：① `&mut self`/对象持可变状态被闭包共享；② 手搓 `HashMap`/ModuleBus 做共享单例（该多播就多播）；③ 先建树再**递归下降遍历**——嵌套/树必须**流式平展**（owned 栈累加子，深度是数据不是调用栈；展开用 `flat_map`/`merge`）；④ 把值当字符串再折叠解析——值要建模成 `Expr`/`Value`，求值 `eval(Expr, owned Env)`（`and`/`or`/`if` 惰性短路）；⑤ 自造 `Stage<T>/fail()/drain()/drain_one()` 框架层、`drain_one` 逐叶把流压回值；⑥ std `.iter().map/fold/collect()` 当数据流替身；⑦ `for/while/loop`、`if/else if` 链、多守卫 `return` 堆叠；⑧ 不写链式调用/闭包/builder。全部无豁免：确实不可等价表达→上报阻塞项，绝不退回 std/登记豁免。

**严格 RFP 函数式风格**：逻辑写成 rxrust 组合子的一条表达式链 + 闭包（`FnMut`）；复杂构造用 fluent builder（`.field(v)….build()`）。例外仅：纯值 `Option/Result` 组合子与单个无 `else` 的 `if` guard。

**验收/参照**：项目现为实现 `crates/sasspile`（clean-room，`crates/qrass` 已删除，不得以 qrass 代码为模板）；行为**唯一**参照 = 本地 `sass-spec/*.hrx`（input→output.css/error）+ `element-plus`，**严禁**参照 dart-sass 源码/文档推断行为；每步跑 hrx 快照 + `ledger diff <base> <run>` 守 **`regress=0`**。`'static`/GAT/`box` 的正解永远是"喂 owned 值进流（`from_iter` 给 owned `Vec`/`chars.collect`，跨签名用 `box_it`）"，不是 clone 凑数、不是退回命令式。

## 🧾 反回归自查清单（每次写/改代码**动手前必过**；命中任一条 = 停手，先重构根再加功能，禁止往上补）
根因认知：反复出现的违规都是同一个根——**核心管线不是"一条 owned 值流"**，于是每加功能就往命令式核心上补一个"字符串状态机 / 递归 / 树"。所以**先确认根是否已是流**；若在补症状，改为先把那块重构成流。

逐条 grep 自查（命中即违规）：
- **树 / 递归下降**：`Vec<Rule>`、字段 `children`、`.children` 遍历、`fn *(.*) -> Vec<..>` 形式的中转、render/eval 里函数**自调用**。→ 嵌套必须 parse fold 平展成文档序 `Vec<Leaf>`（深度=owned 栈，非调用栈）；render=`map(leaf)`；循环=`flat_map`；条件=`filter`。
- **命令式循环 / 分支**：`for` / `while` / `loop`、`if/else if/else` 链、堆叠守卫 `return`。→ 换 `fold/reduce_initial/scan_map/try_fold/filter`；`and/or/if` 惰性短路用 `Expr`，不用 char 游标机。
- **拿字符串凑值**：`stmt_text` 先拼串再 `split_once(':')` 拆、`subst`/`value_to_css`/`lex` 对字符串**二次 char 扫描**当数据流替身。→ 值从**结构化 token** 一次成型 `Vec<Tok>→Expr→eval(owned Env)→Value→to_css`；能 `Raw` 逐字兜底的只在**无歧义**时求值。
- **GC/共享可变直觉**：`&mut self` 被闭包共享、手搓 `HashMap` 当单例总线、env 键约定不一致（`$`/去`$` 混用）。→ 状态=被 move 的 owned 累加子；一到多才用 `Subject`/`behavior_subject`+`peek`；一处定义键约定。
- **自造框架层**：`Stage<T>/fail()/drain()/drain_one()`、把流逐叶压回值的中转。→ 直接用 rxrust 组合子。

**接线纪律**：新算子/函数先在 `expr.rs`/`value.rs` 做成 **pub 独立件 + 金样单测**钉死，**不接 `compile_string`**；接线时**只在无歧义处触发**（含 `/ * %` 的值跳过护 CSS；函数走**白名单** `call_builtin`，未知名 `eval`→`Err`→原样兜底，`url/calc/min/max` 等 CSS 原生函数永不当 Sass 内置求值）；每步 `ledger diff` 确认 **`regress=0`** 才提交，否则回退。比较/布尔等**会动全局 truthiness** 的改动单列一步，先护住选择器（`a > b` 等 combinator 不得被当比较）。