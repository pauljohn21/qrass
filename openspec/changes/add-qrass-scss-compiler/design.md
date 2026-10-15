# Design — qrass：流原生（stream-native）SCSS 编译器

## Context

- `crates/qrass` 已有 flat_map 阶段编排骨架（`pipeline.rs`，24 测试绿）；`Cargo.lock` 锁定 rxrust `=1.0.0-rc.5` 依赖闭包（48 包，含 tracing/tracing-subscriber）
- 需求负载（element-plus theme-chalk 实测语法面，浅克隆 HEAD）：`@use` 544 / `@forward` 13 / `@import` 0 / `as *` 344 / `with (` 0 / `@mixin` 68 / `@include` 1520 / `@function` 14 / `@each` 82 / `@for` 17 / `@if` 31 / `@while` 0 / `@at-root` 12 / `@extend` 5 / `@media` 3 / `@supports` 0 / `@keyframes` 22 / `!default` 195 / `!global` 12 / `#{}` 插值 1086
- 命名空间调用集中在：`map.get` 393、`map.merge` 171、`math.div` 15、`color.channel` 13、`map.deep-merge` 5、`string.*`、`math.round/percentage`、`color.mix/alpha/ie-hex-str`、`meta.inspect`
- 项目规则：edition 2024 / toolchain 1.99；rxrust 所有权风格（禁 `Arc<Mutex>` 承载业务数据）；tracing span 强制；测试外置 `tests/`；单文件 ≤500 行；禁 Python / println
- **坐标系定位**：sass 规范（及 spec 文档）是行为定义的唯一来源；dart-sass 1.105.1 仅作用为**行为 oracle**（离线生成 golden，见 D7）。它的实现结构（类层级、Rational、persistent 集合、异常机制）与性能模型**不是设计输入**——推导任何一条决策时都不引入它的概念（方法详见 D0）

## Core model：编译器的每个阶段都是流迁移器（stream transducer）

数据形态链——每一跳是 `(输入流, owned 状态值, 迁移闭包, 输出流)` 的一个实例：

```
chars 流 ──lex──▶ token 流 ──parse──▶ owned AST（移入流） ──eval──▶ CSS 事件流 ──extend/render──▶ CSS 文本
   ▲                                                                        │
   └────── 插值 #{} 再词法 = inner stream（流中流，同一 transducer 复用） ◀──┘
```

编译器域概念与 rxrust 算子是**同构**关系，不是类比——这是"用足 rxrust"的正面陈述：

| 编译器域概念 | rxrust 算子 | 数据形态 |
|---|---|---|
| DFA 状态机（词法） | `scan_map(&mut Acc, Item) -> Out` | char → `Vec<Token>`（0..N 生长），再 `flat_map(from_iter)` 摊平 |
| 递归下降 / 嵌套结构（语法） | `scan_map` 栈式累加器 + 边界事件 emit | token → `Vec<Stmt>` 片段 |
| 插值再词法、mixin/函数展开 | `flat_map` inner stream（高阶流） | 子树/子表达式 → 子事件流 |
| `@content`（随时可调环境） | 数据对物化 → `from_fn`/`defer` 惰性 inner | `(owned body, Env 快照)`，非 `dyn Fn` 引用闭包 |
| `@extend`（全局约束） | `collect` → 纯函数 pass → `flat_map` 重发射 | 事件 → 事件 |
| `@media`/`@supports` 冒泡 | `scan_map` 携带上下文栈折叠 | 事件 → 事件 |
| 输出序列化 | `reduce_initial(owned acc)` + `finalize` | 事件 → String |
| 模块单例（一次实例化 N 父引用） | `Shared::subject` / `Connectable` 多播 | 实例化流 fan-out |
| 独立分支并发编译 | `Shared::flat_map`（inner 并发）+ `merge_observables` | 模块依赖 DAG |
| 阶段边界（跨函数传流） | `box_it()` → `Stage<T> = LocalBoxedObservable<'static, Result<T, SassError>, Infallible>` | 类型擦除 + 按值 Item |
| 错误 | 流内 `Result` 数据项（Err 通道恒 `Infallible`），失败即融合停发 | 清理靠 RAII（Drop/finalize），无栈展开假设 |

## Goals / Non-Goals

**Goals:**
- 一条可复现的编译流水线：element-plus index.scss 一次编译产出 golden CSS，回归逐行稳定
- sass-spec 现代子集通过率达到可持续收敛的水平（allowlist 显式管理残余）
- rxrust 管道贯穿各编译阶段：阶段内核同步内联（Local），模块编排跨线程并发（Shared + tokio multi_thread，仅此一处运行时依赖）

**Non-Goals:**
- legacy 引擎完整语义（global 函数可覆盖、legacy @import 作用域怪癖）
- nested/compact/compressed 输出风格、source map、`@debug` 交互、indented(sass) 语法
- watch 模式、增量编译（模块并发编排属 Goals，见 D1-L2）

## Decisions

### D0: 设计方法 — 结构由流形状 + 所有权推导，行为由 spec/golden 约束
推导只依赖两样东西：SCSS 源本身是数据（字符流→token 流→树→事件流，形态链见 Core model），以及 rxrust 算子的语义。每个阶段的算法与数据结构由"该数据形态在流世界里的原生形状"决定：状态机就是 `scan_map`，展开就是 `flat_map`，全局重构就是 `collect → pass → flat_map`，共享实例就是多播。**spec 定义的行为**（extend 展开结果、序列化位数、模块单例语义）受 golden 约束；**spec 未定义的部分**（数据结构、算法形态、并发策略）自由设计，不从任何既有实现（尤其 dart-sass）继承。

护栏（防滑回共享引用/参照系直觉，三条）：
1. 共享数据先问"能否按值移入流"；引用型共享（`Arc`/`&'long`）需结构性理由（多播、只读发布）并显式标注
2. 算子闭包只携带 owned 值与状态迁移；需要"随时可调环境"的地方（`@content`）用数据对物化（owned body + 实参 Env 快照），不写持环境引用的 `dyn Fn`
3. 借用只存在于算子闭包生命周期内；借用想逃逸 = 数据流设计错误的信号，重排管道而非加 lifetime/`Rc` 补丁

被否坐标系的教训（记录以防再犯）：初版设计以 dart-sass 的 GC 假设（分配即永久、引用随意共享、闭包捕获环境、persistent 传引用、异常自动清场）为默认前提，后以"GC→所有权翻译表"折中——翻译表本身仍以 dart-sass 为主语，仍是参照系错误。本节方法取代该立场。

### D1: 分层上下文 — 阶段内核 Local 算子链，模块编排 Shared + Subject 多播
**L1 阶段内核（Local）**：每阶段主体逻辑以 rxrust 算子闭包承载（数据流即代码形态，禁止过程式内核包流壳）：
- loader：`Local::from_iter(worklist).scan_map(去重状态闭包)` → owned `Vec<(CanonicalPath, SourceFile)>`
- lexer：`Local::from_iter(chars ⊕ EOF-sentinel).scan_map(Lexer, |lx, ch| feed).flat_map(Local::from_iter)` —— 逐字符状态迁移闭包生长 0..N token 的 reactive transducer
- parser：token 流 `scan_map(ParseState)` 状态迁移，语句在边界事件 emit，`flat_map` 展开；插值 `Piece::Expr` 的再词法是同一 transducer 的 inner stream 复用
- eval：`scan_map(Env)` 按值携带定义注册表；`@include` = mixin 体 inner stream 的 `flat_map` 展开（高阶流 ≡ mixin 语义的数学同构），`@content` = 数据对物化（见 D0，非 dyn Fn 环境闭包）
- render：`reduce_initial` 折叠 + `finalize` 收尾
逐项流数据全部按值 move（`scan_map`/`reduce_initial`/`collect`），mpsc 终端排空，错误走流内 `Result` + `Err = Infallible`；不出现 `Arc<Mutex<业务状态>>`。

**L2 模块编排（Shared + tokio multi_thread runtime）**：两个真实需求决定这一层必须 Shared + 多播：
1. **模块单例 = 多播语义**：sass 规范要求同一 canonical URL 只实例化一次并被 N 个父模块引用（fan-out 拓扑），用 `Shared::subject` / `Connectable`（首订阅者触发实例化，后续复用）表达；实例化完成后发布 `Arc<Module>` **只读**共享（所有权系统的显式共享计数，非 GC；完成即不可变），并发协调仅 `Mutex<HashMap<CanonicalPath, Arc<Module>>>` 注册表。
2. **独立分支并发编译**：element-plus 60+ 组件的 @use 闭包互不相交，`Shared::from_iter(components).flat_map(instantiate)` 兄弟分支跨线程并发，`Shared::merge_observables` 汇聚，`collect` 落地后回到 L1 Local 完成 render。CLI 入口 `#[tokio::main(flavor = "multi_thread")]`。

被否选项：(a) 全 Local + arena move 下标查表——用数据搬运绕开多播，模块 fan-out 与并发两个真实需求都不正面表达；(b) 手写命令式管线（违背 rxrust 核心要求）。单文件直通路径保留为 L2 的退化情形（依赖闭包 = 1 时无并发分支）。

### D2: 数值/颜色内部表示用 f64，序列化行为受 golden 约束
数字类型的内部表示不是对外契约：spec 只约束可观察序列化（10 位有效数字、`math.div(1,3)*3` 等数值用例），由 golden 验证。流视角的原生选择：owned `SassNumber { value: f64, unit: Option<String> }`，无间接层无分数对象；element-plus 计算面（px/rem、颜色通道取整）在 f64 下与基线差异风险低。仅当 sass-spec/回归出现精度失败用例时，才升级局部分数表示——升级依据是失败证据，不是模仿任何实现（含 dart-sass 的 Rational）。

### D3: 模块系统只做现代语义 + 一次实例化缓存
模块身份 = canonical 文件路径；单例缓存机制见 D1-L2（`Shared::subject` 多播 + `Arc<Module>` 只读发布 + `Mutex` 注册表协调）。`@use` 支持默认名/`as`/`as *`、私有成员过滤、`with` 配置（element-plus 虽未用，sass-spec 现代用例需要）；`@forward` 支持 show/hide/prefix。SCSS `@import` 不实现（编译报错 unsupported，CSS `@import`/url() 透传）。循环 `@use` 在图构建期检测（实例化流携带父路径栈）。被否：完整 legacy 合并（工作量翻倍且无目标负载）。

### D4: `@extend` 以全局约束 pass 实现（collect → 纯函数 → flat_map）
实测 5 处全部为 placeholder 扩展：`image.scss` 的 `%size`（同文件）与 `mixins.scss` 的 `@mixin extend-rule($name)` 动态插值扩展 `%shared-<name>`（配合 `share-rule` 用 `@at-root` 定义，同模块）。精读补充（table.scss）：placeholder 在条件 mixin 路径（`m((group, border))` 体内）定义，且可能定义于 extend 站点之后，同模块 5 个 extend 站点各自携带不同嵌套上下文。这个依赖顺序**由 extend 语义自身决定**（发射点无法局部解算），因此流形态必须是整模块收齐后的全局 pass：`collect → extend_pass（纯函数：placeholder→targets 映射上的选择器并合，展开结果按 sass extend spec 定义）→ flat_map 重发射`。实现范围："同模块 placeholder 并入目标选择器 + `!optional` + 插值 placeholder 名 + 空 placeholder 不输出"即可覆盖；跨模块 extend、复杂 combinator 由 hrx runner allowlist 管理。

### D5: BEM mixin 语义以 element-plus 实测为准（精读结论）
BEM 体系 = `mixins/config.scss`（`el`/`-`/`__`/`--`/`is-`）+ `mixins/mixins.scss`（b/e/m/when/share-rule/extend-rule）+ `mixins/function.scss`（selector 内省 + bem()/getCssVar）。eval 引擎硬需求：
- `$B`/`$E … !global`：b()/e() 跨 mixin 调用维护全局状态，mixin 体内部赋值必须写穿 global 层（Env 栈底全局帧，D0 护栏 2 的物化闭包不豁免此语义）
- `e($element)`：用全局 `$B` 重组 `.B__E`（不继承 `&` 装饰）；先判 `hitAllSpecialNestRule(&)`（`&` 含 `--`/`.is-`/`:`）命中则改为嵌套形输出（后代组合）；否则 `@at-root` 拍平
- `m($modifier)`：以 `&` 为基底串接 `--unit`；`e`/`m` 的 `@each $unit in $arg` 要求标量字符串按单元素列表迭代（`e((mask, top-mask, …))` 列表形式实测存在）
- 插值拼接的选择器组字符串（尾逗号串接）需 reparse 回 compound selector list
- `@at-root` 双形态（块式/头式）只提升直接包裹规则；badge 实测 `e>when>when` 三层链依赖此精确语义
- Selector 是一等值类型：`$selector: &`、`meta.inspect(&)` 加引号序列化（`selectorToString` 用 `string.slice(2,-2)` 剥引号，需负索引支持）
- 变长参数 `getCssVar($args...)` → `joinVarName($args)` 转传；`@media only screen and #{string.unquote(map.get(...))}` 插值；`@each $k,$v in map` 遍历 map（picker-popper）；`color.channel` + `math.round`（roundColor）；`()` 空 map 字面量 + `map.merge` + `!default`
- 组件顶层重复 `@include b(button)` 按序独立输出两条规则（不合并），golden 顺序敏感

### D6: 内置函数双注册表共享实现
`BuiltinRegistry`：`namespaces: HashMap<&str, FnTable>` + `global: FnTable`；注册时 `map.get` 与 `map-get` 指向同一 `fn`。实现优先级按实测调用面（map.* → math.div/round/percentage → color.channel/mix/adjust/alpha/ie-hex-str → string.* → meta.inspect/type-of → list.* → selector.*），其余 sass-spec 高频函数随后补齐。`math.sqrt/log/pow` 用 f64。函数体是纯闭包 `(args: Vec<Value>) -> Result<Value, SassError>`——无环境捕获（变量可见性由调用点 Env 解决），与 D0 护栏 2 一致。

### D7: golden 基线的生成与回归分离
- 基线源 = **element-plus 官方构建链路**（精读 `packages/theme-chalk/buildfile.ts`：每个 `src/*.scss` → `sass-embedded` compileAsync（dart-sass 1.105.1，expanded）→ lightningcss minify → `dist/el-*.css`；聚合入口 `src/index.scss` 115 个 @use；dark 单独入口）。已用官方命令 `pnpm -C packages/theme-chalk build` 产出完整 dist，并另存 expanded 主对比件：`crates/qrass/tests/golden/element-plus/{index.css, dark/css-vars.css, dist/}`；基线为静态数据提交，更新靠同一链路（版本由 frozen-lockfile 钉死）
- 回归：`tests/element_plus_e2e.rs` 将 qrass 编译 `index.scss` 的输出与 golden `index.css` 对比（归一化空白），不一致即失败并报首个偏差行；组件级 min 文件（dist/el-*.css）用于定位单组件偏差
- 被否：用 qrass 自产基线（自己证明自己，失去正确性信号）；每次运行记录基线（掩盖回归）。注："纯 Rust 门禁"约束的是 qrass 实现与测试代码（均 Rust，无脚本），不禁止用外部参考实现离线生成静态基线。

### D8: 文件结构与 500 行红线
`crates/qrass/src/`：`lib.rs`（编排 compile）、`main.rs`、`error.rs`、`span.rs`、`token.rs`、`lexer.rs`（状态/入口/feed 分发）、`lexer_detail.rs`（各状态迁移 handler）、`ast.rs`、`ast_expr.rs`、`parser.rs`、`parser_expr.rs`、`pipeline.rs`（Stage 编排）、`loader.rs`、`module.rs`、`value.rs`、`env.rs`、`eval.rs`、`eval_stmt.rs`、`eval_expr.rs`、`builtins/{mod,math,color,map,string,list,selector,meta,global}.rs`、`css.rs`、`tracing_setup.rs`。每文件超限再按语法域拆 submod。`tests/`：`common/{mod,hrx,css_diff,css_validate,golden}.rs` + `lexer.rs/parser.rs/eval.rs/builtins.rs/scss_modules.rs/hrx_spec.rs/element_plus_e2e.rs`。

### D9: 可观测性即调试协议的一部分
`compile()` 以 `tracing::span!(INFO, "compile", stage=..., module=..., id=...)` 包裹每阶段（span 在实际执行算子的闭包内 enter，`tap`/`map` 记录 `elapsed_ms`），字段含文件路径与耗时；错误路径 `error!` 附 span 上下文。测试内编译用 `#[tracing::instrument]` 自然继承。禁止 println 由 clippy 无强制——以 code review + grep 检查把关。

## Risks / Trade-offs

- [滑回参照系/共享引用直觉（本项目最大的方法论风险）] → D0 三条护栏逐条自检；出现借用逃逸视为重排管道的信号而非加 lifetime 的许可
- [照搬 GC 结构撞 lifetime 墙] → 依 D0/D1：所有权进流、借用限闭包内、@content 数据对物化；无法落地即回 Core model 重选形态
- [f64 精度与行为基线序列化差异导致 golden 偏差] → 先以覆盖/结构校验固化自身一致性；对照 sass-spec `core_functions` 数值用例收敛；必要时升级 D2 为分数表示（失败驱动）
- [@extend 不完整导致 element-plus 个别选择器缺失] → e2e 覆盖断言精确定位；D4 范围内修复，超出则记 allowlist 并与用户确认扩围
- [rxrust rc.5 API 不稳定（scan_map/reduce_initial 签名变动）] → 锁 `=1.0.0-rc.5`（Cargo.lock 已固化），编译错误集中在 lib 边界易替换
- [sass-spec 子集仍含隐式 legacy 假设的用例] → runner 以 hrx options 段判定（如含 `:libsass` 或 legacy-only 目录直接 skip 并标注）
- [Shared 并发的 Send/'static 约束渗透进阶段内核造成锁税] → 分层边界钉死（D1）：高频逐项流必须留在 Local；模块编译单元整链在同线程推进，仅产出跨线程
- [单文件 500 行拆分过碎增加心智负担] → 按数据形态域（token/stmt/expr）切分，mod.rs 仅 re-export

## Migration Plan

全新增量，无迁移与回滚风险：分 6 期实施（见 tasks.md），每期结束 `cargo test -p qrass` 保持可运行；不触碰 bootstrap/sass-spec/element-plus submodule 内容（element-plus 仅作负载与基线来源，只读）。

## Open Questions

（无——原"@extend 上下文强度"已调研关闭：5 处均为同模块 placeholder 扩展，见 D4；"是否全 Local"已论证关闭：模块单例与并发两个结构性需求要求 L2 Shared + 多播，见 D1-L2）
