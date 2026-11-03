---
trigger: always_on
alwaysApply: true
---

## 技术栈与语言
- Rust edition 2024，toolchain 1.99。所有对话用中文，代码/命令/文件名/注释保持 English。
- 以 **rxrust 为核心**（见「铁律」）。版本钉 `=1.0.0-rc.6`。

## 🧭 新会话起手（先读这几处再写码，否则必返工）
1. `docs/rxrust-source-analysis/`（rc.6 逐行取证，01–09；**09** = 取消/锁/终止投递次序不变量 + “库自带注释与实现不一致”清单）；
2. `.qoder/skills/rxrust-reactive/SKILL.md`（动手前“先选动词”清单 + 本仓库踩过的 GC 形态）里的
   「Single sources already fixed here」表——已定死的单一真源清单（本文件不再复述条目与条数，
   以免变成第二份真源；表就是唯一入处，写 helper 前查它）。
3. `sasspile-start` skill（已落地）：起手四步必须实跑命令并**逐字复述读数**（`ledger runs`、
   意外错误计数、`sasslint` 命中数、`openspec status`）；细则在它自己的 `reference/` 三份里。设计文档
   `docs/superpowers/specs/2026-10-28-cross-session-anti-gc-regression-design.md` 的 S1+S2 已实施（两条新闸
   +`defence_fitness` 体检+子代理+本文件瘦身），**但它的“冷启动三次实测”验收尚未跑** —— 见同一
   change 的 plan 末尾「实施结果与遗留」。两条纪律先从纪律层面生效：
   **读数只能来自产生它的那条命令**；**陈旧摘要/旧 dump 的记忆不是证据**（本轮 10 次“旧断言前提变了”同一成因）。

## 📌 基线（唯一副本；别处只准放指针）
   已得结论**不得凭记忆重推，也不得重做**（四个内核、真 `Err` 通道、结构化选择器/属性名、四条铁律机器闸、
   色彩三族与 math 域的已定定律均已成文，见 `state-of-the-port.md`）。当前基线 hrx **2657/11,533**（23.0%，**实质通过 2492** + 空期望蒙对 165，skipped 1264；
   A/B 记录 = `reports/hrx.db` 里的具名 run `missing-rectangular-v2`（`missing-rectangular` 是审查前那一树的读数，两者全等；
   2026-11-03：`none` 缺通道在**索引对齐族**（rgb 系 + `xyz`/`xyz-d50`）之间
   按位置传播，**`regress=0 net=44`**（2613→2657）；`ledger shift` 定侧：`changed=169 closer=0 farther=0 error_expectation=0 shape_of_text=169`
   —— 169 行**全部落在 shape 类**。注意这两个 0 的含义（量具源码 `shift_impl.rs` 里 shape 与 distance 是互斥分支）：
   对这些行它**没算**距离，所以“closer=0 farther=0”是**未测**不是“测得没动”（不得拿它当“没动任何数字”的证据）；
   无效错误仍 19，空库复算同读数）。
   三条律各自有金样：矩形族按位传播且缺者仍`0`代入算术（`to_space/xyz_d50/xyz.hrx::missing/x` 得 `color(xyz none 0.208311512 0.395008248)`，
   后两个数字与传播前已逐字相同的两个一致）、**任一侧为派生族**时不传播而代以 `0`（绿行只来自派生**源**：
   `to_space/lab/srgb.hrx::missing/lightness` 打三个数；派生**目标**方向仍红在第十位，属 9.14 的天花板）、
   全缺则全缺但 **`hsl` 形体例外**（`to_space/srgb/lab.hrx::missing/all` → `lab(none none none)`，
   `to_space/srgb/hsl.hrx::missing/all` → `hsl(0, 0%, 0%)`）。守卫 `tests/color_missing_channels.rs`。
   **未做（不得当作已交付，也不能说成“只剩两条”）**：`missing/` 仍红 869 = `to_space/` 下 791 + 其余路径 78；
   前者按桶看是四块：源为 legacy 形式 230 行（要 `color::Channel{value,percent}` 能携带“缺”，值模型改动）、
   目标为 `hwb`/`rgb`/`hsl` 形体 83 行（**其中 `hwb` 目标是有裸金样的**：`to_space/hwb/hwb.hrx` 整件不用 mixin，
   `::missing/non_hue` 写 `hwb(10deg none none)`、`::in_range` 却写 `hsl(0, 55.5555555556%, 45%)`，
   而 `transfer.rs` 现在根本不接 `hwb` 目标）、派生↔派生跨族 36 行、以及索引对齐族内因 primaries 未校准而数字本身就错的其余行；
   **polar 对（lab↔lch、oklab↔oklch）22 行里有 20 行合同一条候选律**（光按位携带；派生通道仅在其依赖全缺时才 `none`），
   只剩 `to_space/lch/lab.hrx::missing/{lightness,non_hue}` 两行破律（oklch 同形反而合律）——所以破的是 `lab` 自己，
   不是“三行两两互斥”；未推出前不落。
   其前一批 run `adaptation-white-final`（2026-11-03：白点适应表**从语料解出** + D50 参考白改为**导出**，
   合计 **`regress=0 net=115`**，无效错误 **19 → 19**，空库复算同读数）。两刀：`adaptation-solved`（`regress=0 net=62`）——`color/matrix.rs`
   的 `BRADFORD` 原是 CSS Color 4 散文的 7 位抄表，语料算的不是它（抄表行和 `0.9957009 0.9993688 1.3289869`，金样 `0.9956342097 1.0026671299 1.3221722918`，
   最差单项 `6.9e-3`），现由 `cargo run -q -p sasspile --example calibrate_space` **从磁盘读 `to_space/xyz_d50/xyz.hrx`** 解出（对未参与求解的行残差 `5.0e-11`
   ＝语料第十位小数的一半步），守卫 `tests/color_adaptation.rs`；`white-derived`（`regress=0 net=53`）——手抄的 `D50` 字面量与适应表实际落到的三元组差 `8.3e-10`，
   被 `xyz_to_lab` 的 `500·(fx−fy)` 放大成 `1.4e-7` 色度，改为 `d50()` 导出后 `to_space/srgb/lab.hrx::white` 从 `lab(100.0000000018% 0.0000001274 …)` 变成金样的
   `lab(100% 0 0)`（**中性自此是构造性事实**，不再依赖第二个白字面量）。**`changed=841` 不得说成进步**：新量具
   `cargo run -q -p ledger --bin shift -- <base> <run>` 把它分为 `closer=555 farther=101 tie=0 error_expectation=0 shape_of_text=185`，并**具名**后两类；
   `farther=101` 中 96 条的某一侧带着尚未校准的 primaries（`a98_rgb`/`prophoto_rgb`/`rec2020`/`display_p3*`：旧抄表的两个错误原本部分抵消），其余 5 条
   具名在 `tasks.md` 9.14。**本批的精度天花板（不得再猜）**：表来在十位小数 ⇒ 每元素 `~5e-11`，由它反解的一侧仍有三行差最后一名（`to_space/xyz/xyz_d50.hrx::gray`、
   `…::out_of_range/far`、`to_space/srgb/xyz_d50.hrx::white`）——这三行**故意不入守卫**。当时量到的“只差一个能力”已由本批头部的
   `missing-rectangular` 落下索引对齐族那一刀（剩下的按本节上面的四个桶分：legacy 源缺通道、目标为 legacy 形体、
   跨族派生、polar 破律的两行）；
   当时的金样根据仍成立（`to_space/hsl/srgb.hrx::missing/all` → `color(srgb none none none)`、
   `to_space/srgb/lab.hrx::missing/all` → `lab(none none none)`、`to_space/hsl/hsl.hrx::missing/hue` → `hsl(none 20% 30%)`），
   而前置只完成了一半：`InSpace.channels` 早就是 `[Option<f64>;3]`，`color::Channel{value,percent}` 还不是。其前一批 run `calc-constants-final`（2026-10-30：CSS 的**计算常数**在壳内读数
   （`calc(pi)` → `3.1415926536`、`calc(e * 2)` → `5.4365636569`），共 **+14**（`regress=0 changed=0`、无效错误仍 19），
   守卫在 `tests/calculation.rs::calculation_constants_are_read_inside_the_shell`（已做阳性对照：撤掉判据即红）；
   本批做了一个**不是律的选择**：非有限数（`infinity`/`NaN`）留在壳内。两个方向都量过：放开则
   `regress=7 net=23`（已现算订正：旧文写的 `net=37` 对不上库里任何一条读数；7 行颜色把
   `color(srgb calc(infinity) 0 0)` 答成裸 `infinity`），保守则 `regress=0 net=14`
   但把另一侧 23 行留在基线红态（`diff calc-constants calc-constants-final` 逐字要求夹紧：`color/degenerate::arg1/nan`
   → `color(srgb 0 0 0)`、`out_of_gamut::hue/degenerate/positive_infinity` → `hsl(0, 100%, 50%)`）——**真正的判据在
   颜色构造子的夹紧律**，不在壳层谓词，未推导前不得当律写（逐条叙述住在 `calculation::collapsible` 的 doc）；
   同一律在颜色打印层仍缺席：`to_space/hwb/hsl::white` 得 `hsl(0, infinity%, 100%)` 而金样是 `hsl(0, 0%, 100%)`；
   常数簇另有 5 行卡在能力上：2 行要 `==` 对算出的数、3 行要 `meta.type-of`）；其前一批 run
   `oklab-solved-polar-v2`（与 `oklab-solved-polar` 读数全等，
   后者是 `unwrap_or_else` 改显式 `match` 前的入库；2026-10-30：cone 表**从语料解出**并回到
   XYZ 域、oklab↔oklch / lab↔lch 同族不再走矩阵往返，**+6**（`regress=0`）。上一 run `oklab-linear-route` 把
   正方向改走线性 sRGB —— **那条论断是错的**（域从来没错，错的是抄来的数字；该 detour 把白行推到
   `101.9416634099%`，比原路线更远），数字与结论已由 `oklab-solved-polar` 取代；那一批靠逆阵现算拿到的
   +13 条 oklab↔oklch 往返行仍然成立，由 `tests/color_space_round_trip.rs` 钉住）；其前一批 `space-scale-once`：`lab()/lch()/oklab()/oklch()` 函数式构造子
   + 把「颜色是不是自带空间」从**两个字段**收成**一个判据** `Representation::InSpace`，共 **+180**（`regress=0`）。
   `changed=816` **不得说成进步**：其中 oklab/oklch 族因换算路线未校准而把逐字文本变成了错数字
   （白点不变量已近得多但仍不等：`to_space/srgb/oklab.hrx::white` 得 `oklab(100.000000148% …)` 而金样是
   `oklab(100% 0 0)`；残量 1e-9 来自 `OKLAB_LAB` 的十位数字，它限制了 cone 拟合的精度；本批两条中性行**更远离**金样（上一批是 `101.8011897979%`），所以 `changed=545`
   里只有 13 条转绿、其余是换了个错数字；cone 表已改由语料行最小二乘解出（六行全拟合到 cone 1.2e-8 /
   通道 7.6e-8，白点三锥相等到 1.4e-7），不再是我抄的三行和 1.3097/0.9984/0.9808 那张；
   `to_space/lab/srgb.hrx::middle` 差 ~1.5e-3）——**开放缺陷，方法与逐条数字在
   `tasks.md` 9.11 与 `color/matrix.rs` 的常量文档**；另有数行期望输出是
   `color-mix(in oklab, …)`，本编译器根本发不出（与矩阵无关的独立能力缺口）；无效错误 17 → **19**
   （新增两条具名行 `change/unusual_numbers::{infinity,negative_infinity}/polar`，需 `calc()` 求值与 Color-4 `$space` 写入）。
   其前史：色彩空间模型 + `color()` 构造 + `color.to-space` 共 **+430**（`regress=0`）；`changed=1431` **不得笼统说成“离金样更近”**：
   审计实测其中含两类**我的缺陷**——identity 转换走矩阵往返而发明数字（已修），以及 15 条
   `error/non_legacy` 把空间色答成 `0%`/`1`/`rgb(85, 85, 85)`（`Color` 同时携 `space` 与
   `Representation::Legacy`，未守卫的读路径拿空字节三元组作答——**开放缺陷**，逐条在 `tasks.md` 9.9）；
   色度行仍差在 ~0.1 量级（矩阵常数只有 7–8 位，金样打 10 位）。其前史：审查驱动的五批修复 `audit-closure`
   **一条 pass 也没增加**，只消缺陷与重复真源；其后的 `@for` 一到多扇出让 `directives/for/for::to_scope`
   从 unexpected error 变实质通过（+1，无效错误 18→17），逐批读数与无金样律见 `tasks.md` 9.6/9.7/9.8）——9.8 那批把 `@for` 的边界交回结构相（`structure::MsgAcc` 发
   一个 owned `Event::For`，`Exec` 侧缓冲/游标/重放四符号归零），**同样一条 pass 也没增加**
   （`regress=0 net=0 changed=0`）。残差是 design.md:88 的 `flat_map` **未做**，而不得把它说成框架限制：
      动词在 rc.6 里存在（`flat_map` = `Map` + `MergeAll`，内层自带 `Context`；保序用 `concat_map`），缺的是把
      `State` 从观察者字段做成流携带的累加子这个**选型**（即本文件唯余的 0′.5）。
   **diff 的 base 只准用本节点名的当前 run**，
   库里其余 run（`ledger runs` 里除它之外的每一个，含上一基线名与已回退批次的作 废读数）一律不得当 base；无效错误长期基线 53 → **20** → **18** → **17** → **19**
   （那 33 行全部是 `math.$pi` 这类带命名空间的引用被当成未绑定变量报成 `Undefined variable.`，
   本轮把它做成词素后消失；曾因定长 arity 把
   Microsoft-filter 重载报成致命错而变成 55，靠 `ledger failures … --count` 拦下）。
   唯余 0′.5（共享态 `C::RcMut`/`C::RcCell`，现无可改之物，触发点在 P4 `@content` / P5 模块多播）。

**三条不可协商（过程门禁；技术禁令见下面「硬性禁止」，不在此重复）**
1. 动手前必过 `sasspile-start` 的起手四步，并复述它打出的读数（复述不上 = 没读）。
2. 批次后必跑 `sasslint`（要 `count=0`）、`ledger changed <base> <run>`（接住 `regress`/`net`
   看不见的 FAIL→FAIL 形状变化）与 `gc-auditor`（finding 必须带 `file:line`）。
3. 不得放松闸、不得改名换形状绕闸、不得用"先记 TODO"顶替修复。

## 硬性禁止（违反 = 任务失败）
1. **禁 Python/脚本**：不用 python3/pip，一律 Rust；测试用 `#[test]` 且放 `tests/`（`src/` 保持纯生产码，禁内联测试）。
2. **禁 `println!/eprintln!`**：一律 tracing 宏；跨函数/阶段的管道必须用 `tracing::span!` 记上下文与耗时（字段 `stage`/`module`/`id`/`elapsed_ms`），不得仅用单条 `event!`。
3. **单文件 ≤ 500 行**（源码、测试分别计），超出必拆。
4. **禁 GC/命令式思维**（细则与反例索引见「铁律」与
   `.qoder/skills/sasspile-start/reference/gc-counterexamples.md`）。

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


## 📦 已迁出本节的内容（指针，不再在本文件复述）
- 反例索引（动手前逐条 grep、命中即回根上重构）：`.qoder/skills/sasspile-start/reference/gc-counterexamples.md`
- 工具级红线（验收读数也会骗人 / A-B 记录 / 量具覆盖面）：`.qoder/skills/sasspile-start/reference/tooling-redlines.md`
- 移植现状长叙述（每相/每族做到哪、为何这么判）：`.qoder/skills/sasspile-start/reference/state-of-the-port.md`
  —— 逐条实测证据在 `openspec/changes/redesign-sasspile-rxrust-shared-stream/tasks.md`，
  架构决策在其同目录 `design.md`，A/B 读数在 `reports/hrx.db` 的具名 run。
## 🔌 接线纪律（新功能）
新算子/函数先在 `expr.rs`/`value.rs` 做成 **pub 独立件 + 金样单测** 钉死，**不接 `compile`**；接线时只在无歧义处触发：含 `/ * %` 的值跳过（护 `font:12px/1.5`、`grid:1/2` 等 CSS 斜杠分隔符），函数走**白名单** `call_builtin`，未知名 `eval→Err→` 原样兜底（`url/calc/min/max` 等 CSS 原生函数永不当 Sass 内置求值）；会动全局 truthiness 的比较/布尔单列一步、先护选择器（`a > b` combinator 不得当比较）。每步 `ledger diff` 确认 **`regress=0`** 才提交，否则回退。

## ✅ 验收 / 参照
项目 = clean-room `crates/sasspile`（配 `ledger` A/B 库、`sasslint` 铁律审核）；`crates/qrass` 已删除，**不得**以其代码为模板。行为**唯一**参照 = 本地 `sass-spec/*.hrx`（input→output.css/error）+ `element-plus`，**严禁**参照 dart-sass 源码/文档推断行为。每步跑 hrx 快照 + `ledger diff <base> <run>` 守 **`regress=0`**，**并再跑 `ledger changed <base> <run>`** 逐条定性两侧都失败但详情变了的行（错值与发明错误只在这一条读数里现形）。
