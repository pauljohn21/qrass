---
trigger: always_on
alwaysApply: true
---

## 技术栈与语言
- Rust edition 2024，toolchain 1.99。所有对话用中文，代码/命令/文件名/注释保持 English。
- 以 **rxrust 为核心**（见「铁律」）。版本钉 `=1.0.0-rc.6`。

## 🧭 新会话起手（先读这三处再写码，否则必返工）
1. `docs/rxrust-source-analysis/`（rc.6 逐行取证，01–09；**09** = 取消/锁/终止投递次序不变量 + “库自带注释与实现不一致”清单）；
2. `.qoder/skills/rxrust-reactive/SKILL.md`（动手前“先选动词”清单 + 本仓库踩过的 GC 形态）；
3. `openspec/changes/redesign-sasspile-rxrust-shared-stream/design.md` + `tasks.md`（现状：**P0′ 已收尾**——四个内核
   `Lexer/Structurize/Exec/Render` 在 `crates/sasspile/src/phases.rs`；0′.1/.2/.3/.4/.6/.7 均已合入：真 `Err` 通道已接且
   `UndefinedVariable` 已致命（守卫：头部只认 `#{}`、special 调用参数、plain-CSS at-rule 体）、选择器/属性名是
   `src/selector.rs` 的结构化值（`&` 就是词素，`str::replace` 已删）、四条铁律已成 `sasslint` 机器闸（CI 门已去
   `#[ignore]`）。P1 已落：具名实参不当引用（`expr::named_argument`）、标识符 `+`/`-` 拼接
   （`expr::concatenation`）、绑定旗标剔除（`lexer::without_flags`）、**调用脊柱**（`calls.rs`+`builtins.rs` 注册表：
   `math.div/round/abs/ceil/floor/min/max/unit/is-unitless/percentage/compatible`，含 arity 与类型消息金样）、
   `Value::Bool` 与 Sass 十位有效数字、**1.4a `Calculation` 第一片**（`src/calculation.rs`：`Value::Calculation`，
   单项出壳 / 嵌套脱一层壳 / 否则原式保留）、**1.4d 前半：有理单位**（`src/unit.rs` 的 `Unit{num,den}` 是
   **保序**多重集 + 构造时同名抵消，`Value::Number(f64, Unit)`，`BinOp::{Mul,Div}` 与 `build()` 优先级；
   词法已把 `*` 提成 `Op`，`/` 只在调用参数内由 `expr::release_slashes` 变算子；复合单位不得离开 `calc( )`）与
   **1.4d 后半：calc 内容平展归约**（`src/brackets.rs` 一次 fold 出 `level`/`owner` 的文档序 run、调用折成
   一个词素；`src/calc.rs` 按层自底向上归约成摘要项 `Term{value,printed,precedence}`，**无树、无递归、
   无状态旗标**，看不懂就走 `Reading::Declined`；括号只在 join 时由优先级定一次）、**1.4d 收尾：调用作
   为算术操作数**（`operations::split` 在顶层算子处切 run（括号内不切），`expr::operand_expr` 读操作数，
   优先级改为 `tree/products/chain` 两趟 fold——`build()` 递归与 `Piece::Other` 已删；`math.div` 产出有理
   单位，`math.compatible` 比维度签名；复合单位不得裸打印由 `Value::is_plain` 在 `exec::evaluated` 出口把关）、
   **1.4d 第三段：已绑定计算参与运算**（`Value::Calculation` 存 `calc::Content{folded,printed,precedence}` 而非
   文本——存 `Option<Value>` 会构成无间接的循环类型；`Value::number()` 让内置函数与外部算术能取到它；
   `expr::protected` 改为上下文判据（括号内的 `/ * %` 不是 CSS 分隔符）；**乘法与除法共用 `Value::cross`，
   可换算对跳着分数线抵消时把因子带进幅值**）。
   **同单位 `+` 化简已按金样做**（`operator/{plus,minus,times,precedence}::simplified`）。
   **以上均不得重做/重新发明**。当前基线 hrx **847/11,533**（7.35%，**实质通过 682** + 空期望蒙对 165；
   A/B 记录 = `reports/hrx.db` 里的具名 run `binding-content`）；
   唯余 0′.5（共享态 `C::RcMut`/`C::RcCell`，现无可改之物，触发点在 P4 `@content` / P5 模块多播）。
   下一主战场：**1.5 色彩**。地形已量录在 tasks.md 1.5（期望值 ~880 条 / 期望错误 ~227 条），两条要点：
   **最小切口是 `core_functions/color/rgb/three_args/*`，不是 `values/colors/**`**（后者几乎全要颜色 `==`
   比较 + lab/lch/oklab 全空间，按本 change 自己的划分属 P8）；前置动作是 `builtins` **按域拆目录**
   （`builtins.rs` 已 436 行，颜色函数一接必越 500 红线）。再往下是 1.7 其余分派 → **P7 诊断消息**：
   语料里大量 `error` 用例缺的是**消息**而不是错误管道，现已积到单条级别（例：
   `math/unitless::error/wrong_name` 要 `Undefined function.`，即 1.4d 唯一未复现的候选行）。
架构结论**不得**凭记忆重推；已作废方向（逐行教程、`crates/docsizer`）不得重启——docsizer 已从 workspace
成员里删除（再引用会直接弄坏构建），其教程 change 与 `docs/rxrust-from-scratch/` 仅作历史。

## 硬性禁止（违反 = 任务失败）
1. **禁 Python/脚本**：不用 python3/pip，一律 Rust；测试用 `#[test]` 且放 `tests/`（`src/` 保持纯生产码，禁内联测试）。
2. **禁 `println!/eprintln!`**：一律 tracing 宏；跨函数/阶段的管道必须用 `tracing::span!` 记上下文与耗时（字段 `stage`/`module`/`id`/`elapsed_ms`），不得仅用单条 `event!`。
3. **单文件 ≤ 500 行**（源码、测试分别计），超出必拆。
4. **禁 GC/命令式思维**（详见「铁律」与「反回归自查」）。

## Git 规范
- 推送用 SSH：`git push github main`（remote `github` → `pauljohn21/qrass`）。只提交不推送时，`commit` 后等用户确认。

## 🔬 调试协议（强制）
禁止凭直觉猜根因；bug 修复须基于 tracing 证据链：① 在疑似路径入口/出口加 span ② 采集 ③ 引用具体 span+字段定位
④ 修复后清理临时 span。**不得再造临时探针文件**：本轮起诊断面已落地（tasks.md 9.0）——
`HRX_CASE=<key 子串>` 只跑一条用例（与 `HRX_RUN` 互斥：部分测量不得入库），`--features trace` 把现有 span
桥到 `opentelemetry-stdout` 直接打印（无 collector 无网络），入口是
`cargo run -p sasspile --features trace --example trace_pipeline`；声明级 `value` span 给
`module`/`id`/`raw_value`/`resolved`，`calculation read` 与 `call decision` 两条事件给**词素原文与切完的形状**
——“这条声明为何逐字输出”、“我的判据为何匹配不到”看这两行即可。三条约束：① 阶段 span 必须**住在观察者里**，
在 `subscribe` 里 `enter()` 的 span 一返回就关闭，`elapsed_ms` 量的就只是接线而不是流线；② `elapsed_ms` 用**带小数毫秒**，
整数 ms 会把小相报成 0 伪装成“瞬时”；③ `tracing` 宏体内不要写 `Value::Xxx` 路径——宏作用域里 `Value` 是
`tracing` 自己的 field trait，判定要在宏外算完再传进去。另：`tracing-opentelemetry` 只导 **span** 不导 event，
给一个工具加 trace 时必须先确认它真的建有 span。代码查询优先 CodeGraph。

## ⛔ 铁律：以 rxrust 为核心 = 用 Rust 所有权写程序
rxrust 不是包在命令式代码外的响应式库，而是 **Rust 所有权的一种良好表达**；顺它写可消掉大批 GC 问题（共享可变、对象图别名、边遍历边改、谁负责释放、空引用/生命周期）。**rxrust 本身就是引擎**，不是给状态机套的壳。

**思维顺序（治本，每次动手前默念）**：
1. 先只描述 **哪些 owned 值、沿哪条流、被算子怎么 move / 展开 / 丢弃**——让所有权与数据形状决定分解，而不是让"我要维护的可变状态"决定。
2. 一冒出"我需要维护一个跨元素的可变状态（栈 / 队列 / 缓冲 / 游标 / 万能 `Acc`）"→ **那是 GC 报警**：停下 **重造数据**（嵌套做进值里、展开=`flat_map` 对 owned 集合、选支=`filter`、一到多=`Subject`），**不是**给状态机包一层算子。`from_fn`+可变 `VecDeque`/`Vec<Frame>`、`scan_map(Acc)` 里 Acc 手搓跨元素状态——**都是穿了算子外衣的 GC 逃逸**。
3. 控制流若"非要有手动栈/缓冲/重放"才能表达 → 几乎一定是 **分解错了**，不是 rxrust 不够用；别改造/迁就 rxrust 来喂命令式结构。
4. **不许用结果（net/过测/分数）给 GC 结构背书**；思维正确性优先，宁可删了重来，也不留"算子外衣下的状态机"。

**绕过 rxrust 不便时先假设模型错**：想绕 `'static`/GAT/要 `box`/不能返回引用——正解是把状态改成被 move 的 owned 累加子、喂 owned 值进流（`from_iter` 给 owned 值、跨签名用 `box_it`），**不是** clone 凑数、不是退回命令式、不是加封装。

**程序即一条流**：from_iter/create/from_stream → scan(状态即 item)/scan_map(私藏 acc)/reduce_initial(owned 累加子) → flat_map 展开 / filter 选支 → collect → 公开入口唯一一次收口，**只用框架原生口子**：collect/reduce_initial + `into_future().await`（`into_future` 要求恰好一个值：空→`Empty`、多→`MultipleValues`，且它丢弃订阅句柄=不可取消）、`into_stream()`（只在 `Drop` 时取消）、或 await 订阅句柄（from_stream/timer/observe_on 等返回的 `TaskHandle` 本身就是 Future）。`std::sync::mpsc` **不是终点**，仅用于必须把 `Sender` 交给外部/跨线程的桥接；禁在异步核心外套 mpsc+block_on std 壳。状态=被流携带的 owned 累加子；错误=流内 `Result<T,SassError>`（`Err=Infallible`）；一到多才用 `Subject`/`behavior_subject`+`peek`/`publish().ref_count()`（rc.6 **无 `share()`**；`Subject` 不是共享可变的挡箭牌，且**回调内重入发射会 panic**，需 `delay(0)` 造异步边界）；深度是数据（栈在累加子里）不是调用栈。注：`Shared::from_stream` 单条保序，切勿对每条 item 各起一个 `from_stream`（并发→乱序）；并发额度满时 `merge_all` 会**丢弃仍在排队的内层**期间的发射（`sort` 无法恢复），要不丢就只用 `concat_map`/`concat_all`。`@if` 链作为 `scan_map` 累加子里 move 的值传递，不是每 item 独立可变标志。

**值与控制建模**：值建模成 `Expr`/`Value`，`eval(Expr, owned Env)`（`and`/`or`/`if` 惰性短路），从结构化 token 一次成型，禁"拼串再 char 扫"。嵌套/树必须流式平展成文档序 `Vec<Leaf>`（render=对 leaf 的 `map`、循环=`flat_map`、条件=`filter`），禁 `Vec<Rule>`/`children` 递归下降。`if/else if` 链、多守卫 `return`、`for/while/loop`、自造 `Stage/fail/drain` 框架层、std `.iter().map/fold/collect()` 当数据流替身——全禁；确不可等价表达 → 上报阻塞项，不退回 std。风格：链式 + 闭包(`FnMut`) + fluent builder；例外仅纯值 `Option/Result` 组合子与单个无 `else` 的 `if` guard。

## 🧾 反回归自查（动手前逐条 grep；命中 = 停手先重构根，再谈加功能）
- **树/递归**：`Vec<Rule>`、`children`/`.children`、`fn …->Vec<..>` 中转、render/eval 自调用 → 平展 `Vec<Leaf>`，`map/flat_map/filter`。
- **批量外壳**：`scan_map` 返 `Vec` + `flat_map(from_iter)`、把 body 收集后重放 → **每相写成真算子内核**
  （`struct Phase{source}` + `PhaseObserver{observer,state}` + `ObservableType`/`CoreObservable<C>`），
  在自己 `next` 里逐条 `observer.next(..)` 发 0..N 项；状态住 Observer、`Err` 保持泛型、不写死 `Rc`/`Arc`。
- **排序代替时机**：为输出顺序维护 `order`/序号 + `sort_by_key` → 先问规范里的**发射时刻**（实例：`sass-spec/css/style_rule`
  `declaration/interleaved/**` = 开子块冲刷父块待定声明段、闭块冲刷残余，一个块可出多条规则），改正后**删掉键**；
  `merge_all` 并发额度满会丢排队内层的发射，**排序救不回缺失元素**。纯 CPU 活包 `from_stream(stream::once(..))` = 假并发 → 串行或 `concat_map`/`concat_all`。
- **兜底当错误策略**：求值路径上的 `.ok()` / `unwrap_or_else(原文)` → `Err = SassError` 真通道（终点 `into_future()` 给 `Ok(Err(..))`）；
  只有**规范定义为透传**的 CSS 原生构造（calc 不可化简、custom property、未识别 at-rule）才原样保留。
- **命令式循环/分支**：行首 `for`/`while`/`loop`、`if/else if/else` 链、堆叠守卫 `return` → `fold`/`reduce_initial`/`scan_map`/`filter`。
- **名字位置当引用**（本轮三类，全在真通道打开后变成假错）：`@for $i from` 的头部、`f($hue: 180)` 的
  具名实参、`$a: b !default` 的旗标——都是**标签/绑定位**，去查环境就报出假 `Undefined variable`
  （实测 1,611 + 118 + 2 条）。接线时先问“这个名字是在引用还是在命名”；已落地的判据：
  `expr::named_argument`、`exec::head_pieces`（只认 `#{}`）、`lexer::without_flags`。
- **分类粒度不能超过词法**：要判“是不是厂商前缀 CSS 调用”、“这里 `&` 是不是父选择器”、“串里的 `$`
  是不是引用”，必须先在 `lexer.rs` 里把该构造**做成一个词素**（`-a-calc(`、`&`、带引号串）；否则
  任何字符串启发式都是错的（已三次实测推翻）。同理：行注释结尾的换行要是 `Space`而不是 `Word(" ")`，
  否则“首个非空白词素是不是 `Var`”这类词素级判定会静默失效。
- **拿字符串凑值**：先拼串再 `split_once` 拆、对串二次 char 扫描 → 结构化 `Vec<Tok>→Expr→eval(owned Env)→to_css`，只在无歧义时求值；选择器/属性名要是**结构化值**（`&` 组合不是 `str::replace`）。
- **GC/共享可变**：`&mut self` 被闭包共享、手写 `HashMap` 当单例总线、env 键 `$`/去`$` 混用 → owned 累加子、一到多才 `Subject`、键约定统一。
- **自造框架层**（`Stage/fail/drain/drain_one` 之类把流压回值的中转）→ 直接用 rxrust 组合子。
- **多播不可重入**：`Subject` 回调里再 `next/error/complete` 会 panic → 插 `delay(0)`；`Subject::is_closed()` 意思是“无订阅者”，不是“已终止”。
- **收口只用框架原生口子**：`collect`/`reduce_initial`+`into_future()`、`into_stream()`（仅 `Drop` 取消）、或 await `TaskHandle`；`std::sync::mpsc` 不是终点。
- 上述新形态已编成**机器闸**(P0′.6,`crates/sasslint/src/lib.rs`):`no-batch-shell`(同行 `scan_map(`+`Vec<` 或
  `flat_map(`+`from_iter(`)、`require-err-channel`(同行 `.ok()`+`eval`;或文件里有 `unwrap_or_else(` 却无
  `is_fatal` 判定)、`no-sort-for-order`(`sort_by_key`/`sort_by(`/`sort_unstable_by`/`.sort()`)、
  `require-slot-before-subscribe`(文件用了高阶/多源且 `.subscribe(` 却无 `SingleAssignment`)。
  闸口 = `cargo test -p sasslint` 的 `gate_sasspile_is_clean`(已去掉 `#[ignore]`,命中即红)与
  `cargo run -p sasslint --bin sasslint -- lint crates/sasspile/src`(命中 exit 1)。
  **它们是代理而非证明**:规则刻意放过一半签名的合法用法(`from_iter` 作源、词素解析的 `.ok()`、
  内核的 `source.subscribe(..)`)——`tests/lint.rs::legal_stream_shapes_are_not_flagged` 钉住这个下界;
  放宽规则或改名绕过仍属违规,应按 `reactive-stream-architecture` 的反例索引改数据分解。

### 🧪 验收读数也会骗人（工具级红线）
- `crates/ledger` 只存失败行；同名 run 重导已改为先 `DELETE FROM run_fail`（旧版会把“已修好”报成 phantom REGRESS）。
- 断言 `regress=0` 前 MUST 在**另一个空库**重做一次 A/B（`HRX_DB=<临时路径> HRX_RUN=<名> cargo test -p sasspile --test hrx`）；
  行为不变型重构改为两侧各入同一临时库后 `ledger diff` 证 **net=0**（快照文件已不存在，md5/`diff` 无对象可用）。
- “同名两结果完全相同”得到的 `regress=0` 不证明任何事； 行为变更 MUST 列出**哪几条用例**变了及其规范依据。
  “FAIL 变成另一种 FAIL”在 pass 读数上看不见 ⇒ MUST 另数无效错误：`ledger failures <run> "unexpected error"`
  （它同时匹配 key 与失败详情；这个查询曾因 `LIKE` 绑定 + 吞错而永远返回 0，现由
  `crates/ledger/tests/failures_query.rs` 钉住，当前基线读数 55）。
- **通过率必须同时报“实质通过”**：`tests/hrx.rs` 会输出 `pass_substantive`（期望 CSS 非空而通过）与
  `pass_empty_expectation`（期望输出为空，“啥也不输出”也能对）。当前 746 PASS = **581 实质 + 165 空期望**。
  实证：`directives/{if,for,while}` 簇的 39 条“PASS”**全部**是空期望 ⇒ `@if`/`@each`/`@for` 其实一行也没实现，
  而 `tasks.md` 3.2 却写着“已做”（从已删的 v1 流水线带过来的假状态，已更正）。**声称某特征能用时，
  MUST 看该簇的非空期望通过数，而不是看总 pass。**
- **用例 key MUST 唯一**：旧 `tests/hrx.rs` 用 `<basename>.hrx::<case>` 做 key，而语料在不同目录重名（两个
  `function.hrx`），11,486 条里实测 **1,135 个重复 key / 33 个同时挂着 PASS 与 FAIL** → 一个文件的通过会
  掩盖另一个的失败（一度把回归归到错误的文件上）。现在 key = 相对 `spec/` 的全路径，且 harness **有撞名断言**；
  旧制式的 run 与新 run **不得跨制式 diff**（`reports/hrx.db` 的 `case-key-scheme` note 已记明）。
- **“报了个错”≠“按金样报错”**：旧 harness 把任意 `Err` 当 `error` 用例通过，使 `FATAL` 翻转虚报 **+385**；
  逐条比对金样 `Error:` 首行后真值是 **+1**（384 条的期望消息是 `Expected identifier.`、单位错误、
  `DEPRECATION WARNING`——它们属 P1/P7 待实现的消息，不是错误管道）。批量新增/回退 MUST 抽样看**期望文本**，
  并区分“指标修正”与“真收益”。
- **A/B 记录只住 `reports/hrx.db` 的具名 run，且不再经任何中间文件**：需要入库时用
  `HRX_RUN=<增量名> cargo test -p sasspile --test hrx`，harness 直接写库（`HRX_DB=<路径>` 指向另一个库，
  供强制的空库复算用）；不设 `HRX_RUN` 时只量不写、不留任何产物。事后靠 `ledger runs` / `ledger diff <a> <b> [簇前缀]` /
  `ledger failures <run> [key-或-详情子串]` / `ledger passed <run> <簇前缀>` 追溯。
  **读数一律走 `cargo run -q -p ledger -- …`：既禁止 `sqlite3` 裸查记录库，也禁止在 shell 里用
  `sed|sort|comm|awk` 对 ledger 的输出重算集合差**（本轮两次违规：先手拼 SQL 反推 PASS 集，后又在
  `/tmp` 导两个 key 列表用 `sort|comm` 算簇级 A/B —— 后者正是本 crate 要消灭的 `reports/hrx-*.tsv`
  sprawl 换地方复发。**缺哪个读数就补哪个读数，而不是绕它**）。本轮据此补了两个命令，并把结论做成
  `pub fn failing/passing/diff_runs` 让测试直接断言（不再 grep 日志）：
  • `passed <run> <簇前缀>` —— 库**只存失败行**且**不记某 run 的测量范围**（in-scope 的 PASS 只登记 key、
    不长行），“这簇现在哪些是 PASS”无处可答；用“全部已知 key − 该 run 失败”去反推会把**从没测过的用例
    报成通过**（实测 `passed head-600-pathkey operators/` 返回 29 条，而那一版 harness 压根没扫 operators），
    故**簇前缀必填**、覆盖面由调用方按语料自证（`tests/passing_query.rs` 钉住）；
  • `diff <a> <b> [簇前缀]` —— 验收口径本来就是**按簇 scoped**（每簇自基线只要求不回退），无簇级读数就
    只能手拆；`Diff{fails_a,fails_b}` 是“簇内根本没用例可比”与“簇内 0 回归”的区分器
    （`tests/scoped_diff.rs` 钉住）。
  另：**`sasslint lint <dir>` 曾把 `lint` 当路径扫**（取 `argv[1]`），故文档化的 CLI 闸**永远报
  `count=0`**，同期 `gate_sasspile_is_clean` 却是红的 —— 永远通过的闸比没闸更坏，已修（吞动词、路径不存在
  即 exit 2）并由 `crates/sasslint/tests/cli_directory.rs` 钉住。
  MUST NOT 再写回“先导出 TSV 再 `ledger import`”的做法，也不得开第二个库当记录。
  **MUST NOT** 攒快照文件或第二个 sqlite（`crates/ledger` 的存在理由就是取代旧的 `reports/hrx-*.tsv` sprawl；
  本轮曾误把基线存成 `target/baselines/*.tsv`，15 个文件 / 27 MB，已全部入库并删除）。
  为验收新建的空库（`--db=<临时路径>`）只用于复算，**不作为记录留存**；`/tmp` 会被系统清理。
- **任何“为 0 / 全通过”的统计读数 MUST 先过一条阳性对照**（已知应该为真的用例）。本轮实错：一个
  `substr` 切片多吞一个引号字符，使“仅差空白/仅差大小写”都报 0，差点得出“无 cheap win”的结论。同类第二次：
  `ledger passed <run> operators/` 对**根本没扫过该簇的** run 报 29 条“通过”（库不记测量范围）。新增读数
  自己也要带阳性对照与“匹配为空”两条测试，否则它只是另一个会静默失效的量具。
- **链式量具命令先确认 `pwd`**：shell 会话会复用，cwd 会漂到子模块里（本轮实际发生：漂进 `sass-spec/spec`）。
  在这种目录下用相对路径读写会静默写错地方或读不到文件，而**用相对路径去读语料恰好仍然成立**，
  所以错误不会报错只会被隐藏——路径一律用绝对路径或先 `cd` 到仓根。
- **绝不在命令链里放可交互程序**（`python3 -v` 会拉起 REPL 吞掉整个终端，后续命令全被当成 Python 语句
  吃掉）；发生时的恢复法：另开一个新终端跑命令（旧终端里的垃圾输入会随它一起报废），**不要**用补引号
  的方式去“救”——那只会制造新的未闭合引号，本轮连错两次。
- **量具覆盖面 MUST 等于计划引用的簇**：`tests/hrx.rs` 的 `SPEC_DIRS` 长期缺 `operators`/`parser`/`variables`
  （共 79 例未计量），而任务 1.6 自己写着“经 `operators/**` 金样验”——引用了却扫不到，等于用不存在的
  判据宣称 regress=0。目录清单以 `sasspile-conformance` 的 hrx harness 契约为准；扩面算范围变更，验收判据是
  **旧覆盖集内不得有 PASS→FAIL**，不是总数上涨（扩面后基线 600 → 616，总数 11,533）。

## 🔌 接线纪律（新功能）
新算子/函数先在 `expr.rs`/`value.rs` 做成 **pub 独立件 + 金样单测** 钉死，**不接 `compile`**；接线时只在无歧义处触发：含 `/ * %` 的值跳过（护 `font:12px/1.5`、`grid:1/2` 等 CSS 斜杠分隔符），函数走**白名单** `call_builtin`，未知名 `eval→Err→` 原样兜底（`url/calc/min/max` 等 CSS 原生函数永不当 Sass 内置求值）；会动全局 truthiness 的比较/布尔单列一步、先护选择器（`a > b` combinator 不得当比较）。每步 `ledger diff` 确认 **`regress=0`** 才提交，否则回退。

## ✅ 验收 / 参照
项目 = clean-room `crates/sasspile`（配 `ledger` A/B 库、`sasslint` 铁律审核）；`crates/qrass` 已删除，**不得**以其代码为模板。行为**唯一**参照 = 本地 `sass-spec/*.hrx`（input→output.css/error）+ `element-plus`，**严禁**参照 dart-sass 源码/文档推断行为。每步跑 hrx 快照 + `ledger diff <base> <run>` 守 **`regress=0`**。
