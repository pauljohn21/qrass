# Design

## Context（背景）

`refactor-rfp-strict` 收掉了"看得见的缺口"（`for`/`while`/`loop`、`else if`），但决策口径是"把它弄出
裸循环"——于是常常落到 std 迭代器链上、把 `&mut self` 走查当"合法载体"留着、把游标判成"不能是
Observable"。用户的裁定把目标重新校准：**rxrust 的内核是所有权**。流是一条**移动 owned 值**的计算；
它不借用共享可变状态，也不需要 GC 式对象图。本设计把剩余命令式 / std 链残余翻译进该模型，保持行为
逐字节不变，并让强制器能看见 std 链，从而把欠账棘轮到零。

仓库里已有的锚点证明了所需每种形态都可行：`lexer.rs`（`feed` 逐字符 → `scan_map` 状态迁移器 +
`LexPunct` punct 分派）、`parse`（逐 token owned 折叠 + `flat_map` drain）、`pipeline.rs`（五阶段
`flat_map` 衔接）、`drain.rs`（唯一 `drain`/`drain_one` 终端、in-stream `Result`、无运行时）、
`color_transform.rs` C5（本会话：`reduce_initial` 携带 owned `Color4` + latched 错误，输入被 move 进
`'static` 闭包）。算子真值以 `rxrust-reactive` skill 为准（`Item` 按值移动；`map`/`filter_map`/`flat_map`
拿值的 move；`filter`/`tap` 拿 `&Item`；`scan_map(&mut acc, item)` 累加子无需 `Clone`；`reduce_initial`
归约单值）。

## Decisions（决策）

### D1 命令式 → owned-累加子 Observable 的机械映射表

| 命令式源形 | owned 流目标形 |
|---|---|
| `for x in xs { out.push(f(x)) }` | `Local::from_iter(xs).map(f).collect::<Vec<_>>()` |
| 循环内 `if c { push }` | `.filter(..)` / `.filter_map(..)` 后 `collect` |
| 累加/拼接（外部 `let mut acc`） | `.reduce_initial(owned_acc, \|acc, x\| ..)` 取单值 |
| 逐元素带状态（字符状态机、计数、缓冲） | `.scan_map(owned_seed, \|&mut acc, x\| out)`（累加子被 move，无 `Clone` 开销） |
| std `try_fold`/`for_each` 作有状态折叠 | 改 `reduce_initial`（单值）或 `scan_map`+`collect`（多值），错误经 latched `Option<E>` |
| 循环内 `match` 产多元素 | `.flat_map(\|x\| Local::from_iter(..))` |
| 循环内提前 `return Err` | 累加子带 latched 错误；或 `.map(..)` 发 `Result` 项，由 `drain` 首错短路 |
| `if/else if` 值链 | 单表达式 `match`（enum/元组），各臂内构造值/错误 |
| 变步长游标 `while` | `Local::from_iter(inputs).scan_map(State, \|st, i\| st.step(i)).flat_map(emit)`（D3） |

同步与否从不改变这张表——叶层也走 `Local::from_iter`。装箱（`box_it()`）只用于跨函数签名或喂 `drain`；
链内不装箱以保留 inline。

### D2 `drain` 是所有权收口，不是"rxrust 缺口的补丁"

`drain` 的职责是把一串被推出来的 `Result<T,SassError>` 项**移交**回调用方：`subscribe(move |x| tx.send(x))`
里塞的是一个 owned、`'static` 的 `mpsc::Sender`（正因 rxrust 订阅 observer 需 `'static`、借用不了栈上
`Vec`，才用 owned channel 跨这道 `'static` 界），再用栈上 `rx.iter().try_fold(..)` 拉出并首错短路。非定时源
在 `subscribe` 期间 inline 跑完，所以无需运行时即同步完成（守 wasm/`Local`）。`tap` 是中段副作用、非终端，
取不回值；`collect` 算子仍需一个终端且不懂"错误在值通道时首错短路"。结论：链到头只允许经 `drain`/`drain_one`
落地，`drain.rs` 是全库唯一 `subscribe`/future 终点。

### D3 游标 = 逐输入状态迁移器（`lexer.rs` 先例）

以 `simple_selectors` 为例（其余游标同法）：把"当前 token + 位置"放进一个 `struct Scan { out: Vec<String>,
cur: String, .. }` 的 owned 累加子，`Local::from_iter(chars).scan_map(Scan::default(), |st, ch| st.step(ch))`
中 `step` 按 sigil/引号/括号边界把完成的 token 放进 `st.out`（每输入 0..N 个输出），链尾 `.reduce_initial`
或直接取累加子的 `out`，`drain`/`drain_one` 收口。判定标准：**输入逐个进、状态在累加子里被 move 着走**，
不出现 `while i`/`for k in ..`/外部索引。若某个跨方法递归下降（如 `parser_if`）用纯 `scan_map` 会破坏
`Parser` 的 `'static` 装箱约束，则退一步：把该次解析做成一段以 owned 子流 `flat_map` 衔接的阶段，而不是留
`loop`。确有不满足"逐输入 owned 迁移"且强行改造会改行为者 → 走 D6 上报。

### D4 std 链检测维度（`style_gate.rs`）

新增第五维 `std_chain`：掩码注释/字符串后，对每文件计"数据流 std 链"命中——启发式取**链尾收口算子**
（`.collect()` / `.fold(` / `.try_fold(` / `.for_each(`）且其前存在 `.iter()` / `.into_iter()` /
`.iter_mut()` / `.map(`/`.filter(`/`.filter_map(`/`.flat_map(`/`.take(`/`.skip(`/`.chain(`。刻意**不**计：
单值 `Option/Result` 组合子、`matches!`、`.iter().any/all/find/position/count` 这类返回标量谓词（它们不"重建
一个集合"，非数据流替身；若日后要严也可加）。命中写进 `tests/golden/std-chain-baseline.txt`（只降不升），
`QRASS_STYLE_GATE_BLESS=1` 刷基线。检测器是粗筛、允许保守，目标"让欠账可见并单调收敛"，不是替代人工评审。

### D5 批次顺序与文件清单（风险升序）

- **B1 叶层纯 std 链**（无行为面）：`css.rs`、`value.rs`/`value_number.rs`、`builtins/*`、`error.rs`、
  `span.rs`、`selector_*.rs` 中 `.iter().map/collect/fold` 构集合者。
- **B2 有状态 std 折叠**：现存 std `try_fold`/`for_each` 的累加（分组、first-wins、去重、校验矩阵）。
- **B3 自包含游标**：`builtins/selector.rs simple_selectors`（纯、hrx 覆盖）、`lexer_detail.rs`、`parser_if.rs`。
- **B4 `&mut self` 求值/loader 走查**：`loader.rs` 候选解析、`eval_*` 语句走查、`eval_config` offer/forward、
  `eval_color_space_symbolic` slot 折叠——把被改状态穿成 owned 累加子。
- **B5 CLI**：先补 `tests/cli.rs` 测试台，再改 `main.rs parse_args` 为对 owned `Vec<String>` 的折叠。

每批内逐文件 commit，单提交可回退；每批前取一份 hrx 基线快照做 A/B 对照。

### D6 不可表达 = 上报协议

某构造若经尝试仍无法在不改行为、不引 `else if`、不留外部游标的前提下等价表达：暂停该文件，写
`reports/rxrust-blocking-findings.md`：最小复现输入、所卡的所有权/`'static`/递归约束、已试的流形态与
失败原因、当前行为对照。**不登记豁免、不悄悄留循环**；由用户裁决下一步（改设计 / 接受为已知限制）。

## Risks / Trade-offs（风险与取舍）

- [逐字符 Observable 可读性 < 显式循环] → 用户已裁：基底原则优先于局部可读性；用 `step`/`feed` 命名 + D1 表缓解。
- [热路径链式化性能] → `Local::from_iter` 同步 inline，成本与 std 迭代器等价；不预置性能红线，收尾以 element-plus 全量编译计时抽查。
- [std 链检测器误报/漏报] → 检测器是粗筛，宁保守；误报以"基线只降不升 + 人工确认"兜底，漏报由"零豁免 + 上报"精神约束。
- [游标改造触碰金样敏感内核] → 每批全量 A/B `regress=0` 硬闸，失败即回退；先做纯且覆盖强的（`simple_selectors`），难的下沉上报。
- [归因混叠] → D5 单轮一边 + R6 违例拆分。

## Open Questions（待定）

- std 链检测维度是否要把 `.iter().any/all/find/position` 也纳入严口径（当前排除）——待 B1 落地后按信噪比定。
- `parser_if` 递归下降若 `scan_map` 装箱受 `'static` 阻，是否接受"改写成 `flat_map` 衔接的多阶段子流"作为等价目标——倾向接受，D6 兜底。
