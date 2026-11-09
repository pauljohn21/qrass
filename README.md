# qrass —— 托管 **sasspile** 的工作区

一个 clean-room **SCSS → CSS** 编译器，写成一个响应式 *push* 管线：每个编译相都是一个真正的
rxrust **算子内核**（`struct Phase { source }` + `PhaseObserver { observer, state }` +
`ObservableType`/`CoreObservable<C: Context>`），把 **owned** token、事件、规则与值一路往前传，
各自发出自己的 `0..N` 个元素 —— `from_iter → Lexer → Structurize → Exec → Render →
reduce_initial → into_future`。路径上既没有需要递归遍历的物化 AST，也没有任何
`Arc<Mutex<_>>` 业务状态。

> 历史：本仓库曾存在 `crates/qrass`（一个 pull 形态的编译器）。项目采纳 push 教义后它被
> **删除**；该 crate 目录已不复存在，其代码不得被当作模板。唯一实现是 `crates/sasspile`。

## 工作区

| Crate | 角色 | 规模（2026-10-29 实测） |
| --- | --- | --- |
| `crates/sasspile` | 编译器本体 —— **只有 library、无 CLI**；仅一个公开出口 | 30 个 `src/*.rs`（含全部层级）、8,606 行、29 个测试目标 |
| `crates/ledger` | A/B 记录：hrx 快照的具名 run 存在一套 SQLite 表里，`ledger diff` 出 `regress`、`ledger changed` 出失败形状 | 1,255 行（src 662 + tests 593） |
| `crates/sasslint` | 本项目铁律的确定性规则闸（外加 `sasslint-mcp` bin） | 1,202 行（src 500 + tests） |

编译器的源码布局：`lib.rs`（唯一链）、`phases.rs`（四个内核）、`lexer.rs`、`exec.rs`、`expr.rs`、
`value.rs`、`selector.rs`、`env.rs`，外加模块目录 `src/builtins/`（每个函数域一个文件 —— `math/`
与 `color/` 还有自己的子模块）与 `src/directives/`。

> **本文件中的数字都是快照。** 基线、机器闸与当前前沿的唯一真相源是 `.qoder/rules/rust.md`（头条）
> 与 `openspec/changes/redesign-sasspile-rxrust-shared-stream/tasks.md`（每刀证据）及
> `reports/hrx.db`（A/B 记录）。若本表与之不符，以它们为准、本表已过期 —— 请重新测量而非照抄。

`crates/docsizer` —— 一个已取消的教程计划的计数工具 —— 已从工作区**移除**（它悬空的成员条目会让
每次 `cargo` 调用都失败）。不要重新造它。

## 状态（闸命令；基线数字只住一处）

hrx 基线与 invalid-error 基线只有**一份拷贝**：`.qoder/rules/rust.md` §基线。A/B 记录是
`reports/hrx.db`，当前参照 run 名也在其中；本 README 故意**不复述**那些数字，因为本项目里每份
复述的拷贝最终都在已过期时被当作证据引用（2026-10-28 实测：下表当时引用的基线已被两个批次越过）。

| 闸 | 命令 |
| --- | --- |
| 测试 | `cargo test --workspace --no-fail-fast` —— 读退出码，不要读最后一行 `test result` |
| 规则闸 | `cargo run -p sasslint --bin sasslint -- lint crates/sasspile/src` —— 必须报 `count=0` |
| 规则闸（CI 形态） | `cargo test -p sasslint`（`gate_sasspile_is_clean`；已不再 `#[ignore]`） |
| 一致性快照 | `cargo test -p sasspile --test hrx`（`total` / `pass` / `pass_substantive` / `pass_empty_expectation`） |
| 每步 A/B | `cargo run -p ledger -- diff <base> <run>`（必须 `regress=0`）**加上** `… changed <base> <run>` —— 后者抓住 `regress`/`net` 保持沉默期间*形状*变化的失败，一个伪造值正是这样藏在 `regress=0 net=11` 背后的 |
| 伪错误计数 | `cargo run -p ledger -- failures <run> "unexpected error" --count` |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` |
| 计划 | `openspec validate redesign-sasspile-rxrust-shared-stream --strict` |

**P0′（框架原生重写）已收尾**：各相皆内核（0′.1/.2）、顺序来自流（0′.4）、`Err` 通道为真且
`UndefinedVariable` 致命（0′.3）、四条铁律已机器闸化（0′.6）、选择器/属性名为结构化值（0′.7）。
只剩 0′.5（共享态 `C::RcMut`/`C::RcCell`）未做，而在 P4/P5 引入共享态之前它没有什么可改。
P1（值的地基）大体已就位 —— 带单位的数字、结构化选择器、带短路 `and`/`or`/`if` 的
`Expr`/`Value`、指数字面量 —— 色彩域大部与 `math`/`color` 核心函数族亦然。下一个前沿是
**`none` 通道值**（`color/mix/missing` 中最大的剩余红簇）、**P7 诊断消息**与 **P8 现代色彩空间**。
「哪些已实现、哪些没有」的实测清单见 `.qoder/rules/rust.md`。

通过率 —— 请从上方的唯一拷贝读，不要读本句 —— **不是**回归信号：架构在 P0–P8 逐相重建期间，
编译器是故意不完整的。一轮绝不能做的是丢掉原本通过的用例 —— 那由
`ledger diff <base> <run>` 要求 **`regress = 0`** 判定，而非原始通过率。`tests/hrx.rs`
出于同一原因不做逐用例断言。

**读 `pass_substantive`，不要读 `pass`。** 通过的用例里有 165 条对的是期望 CSS 为空的 fixture ——
合法地正确（Sass 在那里确实什么都不输出），但一个什么都不产出的编译器同样能拿到这个分，所以它们
对任何特性都没有证明力。当初发现这点的审计：`directives/{if,for,while}` 里显示 39 个「通过」用例，
**全部 39 条**期望为空，即 `@if`/`@each`/`@for` 当时根本哪里都没实现，尽管 `tasks.md` 另有声明
（现已更正；字面整数范围的 `@for` 后来已建，其行以值钉在 `tests/loops.rs`，直接取自
`directives/for/for.hrx` —— 它移动多少用例是一次 `ledger` 读数，不是本文件重复的数字）。
harness 每次运行都会同时报这两个数字。

## 一致性权威

- **只有** `sass-spec` 子模块下的 `.hrx` golden 定义行为（`input.scss` → `output.css` / error 块）。
- `element-plus` 用作**非回归自基线** —— 绝不作为另一个实现产出的期望输出来源。
- **不得**查阅 dart-sass 源码或文档来推断行为；它是*语言是什么*的参考，不是*我们的输出该怎样*的参考。

```bash
git submodule update --init element-plus sass-spec
```

## 库用法

```rust
use sasspile::compile;

async fn build(scss: String) -> Result<String, sasspile::SassError> {
    compile(scss).await          // 唯一公开入口；接内容，不接路径
}
```

诊断走 `tracing`（绝不 `println!`），所以返回的 `String` 是纯 CSS：

```bash
RUST_LOG=info cargo test -p sasspile --test hrx -- --nocapture   # `trace` 会加每阶段的 span
```

## 管线

一条冷链、六个环，写在 `compile` 里、其他地方没有 —— 没有任何阶段函数接收或返回 observable 类型，
没有 `box_it()` 缝，没有 `Stage`/`fail`/`drain` 编排层：

```
chars ─▶ Lexer ─▶ Structurize ─▶ Exec ─▶ [全局 pass] ─▶ Render ─▶ reduce_initial ─▶ into_future()
             （每个 ▶ 都是一个内核：struct{source} + Observer 持有本相状态，
              用 observer.next(..) 发 0..N 项；Err 保持泛型）
```

它所取代的那条链（`scan_map → Vec<Tok> → flat_map(from_iter)`，然后 `scan(State)` +
`filter_map` + 一个 `order` 键 + `collect`/`sort_by_key`）已**删除**，重新引入其中任何部分都会
被 `sasslint` 拦下。嵌套不是树：`Exec` 持有一个 owned `scopes` 向量，在两个时刻落定一条规则
（嵌套块打开时冲刷父块待定声明；块关闭时冲刷残余），所以输出无需排序、一个块可以出多条规则。

真正需要全体规则集的横切 pass（`@extend` 不动点、媒体冒泡）表达为 `collect` + 一次**纯 forward
fold**，再经 `flat_map` 推回流 —— 绝不是「可变对象图 + walker 遍历」。

## 反 GC 红线（写任何管线代码之前先读）

下面每一行都是本仓库真正交付过、被迫返工、如今由规则 + `sasslint` 闸拦下的形状。它们写在这里
（不只在长篇文档里），是因为一个新会话不应该重新把它们发现一遍：

1. **每个相都是真算子内核**，不是批量外壳：`struct Phase { source }` + `PhaseObserver { observer, state }`
   + `impl ObservableType` + `impl CoreObservable<C: Context>`，经 `observer.next(..)` 一条一条发
   `0..N` 项。禁止：`scan_map` 返回 `Vec` 再 `flat_map(from_iter)` —— 那是把一到多藏在容器后面。
2. **顺序来自流，不来自排序。** `order`/序号键加 `sort_by_key` 是「*发射时刻*建模错了」的症状
   （Sass 在嵌套块打开时落定一个块的待定声明段、块关闭时落定其余 —— 所以一个块可以出几条规则）。
   排序也找不回元素：`merge_all` 会**丢弃**仍在排队的内部源的发射。用 `concat_map`/`concat_all`
   或串行链。
3. **错误走真 `Err = SassError` 通道**（`Observer::error` → `into_future()` → `Ok(Err(..))`）。
   拿 `.ok()` / `unwrap_or_else(source_text)` 兜底不是错误策略：它把真错误静默变成错误输出，
   掩盖 spec 的 `errors/**` 期望。只有 spec 定义为逐字的构造（不可归约的 `calc`、自定义属性、
   未知 at-rule）保持原样。
4. **没有假并发**：把微秒级的纯 CPU 工作包进 `from_stream(stream::once(..))` 不是并行（实测：同一
   工作线程）。真扇出只属于独立的模块子图（P5）。
5. **只经框架的终点关闭**：`collect`/`reduce_initial` + `into_future()`、`into_stream()`（`Drop`
   时取消），或等 scheduled 源返回的 `TaskHandle`。`std::sync::mpsc` 是为需要 `Sender` 的调用方
   准备的桥，绝不是终点。
6. **多播不可重入**：在 `Subject` 自己的订阅回调里调 `next`/`error`/`complete` 会 panic；反馈环
   插 `delay(Duration::ZERO)`。还有 `Subject::is_closed()` 意思是「无订阅者」，不是「已终止」。
7. **名字的位置不是变量引用。** 实测过的三种形状，都是同一个错误 —— 在 Sass 意为*标签*的地方去查
   `$name`：块头绑定（`@for $i from …`）、实参标签（`color.hwb($hue: 180, …)` —— 1,611 条用例）、
   绑定的声明旗标（`$a: b !default`）。块头只解析 `#{ … }`；传命名实参的 run 根本不求值；
   `!default`/`!global` 只从绑定值上剥掉。
8. **分类器永远不可能细过 lexer 的词素。** 判断「这是 vendor-prefixed CSS 调用吗」或「这是父引用
   `&` 吗」曾在 `-a-calc(` 是三个词素、`&` 被粘成词的一部分时失败 —— 修复都是词法层的（`-` 延后
   一字符，`&` 是 `STRUCTURAL` 词素，引号串跟踪 `$`/`\`）。另两条词法级事实：`//` 注释的行尾是
   `Space` 而不是 `Word(" ")`；`#{&}` 里的 `&` 是文本替换，不是父位置。
9. **判据必须判 spec 所陈述的东西。** `error` 用例只有在编译器消息等于 fixture 的 `Error:` 头时
   才算通过 —— 把「任何错误」都算通过曾报出虚假的 **+385**，真实增量是 **+1**。同样：用例 key 必须
   唯一（`<basename>.hrx::<case>` 曾碰撞 1,135 次，其中 33 次同带 PASS 与 FAIL），且 harness 的扫描
   目录清单必须等于计划点名的目录 —— `operators`/`parser`/`variables` 有 79 条用例根本没人测。
10. **一个事实只住代码里的一处。** 一条法律写两遍是 bug 之源而非注释：可换算单位表曾同时存在于
    `unit::UNITS` 和 `builtins/math.rs` 的一组长度字面量里（已删 —— `math.hypot(3cm, 4mm * 10, 5q * 40, …)`
    需要 `q`，只有共享表有），比较现在只住在 `math.rs::compare` 一处、带两种消息渲染
    （`comparable`/`compare_named`），三个颜色族共用 `color/modify.rs::route(given, writing)`。
    新增第二张表/规则是回归；合并它。
11. **读数只属于产生它的命令。** 构建失败时，链里的下一个命令会打印*先前记录*的数字 —— 一个错
    误的 `net` 就是这样读出来的。而一个「在错地方找东西」的工具给出的 0 不是发现：
    `grep --include=input.scss` 对着 `.hrx` 归档只能返回 0，因为那是单文件归档、磁盘上根本没有
    这类文件。过期摘要与记忆中的 dump 同样不是证据 —— 一个提出「指数打印」的切口在语料被 grep
    后当场作废：没有任何期望输出要求科学计数法。

**验证本身也并非不证自明。** `ledger` 只存失败，所以重新导入一个 run 名必须先清掉该 run 的旧
`run_fail` 行（现在如此，且有测试）—— 否则一个已修复的用例会报出幽灵 REGRESS。在信赖 `regress=0`
之前，拿一个独立的数据库重做 A/B（`HRX_DB=<scratch path> HRX_RUN=<name> cargo test -p sasspile --test hrx`），
而行为中性重构则靠「记录两边、展示 `ledger diff` 给出 **net = 0**」自证 —— 已没有任何快照文件
可以再 `md5`。本仓库真正踩过的另两个陷阱：(a) 一个读出 **0** 的统计量在已知阳性用例证明测量
代码可用之前都是 bug（一个 `substr` 差一错误吞掉一个引号、报出“0 行仅空白/大小写不同”，差点
产生错误结论）；(b) **一个 FAIL 变成另一个 FAIL 在通过数里不可见**，所以还要数 invalid error
—— `ledger failures <run> "unexpected error"` 过滤的是存储的 detail 文本（今天 55；该查询自身的
bug 与测试记在 `crates/ledger/tests/failures_query.rs`）。A/B 记录是 **`reports/hrx.db` 里的一个具名
run**，由 harness 直接写入 —— 绝不用松散的快照文件，那正是 `ledger` 存在的意义（要消除的杂散物）。

## 规则与知识住在哪

| 文档 | 权威级别 |
| --- | --- |
| `.qoder/rules/rust.md` | 项目硬规则（always-on）：新会话起飞前检查、rxrust 作为引擎、tracing span、≤500 行文件、测试在 `tests/`、无 `println!`、无 Python/脚本、SSH 推送，以及测量红线 |
| `docs/rxrust-source-analysis/`（01–09 + README） | **权威** rxrust rc.6 源码阅读；`09-…-ordering.md` 持有取消/终点投递次序不变量，以及「库自身 doc 注释与实现相矛盾」清单 |
| `.qoder/skills/rxrust-reactive/` | 写管线时应用的 do/don't 教义（`SKILL.md` + `reference.md`） |
| `openspec/changes/redesign-sasspile-rxrust-shared-stream/` | **active plan**：架构（`design.md`）、相任务（`tasks.md`，含每条声明背后的实测数字）与正在实现的需求增量（P0′、P0–P8） |
| `docs/rxrust-ai-manual.md` | **历史**（针对 rc.5 写成）；被上两者取代 —— 信赖细节前先对源码核实 |
| `docs/sasspile-framework-design-report.md` | 设计报告的日期快照，其「现状校正」小节承载更正 |
| `openspec/changes/add-rxrust-line-by-line-tutorial/`、`docs/rxrust-from-scratch/` | **已取消**的教程计划，仅为历史保留：不要应用、继续或引用其行为 |

`reports/hrx.db`（gitignored、本地）是 A/B 账本；`ledger runs` 列出已记录的 run，
`ledger diff A B` 重算上述文档里的任何声明。

## 基本规则

- Rust edition 2024（toolchain 1.99）；rxrust 锁 **`=1.0.0-rc.6`**（裸 `cargo add rxrust` 仍会解析到
  不兼容的 0.15 线）。
- 所有权优先于共享可变：状态以值的形式由流携带前行。
- `Shared`/线程只用于真正独立的工作；扇出的顺序陷阱（排队的内部 observable 会**丢**发射 ——
  `sort` 找不回）记录在 `docs/rxrust-source-analysis/09-…`。
- 无 `println!` / `eprintln!` —— 用 `tracing` 宏，跨函数阶段包 span。
- 无内联测试：全部放 `tests/`。每个源文件或测试文件 ≤ 500 行。
- 代码查询先走 **`codegraph_explore` MCP 工具**（见 `.qoder/rules/codegraph.md`）；无 MCP 时
  `codegraph` CLI（`explore`/`node`/`callers`/`impact`）是同等语义的后备。正确性仍由编译器与
  测试裁决。
- 经 SSH 推送：`git push github main`。

## 已知缺口（记录在案，不藏着）

- 语言覆盖处于早期阶段：值/类型语义（`*`、`/` 与斜杠-vs-list 歧义、`Calculation`、**复合单位**、
  颜色）、选择器/`@extend`、模块 `@use`/`@forward`/`@import`、可调用物与 CSS at-rule 都在 OpenSpec
  change 里有规定，但只部分实现。
- 内置调用脊已存在（`calls.rs` + `builtins.rs`，tasks 1.7a），目前共**十一**个签名（`math.div`、
  `round`、`abs`、`ceil`、`floor`、可变参数 `min`/`max`，以及 `unit`、`is-unitless`、`percentage`、
  `compatible`）；无签名的被调名绝不猜测、逐字保留 —— 这就是大片 `core_functions/*` 簇仍红的原因。
  参数类型检查**按函数逐个**做，因为语料不同：`math.round(c)` 必须以 `$number: c is not a number.`
  失败，而 `math.div(c, 3)` 只警告且仍产出值。单位换算因子只来自 oracle 打印的数字
  （`libsass/units/conversion/size.hrx`），未知单位选择「婉拒」而不是判不兼容。
- 剩余 `core_functions/color` 失败大多需要**真消息与真函数行为**（P1.5/P1.7 + P7 诊断），而不是错误
  管道：为此语料被逐 case 分桶确认过，数字在 `tasks.md`。
- 尚无 CLI 二进制 —— 该 crate 设计为纯库（`compile` 是唯一出口）。
- harness 只跑 `.scss`（无 `--indented`）、跳过 multi-file 与 `options.yml` 用例、只比 `Error:` 头
  （无 span/`Warning:`/deprecation）—— tasks 0.2/0.3/0.4。

## 许可证

MIT —— 见 [LICENSE](LICENSE)。vendored 子模块（`sass-spec`、`element-plus`）保留各自的许可证。
