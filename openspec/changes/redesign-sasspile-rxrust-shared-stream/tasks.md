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
> 根子拆掉，再往下堆语言面；每子项完成后跑 scoped `regress=0`（当前整库基线 **pass=643/11,533**，其中
> **实质通过 478**（非空期望）、空期望蒙对 165；量具 = 相对 `spec/` 全路径 key + 错误消息金样 + 9 个安全
> 子集目录，现直接输出 `pass_substantive`/`pass_empty_expectation`，见 0′.3 末“可复核数据”与 0.6）。

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
      的 `case-key-scheme` note 里记明，新基线是 ledger run `head-600-pathkey`）。
  - [x] **0′.3c（已合入）**：`UndefinedVariable` 进 `FATAL`；`tests/fallback.rs::
    unbound_variable_travels_the_err_channel` 去掉 `#[ignore]` 并断言金样消息 `Undefined variable.`
    （语料 23 处该错误的首行都不带变量名，名字属于头下面的 source span，归 P7 7.2）。
  - [~] **0.4 harness 断言 error/warning 期望（部分完成）**：`error` 用例改为比对金样 `Error:` 首行，
      消息不等即 FAIL（旧口径“任何 Err 都算过”会凭空 +384）。未做：span/行列、`Warning:`、
      `DEPRECATION WARNING`（这类首行非 `Error:` 的用例现在一律判不匹配，诚实计为 FAIL）。
  - **可复核数据 = `reports/hrx.db` 里的具名 run，而且不再经任何中间文件**（本轮改错又改回、最后连文件接口都干掉了）：
    要留记录就 `HRX_RUN=<增量名> cargo test -p sasspile --test hrx`，harness **直接写库**
    （`ledger::record`，早在 sasspile 的 dev-dependencies 里却从未被接；`HRX_DB=<临时路径>` 用于强制的
    空库复算）；不设 `HRX_RUN` 则只量不写、不留产物。A/B 靠 `ledger diff`、列表 `ledger runs`、
    失败详情 `ledger failures <run>`。入库与旧的 `import <tsv>` 通路已验证等价：
    `ledger diff bool-units-746 db-direct-746` = **regress=0 net=0**。
    **曾误做法（两层）**：① 我自己写过“快照存 `target/baselines/`”，堆出 15 个文件 / 27 MB 并多建一个
    临时 sqlite——那正是 `crates/ledger` 建立时要消灭的 `reports/hrx-*.tsv` sprawl；② 改入库后仍保留
    “harness 写一个 TSV + `ledger import <file>`”的中间步骤，于是 A/B 必须凑两个文件，我只能用
    `git stash` 重建“改动前”快照——它就撞过一次 `index.lock`，让一组旧码读数差点进入结论。
    现已无散文件、无 import、无 stash 重建。
    现有可追溯的 run（path-key 制式）：`head-600-pathkey`(600，粗量具)、`s2-fatal-empty-599`(599)、
    `s2-fatal-flip-probe-600`(600)、`p1-named-args-600`、`p1-quoted-string-600`、`scope-full-616`(616)、
    `head-633-concat`(633)、`head-635-flags`(635)、`inert-callable-bodies`(635)、`for-loop-643`(643)、
    `call-spine-653`(653)、`math-family-692`(692)、`variadic-minmax-714`(714)、**`bool-units-746`(746，当前基线)**
    与等价的 `db-direct-746`（第一个由 harness 直接写入库的 run）。在此之前的 basename-key run 与它们
    **不同制式，不得跨制式 diff**（库里 `case-key-scheme` note 已记明）。
    行为不变型改动改为：把两侧各以一个 run 名入**同一个空库**（`HRX_DB=<临时库>`）后 `ledger diff` 证 net=0
    （TSV 已不存在，`cmp` 无对象可用）；变型则另在空库上 `diff` 列 NET/REGRESS
    具体键，并用 `awk|sort|comm` 复核集合差——本轮两次证明单看一个量具不够（临时空库只为验收，
    **不作为记录留存**）。
  - **量具自身必须带阳性对照（P1.3 新血泪）**：一次“仅差空白=0、仅差大小写=0”的读数让我差点得出
    “无 cheap win”的结论，实际是 **`substr` 切片多吞一个引号字符**的 bug；拿一条已知应该为真的用例
    （`calc-size::case_insensitive`）当对照，才暴露出量具错。任何“为 0 / 全通过”的统计必须先过一条阳性对照。
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
- [x] 0′.7 **`Selector`/`PropName` 结构化（已合入）**：新增 `src/selector.rs`——`Piece::{Text,Parent}` +
      `Selector`（`from_pieces`/`references_parent`/`nest`/`trimmed`/`to_css`）+ `PropName`（构造时就从
      pieces 定下 `is_custom`）。`exec` 的 `interp_text`（拼串）换成 `head_pieces`：**每词素映 0..N
      piece 后 `concat`**，`&` 组合走 `Selector::nest`（splice），**`str::replace` 与 `combine` 已删**；
      `StyleRule{selector: Selector, decls: Vec<(PropName, Value)>}`；绑定目标改成 `Target::{Variable,Property}`
      （先拼 `"${var}"` 再 `strip_prefix('$')` 的回环消失）。前置：词法把 `&` 升为独立词素
      （`STRUCTURAL`），因为分类粒度不能超过词法粒度（0′.3b 的教训）。
      **两条语料新发现**（均为本轮实测而非推定）：
      1. `#{&}` **不是** `&` 组合位置——语料期望“把父选择器当**文本**代入后照常嵌套”：
         `.test{ .selector#{&}{} }` → `.test .selector.test`（`libsass-closed-issues/issue_1043`）。
         我先把它映成 `Piece::Parent` 得 `.selector.test`，新写用例自身按语料不成立 → 改为带 `parent` 文本
         入参的 `pieces_of`。（旧字串写法同样错，但 `libsass-*` 不入基线，从未被量到。）
      2. 词法在行注释结束时 `buf.push(' ')` 把换行发成了 **`Word(" ")`** ——伪装成词素的空白。它一直存在，
         但新的词素级绑定判定（首个非空白/非注释词素是不是 `Var`）使它把 `// c\n$ids: ();` 里的 `$ids`
         当普通属性处理 ⇒ 变量未绑定 ⇒ 真 `Err` 通道直接报错（实测一度回退 16 条）。正解是修根：该位置发
         `Tok::Space(' ')`，空白在词法层就是空白。
      实测（全路径 key + 消息金样量具）：**600 → 600，`regress=0 net=0`**（空库 `ledger diff`）——属结构
      改造，不应也真的没移动任何判定。全量 `diff` 两快照仅 **1 行**不同，且属于本来就 FAIL 的
      `css/custom_properties/nesting_characters::`：它的垃圾文本里“两个空格”变“一个空格”，无判定变化。
      新增 `tests/selector.rs` 4 例（root `&`/`&.b`/`&b`/descendant、`:has(~ &)`、`:where(&)`、`#{&}`、
      注释不伪装下一个绑定），每条期望都注了出处 fixture。门：57 测试全过 0 ignored、clippy
      `-D warnings` exit 0、`sasslint count=0`。

## P0 — hrx harness(验收安全网)
- [~] 0.1 `tests/hrx.rs` 经 `compile` 跑 `sass-spec` 安全子集、产 `ledger`  快照 + 立基线(seed 560/11486，现 **600**)。**已具雏形,待补 0.2/0.3**；key 制式已改（0′.3 的 0.5）
- [ ] 0.2 harness 支持**多文件输入**(partial `_x.scss`/相对 import)与目录型用例
- [ ] 0.3 harness 解析 `options.yml`:尊重 `:todo:`(skip/xfail,不入基线)、`precision`、`--indented` 语法
- [~] 0.4 harness 断言 `error`/`warning`/`deprecation` 期望(按 P7 诊断范围决定是否计入)。**`error` 已收**：
      比对金样 `Error:` 首行（详见 0′.3 的 0.4 条）；`warning`/`deprecation` 与 span 仍未计入
- [x] 0.6 **安全子集目录清单补齐（已合入，量具缺陷）**：`operators`、`parser`、`variables` 三个目录从未被
      `SPEC_DIRS` 扫到，共 **79 例不在计量内**——而任务 1.6 自己写着“经 `operators/**` 金样验 regress=0”，
      即规格引用了一簇而量具不扫它（已写进 `sasspile-conformance` 的 hrx harness 契约）。
      实测：总数 11,486 → **11,533**，PASS 600 → **616**（新覆盖里 operators 4/25、parser 1/3、variables 11/3 通过），
      **旧覆盖集内 PASS→FAIL = 0**（逐条 `comm` 比对，扩面的验收判据就是这条，不是总数上涨）。
      基线换为 ledger run `scope-full-616`。
      **新暴露的 25+3+3 条已有名字**，已归入 P1（见 1.3b/1.4b）与 P3，不再靠猜。

## P1 — 值地基:单位代数 + calculation + 全色彩空间 + list/map

> **P1 地形（2026-10-26 逐条量具实扣，免得下一轮重量）**：基线 run `head-635-flags`（635/11,533）。
> 失败最大家：`core_functions/color` 6,323（占全库失败 58%）、`values/calculation` 900、
> `core_functions/selector` 898、`core_functions/math` 482。两个结论决定了顺序：
> 1. **不存在“便宜的批量赢法”**：`values/calculation` 的 900 = 436 条期望已无 `calc(`（需真化简）
>    + 439 条需逐条错误消息 + 20 条消息不符；非-calc 上下文里含 `*` 的 53 条 input **全部带 `math.`**
>    （⇒ 单独做 `*` 运算符零收益，它是前置不是赢面）；“仅差空白/仅差大小写”两类候选均被语料
>    反例推翻（见 1.4b/1.4c）。
> 2. **`color` 家不是策略问题而是能力问题**：1,945 条“期望错误却给了 CSS”按金样首行分桶后是长尾
>    （实参个数 128、逐参数类型/单位校验 259、calc 内禁用 79、plain-CSS 内禁用 61、解析器期望符 261、
>    选择器代数 67…）——需要内置函数注册表 + 每参签名校验（P1.7）与 `Value::Color`（P1.5），
>    **不得用兜底不报错抹平**。⇒ P1 真实关键路径 = 1.4 `Calculation` + 1.7 内置函数分派（含参数绑定，
>    1.2b 现在跳过含具名实参的 run）+ P4 的“声明不求值体”（解开 31 条假错）。

- [~] 1.1 `Value`(带单位 Number/引号态 String/Color/List/Map/Bool/Null)+ `to_css` 规范化。**已有基础,缺单位换算/calc/CSS4 色**
- [x] 1.2 `Expr` 折叠 + 惰性 `eval(owned Env)`;算术/比较/布尔;`/ * %` 跳过、CSS 原生函数保留。**已做(未接入 compile 值路径)**
- [x] 1.2b **具名实参不是引用**（本轮实测发现并修）：`color.hwb($hue: 180, …)` 里的 `$hue:` 是参数**标签**，
      旧逻辑去环境查它 ⇒ 对外观正常的程序报出假的 `Undefined variable`。全库实测 **1,611 条**在用用例受此影响
      （`core_functions/color` 1,376 条，另 list/meta/math/selector 等）——与已修的 `@for $i` 头部是同一类建模错（绑定/命名位当引用）。
      判据 `expr::named_argument` 落在**词素**上：调用的 `(` 与 callee 粘合（`Word("hwb(")`）而 map 字面量的 `(` 独立
      （`Word("(")`），所以 `($z: null)` 里的 `$z` 仍按引用处理（语料反例：`map.remove($map: (c: d), $key: c)`）。
      含具名实参的整个值 run 保持原文（调用求值属 P4）。实测：无效错误行 **1,611 → 118**；
      判定集合未移动（空库 `ledger diff base600 named-args` = **regress=0 net=0**，因这些用例仍缺调用求值）。
      新增 `tests/named_args.rs` 4 例包边界（全具名/嵌 map/命名空间 callee，以及`hwb($missing)` 仍必错——
      防它退成万能兜底）。需求已写进 `callables/spec.md`“具名实参的位置是标签”。
      **指标教训**（归 0.4 后续）：“FAIL 变 FAIL”在 pass 读数上看不见，所以下一步除了净涨还必须直接测
      **无效错误计数**（`ledger failures <run> "unexpected error"` 的行数，当前基线 55），否则假错会一直藏在失败阴影里阻塞 P1.7。
- [x] 1.2c **串内的 `$` 不是引用（已合入）**：词法新增 `quote`/`escaped` 状态——进入 `"`/`'` 后，反斜杠转义下一
      字符（`\"` 不提前闭合）、遇到同名引号闭合、**`$` 当普通文本**不再开 `Tok::Var`。依据是语料自身：
      `core_functions/selector/extend/simple/pseudo/arg::class/equal` 就传 `selector.extend(":c(@#$)", …)`。
      实测：无效 `Undefined variable` 行 **118 → 93**；判定集合仍不动（空库 `ledger diff base600 quoted-string`
      = **regress=0 net=0**，因这些用例仍缺调用求值）。新增 `tests/lexer.rs::dollar_inside_a_quoted_string_is_text`。
      **同时修一处 special 名过严**：`css/functions/special/prefixed/uppercase.hrx::expression/script_like`
      输入 `a {b: -C-EXPRESSION($d)}` 期望原样保留 ⇒ `expr::special_call` 的基名匹配改为**大小写无关**
      （实测同一输入的 lowercase 一直 PASS、uppercase 因名字不匹配而报错）。
      **边界说清（不粉饰）**：本项只把 `$` 从串里抓回来，**带引号串仍不是一个词素**——串内的
      `:` / `;` / `{` / `//` 依旧会拆词与误判为注释（例如 `content: "a: b"`、`"a//b"`）；把它做成带
      引号态的单一词素是 1.1 `Value::String` 的真前置，单独立项。
      **修正上一轮的错引**：我写的 `values/strings/**（129 条失败）` 两处都不对——该路径是**文件**
      `values/strings.hrx`（入基线 4 条，现均 FAIL：`new-line/*` 需要“串内裸换行报 parse 错”与引号态转换），
      而 129 是 `values/numbers` 的数。串类函数在 `core_functions/string/*`（153 条失败）。
- [ ] 1.2d **剩余 93 条假错的真实归属（逐条分桶而来，不是策略问题而是能力缺失）**：
      • 36 条 = **模块变量未加载**：`@use "sass:math"` 后的 `math.$pi`/`math.$e`（归 P5.1）；
      • 31 条 = **可调用体在声明处被求值**：`@mixin add_two($v) {b: $v + 2}` 未调用也应不报错（✅ 已由 4.0 解开，
        本轮实测无效错误 93 → 64）；
      • 14 条 = **循环变量未绑定**：`@for $i from …` 里的 `$i` 头部已不查（P0′.3b-1）但循环体未绑定（归 P3.2）；
      • 12 条 = 其他，待逐条再看。
      注：这些**不得用“兜底不报错”抹平**（铁律禁），只能按归属把特征实现掉。
- [ ] 1.3 **单位换算代数**(length/time/angle/…)+ 精度(`values/numbers/**`)
- [x] 1.3b **无引号标识符的 `+` / `-` 拼接（已合入）**：`expr::concatenation` 是数字折叠之外的**另一条折叠**，
      由 `from_tokens` 在 `arithmetic` 返回 `None` 后 `or_else` 尝试。规则全由金样定：
      `operators/plus.hrx::syntax/*` 把 `c+d`/`c + d`/`c +d`/`c+ d`/`c/**/+/**/d` 全折叠为 `cd`（`+` 消失），
      `operators/minus.hrx::syntax/*` 把 `(c)-(d)`/`c - d`/`c -(d)`/`(c)- d` 全折叠为 `c-d`（`-` 作为文本保留）。
      括号靠 `unwrapped`/`whole_wrap` 剥**完整包裹**层（`(c)(d)` 不剥）。
      **守住的边界**（故意窄到只吃这些形）：操作数必须**全是标识符**且至少有一个运算符，掺数字则退回原路
      ⇒ 间距律与 `10px -5px` 二分列表、`1px+2px` 逐字输出、单标识符、`c-d` 词素均不变（`tests/operators.rs`
      逐条钉住这些反例）。
      实测：`operators/plus` 与 `operators/minus` 两族**全绿**——18 个 section 里 17 条新通过
      （`minus::syntax/comment/both` 本来就过）；空库 `ledger diff base616 concat` = **regress=0 net=17**，
      基线 616 → **633**（新基线 ledger run `head-633-concat`）。
      上轮估的“21 条”不准（把 slash/modulo 混进来了）：更正为 plus 9 + minus 9 = 18 个 section。
      `operators` 现仍剩 8 条：**slash 4**（归 1.4b，斜杠空白依赖来源）、**modulo 4**（需 `%` 与 infinity）。
- [ ] 1.4 **`Calculation` 表达式树**(`calc/clamp/min/max/var()`,可部分化简、否则原式保留)(`values/calculation/**`)
- [ ] 1.4b **斜杠两侧的空白取决于来源，不得一刀切（本轮否掉了一个诱人但错的方案）**：
      表面上看 56 条失败“仅差空白”，全部是 `b / 25%` 应为 `b/25%`（相对颜色语法，
      `core_functions/color/hwb/one_arg::relative_color/*/alpha` 等）。但语料同时给出**反例**：
      `values/lists/slash.hrx` 要 `list.slash(c, d)` → `c / d`（带空格），且全库期望输出里
      **含 `X / Y` 带空格的行有 1,060 处**（如 `calc(infinity / 1px)`）。⇒ 斜杠空白是
      “这个列表来自**源码文本**还是由**函数构造**”的上下文属性，所以下列两个做法都是假的：
      ① 序列化时无条件删空格；② 无条件保留。必须等 1.4 的 Calculation/`Value::List`（三种分隔符 +
      来源标记）与 1.5 的相对颜色解析一起定；`operators/slash::separator/calculation/simplified/*`
      （`calc(1)/calc(2)` → `1/2`）是同一条依赖。
- [ ] 1.4c **special 调用名的大小写不是字符串规则，而是“走了哪条解析路径”的产物（本轮量出一个诱人的假赢法并弃用）**：
      “仅差大小写”的 24 条看似可用“输出时把 special 名转小写”一刀收，但同一文件
      `css/functions/special/prefixed/uppercase.hrx` 内部三族期望互斥：
      • `-A-CALC($c)` → `-a-calc($c)`、`-C-ELEMENT(...)` → `-c-element(...)`：走 plain-CSS 的
        calc/element/expression 路径 ⇒ 名**小写、前缀保留**；
      • `-C-URL(0)` → `url(0)`：字面量 url 被当 plain-CSS url **重新序列化** ⇒ **前缀被去掉**；
      • `-E-URL($a)`（`$a: b` 已绑定）→ `-E-URL(b)`：实参是变量时按 SassScript 解析，变成**未知函数调用**
        ⇒ 名字**原大小写逐字保留**（与 1.2b 同一条“special 参数里绑定变量仍解析”的推论）。
      ⇒ 所以必须先有“这个调用到底按哪条路径解析”的判别（= 1.4 的 Calculation/特殊调用解析 + P1.7
      的调用求值），序列化才有依据；**禁止用小写化的补丁“先赢 20 条”**，它会把后两族写错。
      另外 `directives/function/name::error/special/*` 要 `Invalid function name.`（声明 `element()` 等
      保留名非法，且大小写不敏感），它属 P7 诊断，不在本条。
- [~] 1.4d **有理（复合）单位：`Unit { num, den }` 取代单位字符串（前半已合入，calc 内化简待做）**
      **三条语料纠正了本项原本钉死的判据**（都不靠推定，逐条有 fixture）：
      1. **不是“构造时按名字排序”**：`math.unit(1px * 1em * 1rad)` 要 `"px*em*rad"`、
         `math.unit(1px * 1em / 1rad / 1s)` 要 `"px*em/(rad*s)"` —— 都是**输入顺序**，排序会变 `em*px*rad` 而全错。
         所以 `src/unit.rs` 是**保序多重集**，只有同名抵消是结构性的（`cancel` 一次遍历分子，分母同步扣）。
      2. **输出律比记的细**：无分子时分母走**负幂** —— `1/1px` → `"px^-1"`、`1 / 1px / 3em / 4rad` →
         `"(px*em*rad)^-1"`；有分子才写分数：单分母 `px/em`、多分母 `px*em/(rad*s)`。
      3. **复合单位不能离开 `calc( )`**（`Unit::is_plain`）：`calc(1px / 2px)` → `0.5` 出壳
         （`calc/operator/units::division`）、`calc(1px / 2)` → `0.5px`（`divide::simplified`），而
         `calc(1 / 1px / 1rad)` 必须留壳（`values/numbers/units/multiple::multiple_denominators`）。
         这条是落地时由**一条真回退**发现的：旧写法先把 `1(px*rad)^-1` 裸打印出来，簇内 `regress=1`。
      4. **重复单位的写法仍无金样**：全语料只有 `libsass-closed-issues/issue_783.hrx` 的**注释行**（该目录不入
         基线），所以 `px*px` 保留重复而不造 `px^2`；`unit.hrx` 自己的 README 也说该函数“仅供调试、
         不要求各实现完全一致”。
      **已合入部分**：`src/unit.rs`（转换表从 `value.rs` 搬入 + `Unit`）；`Value::Number(f64, Unit)` 全线
      改用（`value.rs`/`expr.rs`/`builtins.rs`/`exec.rs` 共 37 处持有面）；`BinOp::{Mul, Div}` + `build()`
      优先级（最右 `+`/`-` 为根，所以左结合且 `*` 绑得紧；递归只在**值表达式**上，不是文档树）；
      词法把 `*` 提成 `Op`（`/` 故意不提成，否则 `font: 12px/1.5`、`1/2` 斜杠 list 与 `url(http://x)` 全错），
      而 `/` 只在**调用参数内**由 `expr::release_slashes` 释放为算子（就是计划说的“斜杠语义是上下文属性”，
      且引号内不切）；`+`/`-` 的单位律改为只看 `single()` 单位（复合单位不得半换算）。
      **拆分补记**：`expr.rs` 涨到 542 行（超单文件 500 红线），按“词素→运算树”这个边界切出
      `src/operations.rs`（234 行，`classify`/`number_lexeme`/`concatenation`/`arithmetic`/`build`），
      `expr.rs` 保留读取顺序与 `eval`（326 行）；拆分后用 A/B 证 **net=0 regress=0**（run `split-check`）。
      **实测**：hrx **768 → 778（实质 603 → 613，空期望 165 不变）**，`ledger diff trace-instr-768
      units-rational-768` = **`regress=0 net=10`**；10 条全在候选集内：`math/unit::{one_denominator,
      multiple_denominators, multiple_numerators, numerator_and_denominator/{single,multiple}}`、
      `math/unitless::{denominator, numerator_and_denominator}`、`calc/operator/divide::{simplified,
      no_whitespace}`、`calc/operator/units::division`；无效错误仍 **53**（不涨）；
      94 测试/0 失败/0 ignored，clippy `-D warnings` **exit 0**（不再用无条件 `echo OK` 冒充门禁），
      `sasslint` CLI `count=0`。
      **后半已合入（平展归约，不是树）**：`src/calc.rs` + `src/brackets.rs`。`brackets.rs` 把词素 run **一次 fold**
      成带 `level`/`owner` 的文档序 run（深度成为数据），并把`var(--c)` 这类调用与其平衡参数列**折成一个词素**；
      `calc.rs` 按**层自底向上归约成摘要项** `Term{value, printed, precedence}`（不存子节点、不递归、不拿旗标
      表达“看不懂”——那走 `Reading::Declined` 错误通道），括号只在 join 那一刻由优先级定一次。
      **两个真实调头**（都是同一课：分类粒度不能超过词法）：① `opens` 最初把任何以 `(` 结尾的词素当开括号，
      于是 `var(` 的 `)` 被组吃掉，`calc((1 + var(--c)))` 读不懂；② 合并后的 `var(--c)` 以 `)` 结尾，又被
      `closes` 当闭括号删掉 —— 现仅 `Punct(')')` 算闭合。另外 `calc(min(...))` 与 `calc(math.round(2.3))` 这类
      **整个参数就是一个调用**的形状仍由内置表/`find` 求值（`whole_call`），与其它操作数混排则无金样依据→decline。
      **实测**：hrx **778 → 823（实质 613 → 658，空期望 165 不变）**，`ledger diff units-rational-768 calc-flat`
      = **`regress=0 net=45`**；按簇：`values/calculation` net=43（`precedence` 13、`simplify` 6、`constant` 6、
      `plus` 4、`minus` 3、`times` 2、`var` 2、`parens` 1）、`min`/`max` 各 3、`values/numbers/units` net=2
      （`multiple::division/by/{multiple_denominators,multiple_numerators}`）；无效错误仍 **53**。
      中途曾出现两条真错，均由 A/B 揭出而非推定：一是 `var(` 被当组导致 `function/sass/namespace` 归退，
      二是 `format_number` 的 `exponent.max(0.0)` 使 `(0,1)` 少一位（`0.763636364` vs 金样 `0.7636363636`）。
      **仍剩（不属 1.4d 字面范围，但候选集点了名）**：`math/div::unit/{denominator,incompatible,unknown}` 与
      `math/comparable::unit/to_inverse`、`calc/operator/units/{denominators,multiplication}`、
      `multiple::division/cancels/*` —— 它们缺的是**调用/已绑定计算作为外部算术的操作数**
      （`math.div(6, 3px) * 1px`、`calc($number / (1 / 1ms))`）：声明值解析在 `operations::classify` 里把非数字
      词素一律记 `Other` 而整串放弃。下一刀是把它换成 `Piece::Operand(Expr)`（并让 `Value::Calculation` 携带
      可参与运算的值），而 `calc/cancels` 一族还要求绑定计算能拆回项（得把文本换成树）。
      `calc.rs` 400 行、`brackets.rs` 207 行、`calculation.rs` 446 行（均在 500 红线内）。
      词法输出律（全来自 `core_functions/math/unit.hrx`，不推定）：
      • `math.unit(1px)` → `"px"`；`math.unit(1)` → `""`；
      • `1px * 1em * 1rad` → `"px*em*rad"`（分子用 `*` 连接）；
      • `1px / 1em` → `"px/em"`；`1/1px` → `"px^-1"`（单一分母不加括号）；
      • `1px * 1em / 1rad / 1s` → `"px*em/(rad*s)"`（多个分母加括号）。
      求值律（来自 `div.hrx` 与 `units/multiple.hrx`）：
      • `math.div(6in, 3s) * 1s` → `2in`；`math.div(6c, 3d) * 1d` → `2c`（不可换算时**不约**，成为 `in/s`）；
      • `calc(1 / (1 / 1px / 1rad))` → `calc(1px * 1rad)`、`calc(1 / (1px * 1rad))` → `calc(1 / 1px / 1rad)`
        ⇒ 这些期望写在 `calc(…)` 内部，所以**真正的门禁是 1.4 的 `Calculation`**，不是 `Value` 字段本身；
      • `calc(calc(1px * 1rad / 1ms / 1Hz) / (1px / 1ms))` → `calc(1rad / 1ms / 1Hz)`（同单位两边同时抵消）。
      数据模型决定（已按上面第 1 条纠正）：单位是**两个多重集**（`num`/`den`），**保持输入顺序**并在构造时
      抵消相同项（不是“按名字排序”——那会把 `px*em*rad` 排成 `em*px*rad` 而违背金样），因此“抵消”是结构相等
      而非后置算法；序列化只在 `Unit::to_css` 一处按上述律输出。这是**值**，自底向上求值，不引入可遍历的 AST
      （不违反“禁物化 AST 再遍历”）。
      必须同批改完的点（任一缺失就编译不过或语义不闭合）：`value.rs`（变体 + `to_css`）、`expr.rs`
      （`number_lexeme` 产出 `Unit`；`BinOp` 需先有 `Mul`/`Div`）、`calls.rs`/`builtins.rs`
      （`unit`/`is-unitless`/`compatible`/`min`/`max`/`div` 改用 `Unit`）、`exec.rs` 的 `protected` 精化。
      **`protected` 精化不能靠“看空白”**：本轮已量到 1,060 处期望输出保留 `X / Y` 反例（如
      `calc(infinity / 1px)`），而 `1/2` 在普通声明值里仍是**斜杠 list** ⇒ `/` 的语义是上下文属性，
      必须由“是否在 `calc(…)`/调用参数内”判定，而这正是 1.4 的开关。
      验收序：先 1.4 的 `Calculation` 框架（原式保留 + 可部分化简），再本项接上 `*`/`/`；
      候选转 PASS 集：`math/unit::*` 的 6 条复合行、`comparable::unit/to_inverse`、
      `div::unit/{denominator,incompatible,unknown}`、`values/numbers/units/multiple::division/*`；
      硬约束：`operators/slash`（`1/2` 仍为 list）不得动，无效错误不得从 55 上涨。
      尚缺证据因而**暂不做**：同一单位重复出现时的书写（`px^2` 一类幂次记法）——语料里没找到对比用例，
      实现前必须先取到金样。
- [ ] 1.5 **CSS Color 4 空间基础**(rgb/hsl/hwb + 通道访问;`Color` 全空间留 P8)(`values/colors/**`、`core_functions/color/**`)
- [ ] 1.6 结构化 token→`Expr` 求值**接入 `compile` 声明值路径**(白名单 + 无歧义触发),经 `core_functions/**` 与 `operators/**` 金样验 `regress=0`
- [ ] 1.7 内置函数各域分派接线(math/string/list/map/color/meta/selector),经 `core_functions/**` 验。**全局裸名 `global/*` deprecation 部分可后置**(真实几乎不用,见语料分析)
- [x] 1.7a **调用脊柱（已合入）**。选型依据（逐条量具）：
      期望 `true`/`false`/`null` 的 616 条失败里 **397/400 需要模块或内置函数**，只有 3 条是纯值比较
      ⇒ 先做比较/布尔 `Expr` **不是杠杆**；杠杆是“调用求值”。`math` 家频次：`div` 185、`pow` 149、
      `clamp` 43、`hypot` 33。**以下均为 `core_functions/math/div.hrx` 实测期望，不是推定**：
      | 输入 | 期望 | 要点 |
      |---|---|---|
      | `math.div(6, 3)` | `2` | 无单位 |
      | `math.div(6, 5)` | `1.2` | 格式（见 `format_number`，最长 10 位） |
      | `math.div($number2: 3, $number1: 6)` | `2` | **具名实参按标签绑定，与书写顺序无关**（终结 1.2b 的“整段跳过”） |
      | `math.div(6px, 3)` | `2px` | 分子带单位/分母无单位 → 保留分子单位 |
      | `math.div(6px, 3px)` | `2` | 同单位 → 约掉 |
      | `math.div(6in, 3px)` | `192` | **可换算单位先换算再约**（定死 `1in = 96px`） |
      | `math.div(6in, 3s) * 1s` | `2in` | 不可换算时是**复合（有理）单位** `in/s` —— **不属本切片**，不得猜 |
      | `math.div(6c, 3d) * 1d` | `2c` | 未知单位也是复合单位 ⇒ 同上，留给单位代数 |
      | `math.div(b, 3)` | `value: b/3; type: string;` | 串/非数值分支（带 `warning` 段）——不属本切片 |
      | `math.div(6)` | `Error: Missing argument $number2.` | **harness 只比 `Error:` 首行** ⇒ 消息可精确实现 |
      | `math.div(6, 3, 1)` | `Error: Only 2 arguments allowed, but 3 were passed.` | 同上，两条都是**签名通用**而非逐函数 |
      实现拆解（必须同批落地，缺任一件就是不可提交的死码）：① 词素层切分实参（callee 的 `Word("…(")` 开深度，
      顶层 `Punct(',')` 分割，`)` 收；嵌套 `(`/`)` 计深度）；② `$label:` 识别为**标签**并参与绑定（与 1.2b 的
      `named_argument` 拒绝共存：有签名可查时绑定，无签名时仍逐字保留）；③ `Expr::Call` + `builtins.rs` 注册表
      （按域拆文件是已定惯例），签名统一声明 `params` 以派生上述两条 arity 消息；④ 数值相除的单位律与
      `1in = 96px` 表（只加能由金样证实的单位）；⑤ `format_number` 已存在，不得用新格式推翻现有 PASS。
      新增致命错误类（arity 两条）必须走 0′.3b 的流程：空库重跑全量 `regress=0`，否则改为非致命（保留逐字）。
      **本切片验收判据**：`div.hrx` 的 `unitless/{whole,fraction,named}` + `unit/{numerator,denominator,same,compatible}`
      转 PASS，且无效错误不增。
  - **已落地与实测**：翻转了 `unitless/{whole,fraction,named}`、`unit/{numerator,same,compatible}`、
        `error/{too_few_args,too_many_args}` 共 9 条，外加 1 条因 `format_number` 拼写修正而过的
        `values/calculation/calc/constant::infinity/math/unsimplified/from_variable`（`math.div(1,0)` 得
        Rust 的 `inf`，而 Sass 写 `infinity`；NaN 两边拼写本就一致所以无需特例）。合计
        **643 → 653（实质 478 → 488）**，新空库重跑两侧 `ledger diff prev spine` = **regress=0 net=10**，
        入库 run = `call-spine-653`；无效错误保持 55 不增。**未达成项**：`unit/denominator`、
        `unit/incompatible`、`unit/unknown`、`non_numeric/*` —— 它们要的正是复合（有理）单位，属单位代数，
        本轮按契约“推辞不猜”（测试里钉住逐字输出）。
  - **两个真实教训（已入代码注释）**：
        ① **词法事实错两次**：`(` 与 `)` 都不是 STRUCTURAL，且 `(` 不冲刷当前 word，所以
           `math.div(6)` 到达时是**一个**词素 `Word("math.div(6")`（被调名+括号+首个实参粘在一起）。
           我前两版都建立在“`Word` 以 `(` 结尾”上，结果**判定集完全不动**（net=0）——这就是
           “分类粒度不能超过词法”的代价；修法是在 `calls.rs` 里局部规范化括号边界（`brackets`/`cut`），
           不动上游词法，`url(http://x)`、`-a-calc(` 的现有行为不受影响。
        ② **`sasslint` 真的拦住了我**：`no-recursive-descent` 的子串禁 `fn resolve(`（记录已删的拉式引擎
           递归解算器），新写的签名查表刚好叫 `resolve(` 就撞上门。选择是**改名 `signature` 而不是放松闸**
           ——放松会把真模式以后放过去；测试 `gate_sasspile_is_clean` 仍是绿的。
  - **往后变便宜了**：注册表添加一个函数 = 一条 `Builtin`（名字+params+body），arity 消息与 `$label:` 绑定
        自动到位；下一个目标按频次是 `math.pow`(149)、`math.clamp`(43)、`math.hypot`(33)。`@use` 的别名
        (`as foo`) 仍属 P5.1（现在按字面名匹配，所以 `math.div` 无需加载器就能跑）。
  - **第二批（紧接首片落地）：`math.round`/`abs`/`ceil`/`floor` + 类型校验 + 带符号字面量**。
        实测 **653 → 692**（新空库 `ledger diff prev math-family` = **regress=0 net=39**，入库 run
        `math-family-692`），新通过 39 条中 **38 条就在 `core_functions/math`**（目标家族），实质通过
        488 → **527**；无效错误仍 **55**（没有臆造出新错误）。
        两个语料细节决定了形状，**不是统一规则**：
        • **类型校验按函数而不按全局**：`round.hrx::error/type` 要致命错 `$number: c is not a number.`
          （值按 CSS 形式渲染：无引号标识符不加引号、串保留引号 ⇒ `$number: "0" is not a number.`），
          而 `div.hrx::non_numeric/numerator` 要的是 **deprecation warning + 仍然出值**——所以 `div` 的体
          对非数字“推辞”而不是报错。把校验做成全局一律致命就是臆造错误。
        • **前导符号属于数字**：`math.round(-5.6)` 一度报出假错 `$number: -5.6 is not a number.`，因为
          `number_lexeme` 不吃 `-`/`+`（与 `@for` 负界限同一缺口）；修后 `10px -5px` 这类“无运算符的两
          个操作数”仍按原路逐字（`arithmetic` 因 operators 不匹配而 bail），未把空格列表误当成减法。
        未达成项不变：`atan2`/`pow`/`clamp`/`log`/`hypot`/三角函数需要真正的数值格式精度与单位规则（含
        `1.4999999999949998` 这类精度边界），复合单位仍是 `unit/{denominator,incompatible,unknown}` 的卡点。
  - **第三批：变参 `math.min`/`math.max`**。实测 **692 → 714**（新空库 `ledger diff prev minmax` =
        **regress=0 net=22**，入库 run `variadic-minmax-714`），新通过 **恰好** `min` 11 + `max` 11（其余
        是本家的 `global/*` 裸名与 `%` 变体），实质通过 527 → **549**，无效错误仍 **55**。
        • `Builtin` 新增 `variadic`：`params` 为空 ⇒ **无参名可绑**，带 `$label:` 的变参调用直接推辞（与 1.2b
          同一条“不得猜哪个是标签”）；零个实参报语料自己的 `At least one argument must be passed.`（复用
          `MissingArgument` 类，不新增错误类）。
        • 变参的类型消息**没有 `$参数:` 前缀**（`c is not a number.`），因为无参数可命名——这个差异是从
          `min/max.hrx::error/type/*` 原文读出来的，不是把固定 arity 的消息模板套用。
        • 单位律：同单位或一方无单位 ⇒ 直接比大小；两边都在**因子表**内 ⇒ 换算后比，但**返回原值**
          （`max(1px, 1in, 1cm)` → `1in`，不会被改写成 `96px`）。长度因子全部引自语料自己给出的数字
          （`libsass/units/conversion/size.hrx`：`1in`→`96px`、`1cm`→`37.7952755906px`、`1mm`→
          `3.7795275591px`、`1pc`→`16px`、`1pt`→`1.3333333333px`）。
        • **不臆造“单位不可比”错**：只有两边都在已证维度表内且维度不同时才报
          `1px and 2s have incompatible units.`（`max.hrx::error/incompatible_units`）；表外单位（如 `2q`）
          逐字推辞——未知不等于冲突。此条已有金样测试。
        • 另一个取证教训：那个 hrx 文件叫 `comparable.hrx`，但语料里的真函数名是 **`math.compatible`**，
          而 `math.comparable(...)` 那条要的是 `Undefined function.`——按**文件名**猜名字就会写错。
          `compatible`(12) / `unit`(11) / `unitless`(9) / `percentage`(10) 需 `Value::Bool`，下一批再做。
  - **第四批：`Value::Bool` + `math.unit` / `is-unitless` / `percentage` / `compatible` + 十位有效数字**。
        实测 **714 → 746**（新空库 `ledger diff prev bools` = **regress=0 net=32**，入库 run
        `bool-units-746`），实质通过 549 → **581**；`format_number` 新增“只在自然表示超过 10 位有效数字
        时才截”的规则（`0.246×100` 要印 `24.6` 而不是 f64 尾巴），因此短值与小值文本不变——实测 0 回退。
        **本轮差点臆造 8 条错误（已逐项坐实并修正）**：把 `known_call` 提到 `protected` 前面后，
        `1/1px`、`1px * 1em * 1rad` 这类**复合单位参数**进入了函数体，被 `number_arg` 报成
        `… is not a number.`——而语料要的是 `"px*em"` 这种结果。区分补上后无效错误从 63 **回到 55、新增 0**：
        “表达式还建不出来的参数（带 `/` `*`）不构成类型判定的证据”，整个调用推辞逐字。
        另外 `math.unit(1px)` 仅支持**单单位**；`em` 入维度表是因为
        `comparable.hrx::unit/to_different` 直接写了 `compatible(1px, 2em)` → `false`，
        而 `ex/ch/rem/deg` 无一对比用例 ⇒ 故意不入表（宁可推辞）。
        命名坑第二次命中：`unitless.hrx` 里的函数实为 **`math.is-unitless`**。
        工具事故（不并入库）：本批一次 `git stash pop` 撞 `index.lock` 导致工作区一度回到旧码，
        当时那组“76 passed”是旧码的读数，已当场识别并在恢复后重测为 78/0/0。

- [ ] 1.8 **`if()` 不是“一个签名就能赢”的快收益（本轮量过并否定）**。当前失败最多的文件：
      `expressions/if/sass`(48)、`core_functions/selector/unify/simple/pseudo`(44)、`css/plain/import/conditions`(42)、
      `core_functions/color/{lab/special_functions/alpha,change/hsl,rgb/one_arg/…,hwb/four_args,hsl/…}`(各 35+)、
      `expressions/if/error/raw`(34)、`directives/forward/error/syntax`(34)、`values/calculation/calc/no_operator`(33)。
      看似最便宜的 `if()` 实际卡三件事：① 语料写的是**缩进语法的 if 表达式**
      `if(sass(true): c; else: d)`（关键字参数形，不是 3 个位置实参）；② `if(sass(false): c) == null` → `true`
      需 `==` 与 `Null`；③ Sass 的 `if()` 是**惰性**的，而现在 `Expr::Call` 把所有实参先求成 `Value`，
      所以下一个真实需求是把 body 改成接 `Expr`+`Env`（只评估用到的分支）——那就得同时改掉已有 11 个签名，
      属于批尺寸改动。⇒ **推荐顺序不变**：1.4 `Calculation` → 1.4d 有理单位 → 1.7 其余分派（含惰性 `if()`）。

- [ ] 1.9 **模块常量查找(`<ns>.$name`)不是“一张表 14 条”**（本轮量完）。`core_functions/math/variables.hrx`
      看名字像是只需加 `math.$pi`/`$e`/`$epsilon`… 的常数表，实则输入多为
      `a {b: math.$e * 1e15}` → `2718281828459045`（全部乘了 `1e15`）——**卡在 `*` 运算符**（属 1.3/1.4）。
      直接可得的只有 `math.$max-safe-integer` → `9007199254740991` 与 `math.$min-safe-integer` →
      `-9007199254740991` 两条（`$max-number`/`$min-number` 要 309 位展开与 `1e300*1e39`，也不可用）。
      ⇒ 做它的正确时机是 **P5.1（命名空间）**：到时它顺带解决“`@use` 后的 `math.$pi` 报假
      `Undefined variable.`”（那 14 条就是当前 55 条无效错误的主要成分，用
      `ledger failures <run> "unexpected error"` 可数），而不是现在为 2 条判定新建一套词素机制。
      另：`error/assignment/*` 那几条要求给 `math.$pi` 赋值时报错（保留名不可写）——它需要的是赋值路径
      上的名字表，同一批做。

- [x] 1.4a **`Calculation` 的第一片（已合入）**：只做契约里那四条塌缩律，**不**碰 1.4d 的化简。
      落码：新增 `src/calculation.rs`（462 行）+ `Value::Calculation(name, inner)`（`name` 用**源码大小写**，
      只有 `lower` 参与判定 —— 1.4c 明令禁止把小写化当输出规则）+ `Expr::Calculation(Value)`（读法需要
      环境，所以它带**已求值的形**而不是 token 树，`eval` 对它是恒等）；`expr::from_tokens` 新增 `env` 参数
      并在 `known_call` 之后、`protected` 之前试 `calculation::value`（`protected` 是为 CSS 简写造的，
      会把 `calc(1% + 1px)` 永远拒之门外）；`calls::brackets`/`top_level_groups` 提为 `pub(crate)` 复用，
      **不再**造第二套括号规范化。
      塌缩律（原文全部来自 `ledger failures bool-units-746 "values/calculation/calc/no_operator"` 的
      got/want 逐项比对，非推定）：单项可塌缩值出壳（`CaLc(1px)`→`1px`、`calc( 1px )`→`1px`、
      `calc( ( 1 ) )`→`1`、`calc((1px))`→`1px`）；`calc` 套**另一个命名计算**脱一层壳（
      `calc(calc(1px))`→`1px` 两步、`calc(calc(1px + 1%))`→`calc(1px + 1%)`、
      `calc(clamp(1%, 2px, 3%))`→`clamp(1%, 2px, 3%)`、`calc(min(1%, 2px))`→`min(1%, 2px)`、
      `calc((calc(1px + 1%)))`→`calc(1px + 1%)`）；否则保壳（`var()`/未知 callee/串/插值一律不塌）。
      **塌缩只认 `Number`/`Bool`，`Raw` 永不越壳** —— 这一条边界是由该簇**已经通过**的 5 例反推钉死的
      （`ledger passed bool-units-746` 给出 `function/css`、`var/bare`、`variable/unquoted_string`、
      `interpolation/{number,parens}`），不是先写完再找理由。
      **第四次撞上「分类粒度不能超过词法」**（前三次见 0′.3b/0′.7/1.7a）：声明值里的 `calc(1px)` 到达时是
      **一个**词素 `Word("calc(1px)")`（两个括号都在里面），块尾的 `}` 才是结束符，所以闭合 `)` 常常
      **不在** run 里；我按「首个词素以 `(` 结尾」匹配、又按 `calls::cut` 把 `(` 粘到**后**一片，两版都
      **判定集完全不动**。`tracing` 打词素才看见真相，最终用本模块自己的 `cut_open`：`(` 留在左侧词素、
      `)` 独立成片、**片内空白保留**（`Word("1px + 1%)")` 里的空格是语料间距律的唯一记录，删了就出
      `1px+1%`）。另两条实测修正：括号深度必须从**整个 run** 起算，否则 `var(--c)` 的 `)` 被外层 `calc`
      抢走（`calc(var(--c))`→`calc(--c)`）；`Op('-')` 不是闭括号（`--c` 以它开头）。
      实测：**746 → 768（实质 581 → 603，空期望蒙对 165 不变）**；空库两侧复算（`before-head` 746、
      `after-calc-14a` 768）整库与 `values/calculation` 簇内都是 **`regress=0 net=22`**（簇内 fails
      897 → 875）；入库 run = **`calc-14a-768`**。22 条 = 契约四条 + 同族同律的 18 条（`number/*` 6、
      `syntax/extra_whitespace/*` 3、`calculation/{min,max,clamp}/preserved` 3、`parens::{number,calculation}` 2、
      `variable/calculation`、`function/sass/namespace`（`calc(math.round(2.3))`→`2`）、
      `interpolation/nested`、`operator/precedence::interpolation/calculation/plain`），**逐条都有 fixture 原文**。
      无效错误 **55 → 53**（少掉的两条 `no_operator::{function/if,variable/namespace}` 现在整体推辞，
      不再对外观正常的程序报假 `Undefined variable`；它们仍 FAIL，缺的是 `if()` 惰性求值与 `math.$pi`）。
      **故意没做（契约明令不得猜，留待 1.4d/P1.7）**：`calc(1px + 2px)`→`3px` 的同单位化简
      （`calc/simplify.hrx` 自己的 README 就把规则限定为 "number + number where both are compatible"）、
      `calculation/*/simplified`（需要裸名 `min`/`max`/`clamp` 求值）、`*` `/` 与有理单位、
      `calc/operator` 其余各族。**两条诚实的未达成**：`::number/exponent` 要 `1e2px`→`100px`，卡在
      `expr::number_lexeme` 不读指数记法（属 1.3，已把逐字输出钉进测试，改对它时那条必须动）；
      `::variable/number/simple_unit` 要 `calc($a)`→`1px`，卡在 `exec::evaluated` 的「裸数字暂不成表达式」
      过滤（`Expr::Num` → `None`），那是另一个决定，不在这个切片里顺手改。
      门：`calculation.rs` 10 例（含 4 条拒绝面：同单位不化简、`1/2` 仍是斜杠 list、`-a-calc(` 不是
      计算、`calc` 内未绑定变量不致命）；全工作区 **94 passed / 0 failed / 0 ignored**；clippy
      `-D warnings` 干净；`sasslint` 闸绿且 CLI `count=0`。
      **本轮顺带补的量具（因为缺读数逼人回退到手搓 shell）**：① `ledger passed <run> <簇前缀>` —— 库里只存
      失败行、且不记某 run 的测量范围，所以“这簇现在哪些是 PASS”无法回答；实测把从没测过的簇报成通过
      （`passed head-600-pathkey operators/` 返回 29 条），故**簇前缀必填**并由调用方按语料自证覆盖面；
      ② `ledger diff <a> <b> [簇前缀]` —— 验收口径本来就是**按簇 scoped**，没有簇级读数就只能
      `sed|sort|comm` 在 `/tmp` 拼中间文件（正是本 crate 要消灭的 sprawl 复发）；同时把 A/B 结论做成
      `pub fn diff_runs -> Diff{regress,net,fails_a,fails_b}` 供测试直接断言（`in_scope` 的两个计数是
      “簇内确实有用例可比”的阳性对照）；③ **`sasslint` CLI 的真缺陷**：它把 `argv[1]` 当路径，所以
      rust.md 里文档化的 `sasslint lint crates/sasspile/src` 一直在 lint 名为 `lint` 的不存在目录、
      **永远 `count=0`**（同期闸测试是红的）；现在吞掉动词、路径不存在即 exit 2，并由
      `crates/sasslint/tests/cli_directory.rs` 钉住“ planted 违规必须非 0 / 干净目录必须 0 / 不存在路径不得算通过”。

## P2 — 结构化 Selector 模型
- [ ] 2.1 `Selector`(compound/combinator/伪类)parse/序列化/`&`(`css/selector/**`、`core_functions/selector/**`)
- [ ] 2.2 unify / superselector / `selector.nest/parse/unify/is-superselector/simple-selectors`

## P3 — 管线 + 控制流 + 作用域 + 媒体/at-root
- [ ] 3.1 `async fn compile` 唯一出口 + **一条冷流串起各相内核**（`Lexer→Structurize→Exec→[全局 pass]→Render→reduce_initial→into_future`）。
      ⚠ v2 口径已改：旧形态（`scan_map`+`flat_map(from_iter)`+`sort_by_key`）**不再合格** → 由 P0′ 重做，本项随之重开。
- [~] 3.2 `@for` 字面整数界限——**已落第一个切片（本轮）**；`@if/@else`、`@each` 仍未实现。
      先破掉一个**假完成**：原条目写“已做 `@each/@for/@if`”，那是从已删除的 v1 流水线带来的旧状态——
      代码里 `@for`/`@each`/`@while` 只出现在注释；该簇 39 条“PASS”**全是空期望蒙对**，实质通过 0（已写进
      `rust.md` 工具红线：声称特征能用前 MUST 看该簇非空期望通过数）。
      实现：`exec::for_loop` 在**词素层**只认 `@for $v from <int> through|to <int>`（六词素恰好形），
      循环体声明作为 `ForLoop.body` 数据挂在开块上，`Close` 时 `values.try_for_each` 逐个绑定变量并重放
      到**外层待定声明段**（循环不是 CSS 子块：不开新规则、也不冲刷父段）；`sequence` 按金样定
      `through` 含端点/`to` 不含/倒序递减/空区间不回包。`declare` 从 `step` 里抽出来供两处复用。
      实测：**8 条新通过**（`for.hrx` 的正向族 + `empty/scss`），`directives/for` 由 20/40 → 28/40；
      无效 `Undefined variable` 64 → **55**；新空库重跑两侧后 `ledger diff prev for-loop` =
      **regress=0 net=8**；基线 635 → **643（实质 471 → 478）**，入库 run `for-loop-643`。
      `tests/loops.rs` 钉住金样与边界（变量界限/浮点界限**不猜**→ 仍报未绑定，而语料要的是
      `1.5 is not an int.`，那属 P7）。
      **已记账的未完面**：3.2b 体内嵌套块（需每轮发射**规则**，现在被捕获后丢弃）、3.2c 界限为变量/表达式
      （`to_scope`：还需“循环开始前一次性求值界限”与循环变量作用域）、3.2d `@each`（入基线用例为 0，
      不做无法度量的特征）、`@for` 错误消息族（6 条，归 P7）。
- [ ] 3.3 `@while` 条件循环(`directives/while/**`)——同 3.2：簇内 1/1 “PASS”也是空期望，未实现。
- [ ] 3.4 **媒体/supports 冒泡 + 逐查询合并**(全局 pass;`css/media/**`)
- [ ] 3.5 `@at-root (without/with)`(`directives/at_root/**`)
- [ ] 3.6 变量块作用域 + `!global`/`!default`(`variables/**`、`libsass/variable-scoping/**`)
- [x] 3.6a **旗标不是值的一部分（已合入，3.6 的一个子面）**：`lexer::without_flags` 在**绑定**路径上剥掉
      值尾部的整段 `!default`/`!global` 旗标游程（`Token` 层判据：`Word` 以 `!` 开 + ident 字符，不看渲染后的串）。
      依据：`variables/double_flag.hrx::default`（`$a: b !default !default;` → `d: b`）与 `::global`。
      **只剥绑定值**：普通声明/自定义属性的 `!important` 是 CSS 文本（`css/custom_properties/exclamation.hrx`
      在基线里就是 PASS，作为反向保险写进 `tests/flags.rs`）。**故意不做语义**：“已绑定则 `!default` 不覆盖”
      与 `!global` 的提升作用域不在这里——前者唯一直证在多文件的 `directives/use/with/*`（harness 暂跳过），
      归 3.6/P5.1；现在 flat env 下两者可见结果相同，两条金样已过。实测：`ledger diff head-633-concat head-635-flags`
      = **regress=0 net=2**（就这两条），基线 633 → **635**（ledger run `head-635-flags`）。

## P4 — 可调用:mixin / function / @content / 参数绑定
- [x] 4.0 **声明不求值体（已合入，P4 的前置）**：`Scope` 新增 `inert`（与 `plain_css` 一样向内继承），
      `callable_declaration` 在**词素层**认出 `@mixin`/`@function`（关键字与名字各为一个 `Word`，关键字大小写
      无关），命中则该体：不求值声明、**不向外层环境绑定参数**、不出规则；又因为它不是 CSS 子块，
      开它时**不冲刷父块待定声明段**（`a { b: c; @mixin m {…} d: e }` 仍是一条规则）。
      **边界（判据必须排除 `--` 开头名字）**：`@function --a()` 是 plain-CSS at-rule，它的体**就是要输出的 CSS**
      （`css/function.hrx::lowercase/result/sass_script` 现在在用，误判就会删掉已通过的规则）——
      `tests/callables.rs` 同时钉住“声明不出规则”与“plain-CSS 仍出体”两侧。
      实测：无效 `Undefined variable` 行 **93 → 64**；判定集合不动（新空库重做一次两侧后
      `ledger diff base635 inert-bodies` = **regress=0 net=0**，入库 run = `inert-callable-bodies`）。
      **不说过头话**：那 29 条仍不转 PASS——它们下一步需要 `@include`/调用求值与 `meta.*`（本条只清掉
      “假错打断整个程序”这一层）；第 4 条金样的“拆块”行为是由两条规则推论而非 fixture，已在测试里标明。
      剩下的 64 条已重新归属：`core_functions/list` 25 + `math` 9 + `meta` 2 属**模块未加载**（P5.1），
      `directives/for` 14 属**循环变量未绑定**（P3.2），`expressions/if` 10 属分支/短路（P3）。
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

- [x] 9.0 **本地诊断面（OpenTelemetry + 单用例选择器）（已合入）**：工具性改动，不是语言特征，也不是收尾项——
      它是让每一轮验收与定位变便宜的基础设施。**起因是实测事实**：1.4a 里我两次写了匹配不到任何东西的 callee 判据
      （词素粘着括号，见 1.4a 段），两遍“判定集完全不动”都看不出原因，只能临时造探针文件 + 删。
      **形态**（经三步选择确认：目标=本地诊断 + 补埋点；接入点=测试与两个 CLI 都接；粒度=连词素现场一起接）：
      • `trace` feature（三个 crate 各一个）：用 `tracing-opentelemetry` 桥接现有 span，导出器是
        `opentelemetry-stdout`，**无 collector 无网络**；`sasspile` 的 OTel 依赖全在
        `[dev-dependencies]`（库埋点只用 `tracing` 宏），两个 CLI 因 bin 看不到 dev-deps 而用 optional 依赖。
        实测：`cargo tree --edges normal` 对三个 crate 均为 **0 条 opentelemetry**；
      • 新增 `crates/sasspile/src/trace.rs`：`Phase`（相）与 `ValueRead`（一条声明的值读）。
        **顺手修一个真缺陷**：四个 phase span 原本在 `subscribe` 里 `enter()`，函数一返回就关闭 ——
        `elapsed_ms` 量的是接线而不是流线（实测整个输入推完的相只有 ~0.3ms）；现在 span **住在观察者里**，
        随终止关闭（`items`/`elapsed_ms` 在 `record` 里写），并把整数 ms 改成**带小数毫秒**（小相全报 0 会伪装成“瞬时”）；
      • 裁决现场：`calculation::value` 一条 DEBUG 事件带 `arrived`（到的词素原文）/`cut`（切完的形状）/
        `decision`（collapsed/preserved/shell dropped/declined）/`resolved`；`calls::known_call` 记
        `callee`/`args`/`signature`。这两行就是“这条声明为何逐字输出”的答案，不再需要探针；
      • harness 新增 **`HRX_CASE=<key 子串>`**（全量 11,533 条不是诊断视图，所以必须能只跑一条），并带两条守卫：
        与 `HRX_RUN` **互斥即 panic**（部分测量入库会让 `run.total` 冒充全量，下次 diff 拿着不可比的数字比），
        以及**打不中即响**（否则“0 of 0”看着像一次干净的空跑）；失败行另存一份 got/want 事件；
      • `ledger` 的 main 原本**一个 span 也没有**（OTel 只导 span 不导 event），所以“开了 trace 的空跑”会导出
        空链路 —— 现每次调用一个根 span（带 `command`/`db`）；`sasslint` 已有 `path` span，直接接上。
      **实测**：一条 `no_operator::number/integer` 用例导 **6 个 span**，并出
      `calculation read … arrived=calc(calc(1px)) cut=[Space, Word("calc("), Word("calc("), Word("1px"), Space,
      Op('+'), Space, Word("1%"), Punct(')'), Punct(')')] decision="preserved" resolved=calc(1px + 1%)`；
      **埋点不得改行为** —— `ledger diff calc-14a-768 trace-instr-768` 整库与 `values/calculation` 簇内均为
      **`regress=0 net=0`**（fails 10765→10765 / 875→875），**阳性对照**同一条命令对 1.4a 前基线报
      `net=22`（证明那个 0 不是坏查询）；门：无 feature 全工作区测试绿，`--features trace` 下
      `ledger`/`sasslint` 各有一条测试断言“**span 真被导出且原有读数照旧印**”（一个永不导出的 feature 和一个
      吃掉读数的 bridge 都是静默失效）；clippy `-D warnings` 两种模式均净；`sasslint` 闸绿。
      **不做的**：不把全量 hrx 当诊断入口（你指出的：“不可能全量 hrx，那太大了”）；不给 `sasspile` 加诊断 CLI
      （“库只暴露 compile”不变，入口是 `examples/trace_pipeline.rs`）；`opentelemetry-stdout` 逐 span 打印
      （带 `parent` id，可自行串树），而不是等根 span 关闭才吐整棵树。

- [ ] 9.1 `element-plus` theme-chalk 端到端编译(**经 P1/P3/P4/P5/P7 主干即应达成,不依赖 P6**),无 panic、输出与自基线一致不劣化(`ledger` 归一化)
- [ ] 9.2 全量 `sass-spec`(排除 `non_conformant`/`libsass-*`/`:todo:`)通过率相对基线单调上升、`regress=0`;`sasslint` 0;clippy `-D warnings` 绿
