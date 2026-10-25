# Tasks — 按 design.md 的 P0–P8 阶段(依赖序;每阶段经唯一出口 `compile` 验 `regress=0`)

> 阶段可跨多次提交推进;每阶段以对应 `sass-spec` 目录簇 hrx 经 `compile` 跑出 `regress=0` + `sasslint` 0 为门。
> "已做" = 本 change 之前的探索性未提交工作中已完成者(见 design「现状处置」)。
>
> **regress 口径(阶段式必需)**:`ledger diff` 的 `regress=0` **按目录簇 scoped 判定**——新阶段刚铺开、
> 尚未实现的域(如 P1 初期 `values/calculation/**` 全 fail)**不计入** regress;仅要求**已过某簇在其后续
> 变更中不回退**。即:每簇维护自己基线,阶段只在自己负责的簇上求 `regress=0` 且单调上升。
>
> **语料驱动排序(见 design「element-plus 真实语料分析」)**:element-plus 端到端主干 =
> **P1(map/插值/单位)+ P3 → P4(mixin/function)→ P5(模块 `@use as *`)→ P7(custom property)**。
> **P6(@extend,真实仅 5 次)、P1.7 legacy 全局裸名、`@while`/`@supports` 对 element-plus
> 非阻塞 → 后置**。custom property 内 `v-bind()`(Vue 构建期)不纳入 sasspile。
>
> **BEM 是 EP 主干(非阻塞项修正)**:element-plus 靠 `b()/e()/m()/when()` mixin DSL 生成 BEM,
> 硬前置 = **`!global` 环境态(3.6)+ mixin `@content` 携选择器上下文(4.1)+ `@use … as *`/`@forward`
> 链(5.1/5.2)+ 选择器插值(2.1)**。⇒ **P3.6/P4/P5 是编译 element-plus 的硬前置;`@forward` 不是后置项**。
> 真正可后置:仅 **P6 @extend** 与 **P1.7 legacy 裸名**。

## P0′ — rxrust 原生重构（v2 口径落码；design §1/§5/§6 的三条改正）

> 输入 = `sass-spec` 语料 + `element-plus`，**不以现有实现为设计输入**。本阶段先把“非框架模式”的
> 根子拆掉，再往下堆语言面；每子项完成后跑 scoped `regress=0`（当前整库基线 **pass=600/11,486**，量具 =
> 相对 `spec/` 全路径 key + 错误消息金样，见 0′.3 末“可复核数据”）。

- [x] 0′.1 **词法/结构相内核化**：`Lexer<S>`/`Structurize<S>` 写成三件套（`struct{source}`+`Observer` 持状态+
      `ObservableType`/`CoreObservable`），在自己 `next` 里逐条发射 0..N 项；删除
      `scan_map(acc)→Vec<Tok>→flat_map(from_iter)` 批量外壳（现 `lib.rs:57-61`）。
      **已完成**：新增 `src/phases.rs`（两内核，`Err` 泛型不锁错误通道）；`LexAcc::feed`/`MsgAcc::feed`
      改为向 `Emit` sink 逐条发射，累加子里的 `out: Vec<Tok>`/`events: Vec<Event>` 全拆（含 `events.clone()`）；
      `compile` 链改用 `.transform(|source| Lexer { source })`。门：build/clippy `-D warnings`/test 39 全过、
      `sasslint count=0`、hrx 快照与基线 **md5 一致**（`ledger diff regress=0 net=0`）。
- [x] 0′.2 **Exec 相内核化**：`scopes`/`env`/`next`/`emitted` 住在 `ExecObserver`（随订阅诞生）；上游句柄用
      `SingleAssignment::channel()` 先建槽后订阅；“终止只投一次”用 `Option::take()`/`C::RcCell` CAS，
      禁裸标志；上游自然终止时只 retire 不 `unsubscribe`。
      **已完成**：`Exec<S>`+`ExecObserver{observer, state}` 取代 `scan(State) + filter_map(into_rule)`，`State::step`
      改 `&mut self` 就地推进 → **每事件不再 `clone` 整个 `State`**（旧形态为了取一条规则而复制全部开块+env），
      `emitted` 字段随之消失。`SingleAssignment`/CAS 部分本相不适用（单源直递 `Unsub`，同 `map`），
      高阶/多源处（P5 模块并发、`@content` 子流）才是它们的落地点。
- [x] 0′.4 **保序改动词**：移除假并发与 `collect`+`sort_by_key`（原 `lib.rs:64-77`）；序列化改串行内核
      `Render<S>`（`StyleRule → String`，1:1）；`order` 键与 `State::next` 计数器**删除**。
      **规范驱动的行为更正**（`sass-spec/css/style_rule` `declaration/interleaved/**`）：发射时机 =
      “开子块先冲刷父块待定声明段 + 闭块冲刷残余”（`State::settle_top`/`settled`），而不是“一个块一条规则
      再按开启位置排序”。旧写法用 `order`+`sort` 近似它，所以 **`around_style_rule`（一块拆两段）永远过不了**。
      门：干净 DB 上 `ledger diff pre after` → **`regress=0 net=3`**（595 → 598，+3 全是上述 interleaved 用例）；
      新增 `tests/interleaved.rs` 把三条规范期望钉成断言；`tests/order.rs` 的旧“并发+order 键”注释按新现实重写。
      **附带修一个工具缺陷**：`ledger` 重导同名 run 时不清 `run_fail` 旧行 → 已修好的用例会报 phantom REGRESS
      （一度使我误判为 `regress=1`）；现 `import_rows` 先 `DELETE FROM run_fail WHERE run_id`，并加
      `crates/ledger/tests/reimport.rs` 两例钉住（该缺陷是旧工具可观测的假数据，不属本阶段验收项）。
- [x] 0′.3 **真 `Err = SassError` 通道**：拆为三步，**已做过一次并因证据不够而回退**（下记实测数据不得被遗忘）。
  0′.3c 起 `UndefinedVariable` **已在 `FATAL` 中**，量具同时换成“全路径 key + 错误消息金样”，两笔历史读数已作废。
  - **试验结果（已 `git checkout` 回退）**：把 `ErrorKind::UndefinedVariable` 接成致命错误后，hrx
    **598 → 984**（+389 全部是规范期望为错误的用例），但同时 **回退 60 条**。因不能解释全部回退项而放弃，
    不得拿 60 条真通过换未评估的收益。
  - **测得的两条硬约束（回退项拆解而来）**：
    1. **字面量上下文不得做变量查找** —— 回退项含 `css/selector/escaping.hrx::dollar_char`（`"$"`）、
       `css/functions/special/prefixed/lowercase.hrx::*/script_like`（IE `progid:…($d=…)`）、
       `css/functions/var.hrx::sass_function/two_argument/doesnt_use_function_if_case_doesnt_match`（大写
       `VAR($args)` 不当 Sass 函数）。⇒ 错误只能来自**我们判定为 Sass 表达式**的求值；
       `interp_text`（选择器/属性名）路径上的变量查找口径过宽，需先限定为“仅 `#{}` 内”。
    2. **剩下的 ≈47 条全在缩进语法用例**（`whitespace.hrx`/`comment.hrx` 的 `*/scss` 键，错在 `"$i"`）——
       根因是 `@for $i` 绑定在缩进语法下未成立（harness 尚未支持 `--indented`，见 0.3），不是错误策略本身。
  - [x] **0′.3a（已合入）管道**：`SassError` 带 `ErrorKind`；`ExecObserver` 改持 `Option<O>` 并用
    `take()` 保证终止恰好一次（上游 `Infallible`，`error` 分支不可达）；`State::step` 返回
    `Result<.., SassError>`；终点不再重新贴标签（删 `map_err(|_| SassError::new("reduce"))`）。
    致命集合 `const FATAL: &[ErrorKind] = &[]` 暂时为空（0′.3c 起已翻转），所以行为**逐字节不变**：
    hrx 快照与基线 **md5 相同**（`2959a889…`，598/598），空库 `ledger diff regress=0 net=0`，
    0 测试失败、clippy `-D warnings` 无告警、`sasslint count=0`。
    新增 `tests/fallback.rs` 钉住两类事实：规范要求的字面量透传（`progid:` 值、选择器 `\$`）不得查变量；
    以及“未绑定变量暂逐字保留”的 interim 策略（P0′.3b 落地时预期改这条）。
  - [x] **0′.3b-1 头部只认 `#{}`**（已合入）：`interp_text` 不再把整个头 run 当表达式求值，而是逐 token 走
      `head_piece`：只有 `Tok::Interp` 求值，其余一律 `token_text` 原样。因为头未必是选择器——
      `@for $i from …` 的 `$i` 是**绑定目标**，旧写法把它去环境里查，制造出假“Undefined variable”。
      实测：598 → **599**，`ledger diff regress=0 net=1`；且翻转 `FATAL` 后原先 60 条回退里的 ≈47 条
      （`directives/for/whitespace.hrx`、`comment.hrx` 的 `*/scss` 键）不再回退。跳过的“意外通过”：
      `core_functions/string/unique_id.hrx::is_unique` 是因为我们从“吐垃圾”变成“什么都不吐”而撞上了
      空期望输出（它要 `@use`/`@for`/`map.*`/`@error`，均未实现），**不计作真收益**。
  - **0′.3b-2 未做（已三次实测推翻猜测）——special call 分类器**：`FATAL` 翻转后回退项从 60 降到 4，但任何
    基于“形状”的判据都错。语料里的**判别文件**是（本轮没读到、导致两次错方向猜测）：
    • `css/functions/not_special.hrx`：“`a {b: -c-type(1 + 1)}` → `-c-type(2)`”——**带厂商前缀的未知函数
      不是 special**，参数照做 SassScript 算术（“名字含 `-` 即原始 CSS”错）；
    • `css/functions/special/unprefixed.hrx` / `special/prefixed/*`：special 名单是 `url`/`calc`/
      `element`/`expression`/`progid:` 这类**具名函数**（去前缀后的基名）；
    • `css/functions/special_variable.hrx` 与 `values/calculation/calc/no_operator.hrx`：special 函数的参数里
      **变量与插值仍解析**（`$a: foobar; b {c: calc($a)}` → `calc(foobar)`；`calc(#{1px + 2px})` → `calc(3px)`），
      只是 `+ -` 不当算术——“名单命中就整个不解析”错。
    实测数据：用 `SPECIAL_CALLS=[calc,element,expression]`+`progid:` 判定并翻转 `FATAL` → pass **599→979**，
    但 `no_operator.hrx` 两条回退（因把 `calc` 当成不解析）；用“名字以 `-` 开头”→ 踩坏 `min`/`clamp`/`url`
    （978→947、3 个单测红）。因此**未落**：已仅保留中性部分（词法让 `-a-calc(` 成为一个词素，
    `regress=0 net=0`，已推），分类器与 `FATAL` 翻转已 `git checkout` 退回。
  - [x] **0′.3b-2 special 调用分类器（已合入，暂时休眠）**：`expr::special_call` + `SPECIAL_CALLS`
      按**去厂商前缀后的基名**匹配 `url`/`calc`/`element`/`expression`（另认 `progid:` 这类命名空间形）；
      词法已保证 `-a-calc(` 是一个词素。注意它只在 `FATAL` 非空时才参与判定，所以现在行为中性（实测
      与 599 基线集合完全相等：新增=0 丢失=0）。
  - [x] **0′.3b-3 翻转的实测结果（已落地，但先前两笔读数全被证明不可信）**：同一翻转在三种量具下的读数 ——
    ① 旧量具（basename key + “有错即算 error 用例通过”）：**599 → 984**，报“新增 385 / 丢失 3”；
    ② key 换成相对 `spec/` 全路径（0.5）后同样两笔：**+385 / −3** 复现，但三条丢失其实是
      `css/function.hrx::*/sass_script`（此前错记为 `css/plain/function.hrx`——两个同名文件被撞成一个 key，
      而 `css/plain/function.hrx::*` 一直因 `got ""` 失败，缺的是 P5 的 `@use "plain"` 加载器，不是透传）；
    ③ 再加“错误消息必须等于金样 `Error:` 首行”（0.4 的一部分）：**599 → 600，regress=0，真收益 1 条**
      （`core_functions/meta/call::error/if_args` 期望 `Error: Undefined variable.`）。
    **385 里有 384 条是假通过**：逐条取金样 `error` 首行比对，只有 1 条真写 `Undefined variable`，其余是
    `Expected identifier.`(23)、`DEPRECATION WARNING [global-builtin]`(18)、`$red: Expected 0.5px to have
    unit "%" or no units.`(42…) 等我们尚未产生的消息 ⇒ “报了个错”被旧量具当成“按金样报错”。
    结论：翻转本身正确（铁律要求真通道），但它今天在**真**指标上只值 +1；已按 ③ 落地。
  - [x] **0′.3b-4 plain CSS at-rule 体透传（已合入）**：`exec::plain_css_body` 认**已求值后**的头部文本
    `@` + `function`（大小写不限）+ `--` 开头的自定义 ident（`@function --a()`／`@FUNCTION --a()`／
    `@#{function} --a()`／`--a(--b <color>)`／`returns <ident>`），命中的 `Scope` 标 `plain_css`，
    嵌套块继承。该位置只**抑制致命性**、不改求值：`result: $b` 留原文，而同体 `#{1 + 1}` 仍出 `2`
    （`css/function.hrx::lowercase/result/{sass_script,interpolation}`）。`@mixin --a` **不纳入**：语料判它
    报错（`css/mixin.hrx::error/css/mixin`），不是透传体。新增 `tests/fallback.rs::
    plain_css_at_rule_body_keeps_an_unbound_variable_as_text` 钉住这条边界。
  - [x] **0.5 harness 用例 key 相撞（已修）**：key 改为**相对 `spec/` 的完整路径**（去 `.hrx`）+ 用例目录，
      并在 harness 里加**撞名断言**（出现重复 key 即整轮判失败）。实测旧方案在 11486 条里有 **1135 个重复
      key**，其中 **33 个 key 同时挂着一条 PASS 和一条 FAIL**（这正是“丢 3 条”被归错文件的机制）；旧 run 落库时
      10891 条失败被压成 7138 个 key。历史 run 与新 run 的 key 不同构，**不得跨制式 diff**（已在 `reports/hrx.db`
      的 `case-key-scheme` note 里记明，新基线 `target/baselines/head-600-pathkey.tsv`）。
  - [x] **0′.3c（已合入）**：`UndefinedVariable` 进 `FATAL`；`tests/fallback.rs::
    unbound_variable_travels_the_err_channel` 去掉 `#[ignore]` 并断言金样消息 `Undefined variable.`
    （语料 23 处该错误的首行都不带变量名，名字属于头下面的 source span，归 P7 7.2）。
  - [~] **0.4 harness 断言 error/warning 期望（部分完成）**：`error` 用例改为比对金样 `Error:` 首行，
      消息不等即 FAIL（旧口径“任何 Err 都算过”会凭空 +384）。未做：span/行列、`Warning:`、
      `DEPRECATION WARNING`（这类首行非 `Error:` 的用例现在一律判不匹配，诚实计为 FAIL）。
  - **可复核数据**：基线与阶段快照一律存 `target/baselines/`（`/tmp` 会被系统清理，已真实丢过一次基线导致
    拿空基线比出 `regress=7134` 的垃圾数）。本轮留存：`head-pathkey-599.tsv`（新 key、旧粗量具）、
    `tight-empty.tsv`(599)／`tight-fatal.tsv`(600)（新 key、消息金样量具，翻转前后对照）、
    `head-600-pathkey.tsv`（**往后的基线**）、`probe-*.tsv`（中间态，供复核 +385/−3 是怎么来的）。
    行为不变型改动用 `cmp` 证逐字节相同；变型必须在**空库** `ledger diff` 列 NET/REGRESS 具体键，并用
    `awk|sort|comm` 复核集合差——本轮两次证明单看一个量具不够。
- [ ] 0′.5 **共享态改 `C::RcMut`/`C::RcCell`**：内核泛型于 `C: Context`，`Local`(wasm32) 与 `Shared` 两份编译均过；
      禁 kernel 里写死 `Rc`/`Arc`。**现状**：四个内核（`Lexer`/`Structurize`/`Exec`/`Render`）状态全为观察者**独有**
      （无跨观察者共享），故本项当前**无可改之物**；它真正的触发点是 P4（`@content` 闭包流）与 P5（模块多播）。
      已做到的是：内核 `Err` 泛型 + `C: Context` 泛型，不写死具体指针类型。
- [x] 0′.6 **`sasslint` 新规则（已合入，四条全部上闸）**：引擎新增两类判定能力——**同行双签名**
      `AND_BANS`与**文件级一致性** `FileFacts`（只看非注释行，防文档注释“洗白”）：
      • `no-batch-shell`：`scan_map(`+`Vec<` 或 `flat_map(`+`from_iter(`（旧 `scan_map→Vec→flat_map(from_iter)`
        外壳的确切签名）；单独一个 `from_iter(` 是合法源，不命中。
      • `require-err-channel`：同行 `.ok()`+`eval`；或文件里有 `unwrap_or_else(` 却无 `is_fatal` 判定
        ——放过 `exec.rs` 现在这种“按形状×按 kind 显式判定”的规范透传，拦住 `value.rs`/`expr.rs` 里
        无判定的 blanket 兜底（词素解析的 `.ok()` 不属此形状，故用双签名）。
      • `no-sort-for-order`：`sort_by_key`/`sort_by(`/`sort_unstable_by`/`.sort()`。
      • `require-slot-before-subscribe`：文件用了高阶/多源（`merge_all(`/`concat_all(`/`switch_map(`/
        `flat_map(`/`connectable(`/`Subject`/`from_stream(`）且自己 `.subscribe(` 却无 `SingleAssignment`。
      **闸口已开**：`tests/lint.rs::gate_sasspile_is_clean` 去掉 `#[ignore]`（P0′.1/.2/.4/.3 已清掉旧债），
      CLI `sasslint lint crates/sasspile/src` 命中即 exit 1。实测：sasslint **10 测试全过、0 ignored**，
      `crates/sasspile/src` 命中数 **0**；四条新规则各有正/反例钉住（`detects_the_p0_prime_shapes`、
      `verbatim_fallback_needs_a_fatality_decision`、`higher_order_subscribe_needs_a_slot`、
      `legal_stream_shapes_are_not_flagged`——最后一例钉住“合法半边不得误报”这个下界，否则闸不可用）。
      诚实记录：它们是**代理而非证明**（只认本仓库真删过的形状签名），不能代替 `ledger diff`。
- [ ] 0′.7 **`Selector`/`PropName` 结构化**：消除 `render→canon→split_whitespace` 与 `&` 的 `str::replace`
      （现 `lexer.rs:223-257`、`exec.rs:150-155`）；选择器/属性名不再经字符串承载语义。

## P0 — hrx harness(验收安全网)
- [~] 0.1 `tests/hrx.rs` 经 `compile` 跑 `sass-spec` 安全子集、产 `ledger`  快照 + 立基线(seed 560/11486，现 **600**)。**已具雏形,待补 0.2/0.3**；key 制式已改（0′.3 的 0.5）
- [ ] 0.2 harness 支持**多文件输入**(partial `_x.scss`/相对 import)与目录型用例
- [ ] 0.3 harness 解析 `options.yml`:尊重 `:todo:`(skip/xfail,不入基线)、`precision`、`--indented` 语法
- [~] 0.4 harness 断言 `error`/`warning`/`deprecation` 期望(按 P7 诊断范围决定是否计入)。**`error` 已收**：
      比对金样 `Error:` 首行（详见 0′.3 的 0.4 条）；`warning`/`deprecation` 与 span 仍未计入

## P1 — 值地基:单位代数 + calculation + 全色彩空间 + list/map
- [~] 1.1 `Value`(带单位 Number/引号态 String/Color/List/Map/Bool/Null)+ `to_css` 规范化。**已有基础,缺单位换算/calc/CSS4 色**
- [x] 1.2 `Expr` 折叠 + 惰性 `eval(owned Env)`;算术/比较/布尔;`/ * %` 跳过、CSS 原生函数保留。**已做(未接入 compile 值路径)**
- [ ] 1.3 **单位换算代数**(length/time/angle/…)+ 精度(`values/numbers/**`)
- [ ] 1.4 **`Calculation` 表达式树**(`calc/clamp/min/max/var()`,可部分化简、否则原式保留)(`values/calculation/**`)
- [ ] 1.5 **CSS Color 4 空间基础**(rgb/hsl/hwb + 通道访问;`Color` 全空间留 P8)(`values/colors/**`、`core_functions/color/**`)
- [ ] 1.6 结构化 token→`Expr` 求值**接入 `compile` 声明值路径**(白名单 + 无歧义触发),经 `core_functions/**` 与 `operators/**` 金样验 `regress=0`
- [ ] 1.7 内置函数各域分派接线(math/string/list/map/color/meta/selector),经 `core_functions/**` 验。**全局裸名 `global/*` deprecation 部分可后置**(真实几乎不用,见语料分析)

## P2 — 结构化 Selector 模型
- [ ] 2.1 `Selector`(compound/combinator/伪类)parse/序列化/`&`(`css/selector/**`、`core_functions/selector/**`)
- [ ] 2.2 unify / superselector / `selector.nest/parse/unify/is-superselector/simple-selectors`

## P3 — 管线 + 控制流 + 作用域 + 媒体/at-root
- [ ] 3.1 `async fn compile` 唯一出口 + **一条冷流串起各相内核**（`Lexer→Structurize→Exec→[全局 pass]→Render→reduce_initial→into_future`）。
      ⚠ v2 口径已改：旧形态（`scan_map`+`flat_map(from_iter)`+`sort_by_key`）**不再合格** → 由 P0′ 重做，本项随之重开。
- [~] 3.2 `@if/@else`(filter + 不可变 taken 链)、`@each`/`@for`(flat_map)。**已做 `@each/@for/@if`;缺 `@while`**
- [ ] 3.3 `@while` 条件循环(`directives/while/**`)
- [ ] 3.4 **媒体/supports 冒泡 + 逐查询合并**(全局 pass;`css/media/**`)
- [ ] 3.5 `@at-root (without/with)`(`directives/at_root/**`)
- [ ] 3.6 变量块作用域 + `!global`/`!default`(`variables/**`、`libsass/variable-scoping/**`)

## P4 — 可调用:mixin / function / @content / 参数绑定
- [ ] 4.1 `@mixin`/`@include`/`@content`/`@yield` + 参数(位置/默认/`$r...`/named-splat)绑定 + 动态作用域;注册表 + 一到多多播(`directives/mixin/**`、`callable/**`)
- [ ] 4.2 `@function`/`@return` + 用户函数求值 + `meta.get-function/call`(`directives/function/**`、`callable/**`)

## P5 — 模块系统(唯一真并发处)
- [ ] 5.1 加载器(partial/index/扩展名)+ `@use`(命名空间/`as *`/`with`+`!default`)(`directives/use/**`)
- [ ] 5.2 `@forward`(show/hide/as/with 透传)(`directives/forward/**`)。**BEM 前置**:theme-chalk 内部
  (`mixins.scss` `@forward 'config'/'function'/'_var'`)必须能解析
- [ ] 5.3 legacy `@import`(弃用路径)+ CSS import 排序(`directives/import/**`)
- [ ] 5.4 独立模块**并发编译**：并发分支“订阅后才生产”或直接串行；`ref_count`/`behavior_subject` 多播
      （守不可重入）；跨线程只 `Arc<只读>`。**禁**用 `sort_by_key` 补救并发乱序（见 P0′.4 / design §5）
- [ ] 5.5 `meta.load/load-css/get-module/css`(module 为一等 `Value`;`core_functions/meta/**`)

## P6 — @extend(依赖 P2 选择器代数;**可后置,element-plus 不卡此**)
- [ ] 6.1 `@extend` 图 + 链式不动点 + `!optional` + 超选择器裁剪 + 选择器伪类并集(`directives/extend/**`);`:todo: dart-sass` 例作 skip

## P7 — 原生 CSS 透传 + 诊断
- [ ] 7.1 未知 at-rule/`@charset`/`@font-face`/`@propset`/`@keyframes`/custom property(`--*`,`var()`)/unicode-range/`!important`/转义 原样透传(`css/**`)
- [ ] 7.2 `@debug`/`@warn`/`@error` + deprecation:**精确诊断格式**(双源 trace:invocation+declaration);诊断**已定为 v1 纳入**(见已定决策),error/warning 走 scoped 基线

## P8 — CSS Color 4
- [ ] 8.1 全色彩空间(lab/lch/oklab/oklch/srgb/display-p3/…)adjust/change/construct + **gamut mapping**(`core_functions/color/**`)

## 收尾(每阶段末与整体)
- [ ] 9.1 `element-plus` theme-chalk 端到端编译(**经 P1/P3/P4/P5/P7 主干即应达成,不依赖 P6**),无 panic、输出与自基线一致不劣化(`ledger` 归一化)
- [ ] 9.2 全量 `sass-spec`(排除 `non_conformant`/`libsass-*`/`:todo:`)通过率相对基线单调上升、`regress=0`;`sasslint` 0;clippy `-D warnings` 绿
