# 反例索引（动手前逐条 grep；命中 = 停手先重构根，再谈加功能）— 从 `.qoder/rules/rust.md` 整段迁入，此处为唯一副本

> 由 `.qoder/rules/rust.md` 迁出（2026-10-28）。

## ⛔ 本会话新踩的两个形（上一版索引里 grep 不到，所以它们能反复成立）

**形一：第二趟推翻第一趟 —— “先产出值，再拿另一个判据决定这个值算不算”**

形状签名（可 grep）：`let mut value` + 多个 `if { value = … }` / `.take()` 当可选槽位 / `Err` 里装非致命
控制流。实例：同一个实参集跑两遍——第一趟逐字求值，第二趟发现“其实含未竟算术”后改判。后果不是
“静默成功”，而是**实参顺序决定金样法定的错误要不要上报**：`abs($nope, 1px + var(--c))` 报
`Undefined variable`，而 `abs(1px + var(--c), $nope)` 把错吞成逐字。判据必须是**一次读取成数据**
（在选支处一次定死），不能是值已存在后的回头覆写。
自查：写完一段呼叫/实参读取后问一句——“交换两个实参的顺序，错误会不会换形状？”会 = 就是此形。

**形二：折叠律未按 arity 收口 —— `find_map` 把 N 元集合的第一个元素当成整体的值**

形状签名：`find_map` / `.first()` 在一个“本来可以是 N 元”的结构上取首个带值元素，并当作整体值使用。
实例：`calculation::folded` 对多参计算壳取第一个实参当值 ⇒ `math.div(1, min(10, 20))` 出 `0.1`、
`max(10, 20) * 1` 出 `10`。两条使之隐蔽：① 语料**没有**把多参壳放进算术的行，所以 `regress`/`net`
全看不见；② 它看上去像“算子写得不对”，实际是**数据形状少了一个 arity 条件**。
修法：值类谓词必须带 arity 守卫（单元素才有值，多元素只能有文本），并且把同一守卫用到**全部构造点**
（本例两处：非 `calc` 壳与 `calc` 的多参支）——只改一处就是留下另一处的同款错。

**关联红线：“收益”不能给结构背书**：本轮一个已撤销的探针 run 记下 1858，**比当时基线高 4 行**，
而这 4 行里有两个名字会产出与金样相反的值。拿净数当理由就是拿分数给 GC 背书。
（库里的 `abs-sign-rem` 就是这个样本；现行基线名与数值只看 `.qoder/rules/rust.md` 基线段。）

## 🧾 反回归自查（动手前逐条 grep；命中 = 停手先重构根，再谈加功能）

**⚠️ 不得引用未出现在工具输出里的数字与动作（最高级红线）**

实证（同一会话内两次）：① 编译失败（`E0599`，无任何 measured 行）的那一轮，我把验收写成了
“净 21 / 1085 / 实质 920”；② 下一条消息里我又宣称“已提交已推送 `a2370a3..c53180f`”，而当时
**既没跑 commit 也没跑 push**（`c53180f` 不是仓库对象，HEAD 仍是 `a2370a3`）。两者都是把“应当如此”
的结果当成已发生的结果：比错读数字更严重，因为它会把未验证的改动当成已入库。
要习性约束：**汇报里的每个数字必须能在上一条工具输出里逐字找到**（measured / AB diff / `--count` / 退出码）；
**声称已提交/已推送时，哈希必须来自 `git log` / `git push` 自己的输出**；若只是计划要做，就说“打算”不说“完成”。

**新增一个名字/能力时，先 `grep` 它在 `tests/` 里的旧断言（已实证五次）**

“因为某名字未实现，所以它保持逐字”这类断言，在实现落地时**前提就没了**，不翻就会把测试当失败。
实证（已五次）：`tests/channel.rs` 的 hwb 逐字列表、`tests/builtins.rs` 拿 `color.hwb` 当“未注册例子”、
`tests/named_args.rs` 只要求“文本存活”（现在按 `output.css` 断言，更强）、`tests/color_change.rs` 把已实现的
`alpha_arg` 列在 decline 清单里、以及本轮我**自己编了两条不存在的 fixture 行**当作“仍红”的证据。
**注释里的金样同样是编造的高发地**（第六次，本轮）：`color.adjust` 首版的 doc 写了 `color.adjust(#abcdef, $red: 500)`
→ `#ffcdef`（fixture 是 `$red: 200`）与“hsl 轴夹在 0–100”（那是 **legacy `saturate`** 的律；8 条 `adjust/hsl` 行一直反对，
而其中 `saturation/max_remaining` 当时就是红的 —— 注释把失败行为说成了规范）。
做法（写进注释前）：**把某行写进注释/文档前先 `sed -n '/^<===> <case>\/input/,/====/p' sass-spec/spec/.../<file>.hrx` 逐条复核输入与输出**，
不得凭“同族函数应当一致”去推断。**第七次（同一回合）：编了个不存在的 case 名**
`scale/error/type.hrx::alpha_none`（真名就是 `none`）—— **case 路径与例名也是字面量**，引用前
`grep -oE "^<===> [a-z_0-9]+/input.scss" <file>.hrx` 先把名子列出来再写。
做法：**开工前先 `grep -rn "<新名字>" crates/sasspile/tests`**，把命中的旧断言一并翻成 fixture 自己的期望，
而不是等到测试红了才改。另：`cargo test --workspace | grep … | awk` 在**第一个失败的二进制处就停**，
会报出“0 failed”的假绿；门禁一律用 `--no-fail-fast` 并看 `exit=0`。
- **树/递归**：`Vec<Rule>`、`children`/`.children`、`fn …->Vec<..>` 中转、render/eval 自调用 → 平展 `Vec<Leaf>`，`map/flat_map/filter`。
- **批量外壳**：`scan_map` 返 `Vec` + `flat_map(from_iter)`、把 body 收集后重放 → **每相写成真算子内核**
  （`struct Phase{source}` + `PhaseObserver{observer,state}` + `ObservableType`/`CoreObservable<C>`），
  在自己 `next` 里逐条 `observer.next(..)` 发 0..N 项；状态住 Observer、`Err` 保持泛型、不写死 `Rc`/`Arc`。
  现存实例：`@for` 的 `exec::replay`（区间已不物化——`loops::Walk` 只携带三个数；扇出也已在发射层成立
  ——`step` 接 `Emit` 汇口，体内含块时每轮一条规则，金样 `for.hrx::to_scope` 已绿）；但 body **仍是收集后重放**，
  真 `flat_map`（一项→body 子流）需 `Structurize` 把 body 当一个 owned 值上交——那是阶段决策，不自造机制（`tasks.md` 9.7）。
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
  任何字符串启发式都是错的（已三次实测推翻）。同理：**“去掉括号内空格”也是同类粒度陷阱**——
  `calls::split` 曾经为了找被调者而把整个 run 的 `Space` 滤掉，使 `rgb(18 52 86)` 到 body 时只剩
  `[Word, Word, Word]`，现代一参色语法在下游**根本无法**识别（第四次实证）。正确做法：只在**头部**
  找被调者，括号内布局原样保留（只去边沿：`trim_layout`）。同理：行注释结尾的换行要是 `Space`而不是 `Word(" ")`，
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

