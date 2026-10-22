# Design

## Context

既有 rxrust 资产即范式锚：`pipeline.rs` 五阶段 `flat_map` 衔接、`drain.rs` 单一 mpsc 终端、`lexer.rs` 的 `scan_map` 流迁移器 + LexPunct 派宏、`branch_fol`/`@if`/`@each` 的 in-stream `Result` + `Err = Infallible` 协议、`move-only` 的 `LocalBoxedObservable` 类型擦除。本设计解决的是**把同一形态律推广到叶层**（~323 处 `for`/`while` + 条件堆叠），而非引入新框架。版本 pin 不动（`rxrust =1.0.0-rc.5`，Local-only 上下文，wasm 契约）。

## Decisions

### D1 算子对照表（命令式 → rxrust 的机械翻译规则）

| 命令式源形 | Observable 目标形 |
|---|---|
| `for x in xs { out.push(f(x)) }` | `Local::from_iter(xs).map(f).collect::<Vec<_>>()` |
| 循环内 `if cond { push }` | `.filter(..)` / `.filter_map(..)` |
| 累加/拼接（`let mut acc`） | `.scan_map(seed, \|st, x\| ..)` 或 `.fold_observable`/`reduce_initial` |
| 字符状态机（depth/标志位） | `.scan_map(State, \|st, c\| st.step(c))`（`&mut acc`，无 Clone 开销） |
| `while` 索引扫描取到边界 | `.scan(..).take_while_inclusive(..)`/`take_while` 组合 |
| 循环内 `match` 产出多元素 | `.flat_map(..)`（每元素→子流） |
| `for` 内提前 `return Err` | in-stream `Result` + `.take_while`/终点 `?`（聚合错误协议 R1） |
| `if/else if` 值链 | 单表达式 `match`（enum 分派，R3 例外①）或组合子链 |
| 嵌套 `if` 守卫 | `.filter(..).map_or(..)`/`and_then` 链 |

同步性不构成改用 std 链的理由（用户裁决）：全库只允许一种流形态，`from_iter` 的订阅在 `drain`/`collect` 下全量 inline 执行，无调度器参与、无运行时依赖（见 D4 开销分析）。

### D2 收束终端：叶层用 `collect`，跨阶段用 `drain`

规则：产物为**值**（Vec/String/Option）的短链以 `.collect::<C>()`（rxrust 的 collect 算子）终结；跨函数边界/多阶段衔接的流仍 `box_it()` + 单一 drain 终端。禁止在 Observable 链外再包 `subscribe` 手工收值；`to_future`/订阅终点仅 `drain.rs` 保留一处。

### D3 批次边界与文件清单（风险升序）

- **S1 叶层纯函数**（无行为面，最廉价验证）：`css.rs`（tidy/bogus/split/strip 族，10 循环 21 mut）、`selector_unify_complex.rs`、`selector_unify.rs`、`builtins/selector.rs`（18）、`plain_css.rs`（11）、`value_number.rs` 等其余 ≤5 循环小文件清零。
- **S2 eval 参数/内置层**：`eval_bind.rs`、`eval_meta.rs`、`builtins/color_*.rs`、`eval_color*.rs` 中循环（多数已是链式，清残余）、`eval_control.rs`（12）。
- **S3 extend 引擎**：`eval_extend.rs`（18）、`selector_extend*.rs`（22）、`extend_plan` 族。
- **S4 parser/lexer**：`parser*.rs`、`lexer_detail.rs` 游标 `while` → `scan_map`/组合子；**允许降级**：不可行者按 R3-④ 登记豁免（sealed 纯函数形态，函数体内 mut 合法）。

每批内部再按文件切 commit，保持单 commit 可回退。

### D4 开销与 wasm 边界

`Local::from_iter` 为冷流、订阅期同步 inline（无 scheduler/无堆调度），逐字符链的分配成本 = 算子闭包 + Vec 组装：与 std 迭代器等价；相对 `for+push` 无渐近退化。`String` 收集用 `collect::<String>()`（`Extend<char>`），不保留 `with_capacity` 手工微优化（契约优先，需要时在豁免评审中重议）。`LocalBoxedObservable` 擦除仅用于跨函数签名（既有约定），链内不擦除以保留内联。

### D5 门禁工具：`style_gate.rs`（Rust 测试，非脚本）

`tests/style_gate.rs`：对 `src/**/*.rs` 做 AST 无关的行级扫描（剔除注释/字符串后匹配 `for `/`while `/`loop `、`else if`、`let mut` 计数），与 `tests/golden/style-baseline.txt` 基线比对：只降不升，S1-S3 目标每批清零所辖文件，S4 结束基线冻结为「仅豁免清单」。同时 crate 顶挂 `#![deny(clippy::needless_range_loop, clippy::explicit_iter_loop)]`。报表即 Requirement.5 的可验证化。

### D6 与 converge 主线的穿插协议

批准生效即双轨：converge 新切片按本契约书写（其 e2e 绊线 + 全量 A/B 闸不变）；存量清洗 S1→S4 穿插其后，**单轮只动一边**（R6）。每批开工前先跑一次基线快照 tsv（既有 A/B 工具链），确保对拍两侧只差重构 commit。

## Risks / Trade-offs

- [S4 游标不可全数组合子化] → 预留 R3-④ 降级路径，不追求纸面 100%；豁免清单显式留痕可审计。
- [逐字符 Observable 的可读性劣于显式循环] → 用户裁决明确（风格契约优先于局部可读性）；以算子命名（`step`/`feed`）与 D1 表对照缓解。
- [重构 commit 与收敛 commit 混叠污染归因] → D6 协议 + R6 违例拆分条款。
- [热路径（如 `normalize_selector` 每规则调用）链式化后性能回退] → `from_iter` 同步 inline，发布前以既有大 fixture（element-plus 全量编译）计时抽查，不设性能预算红线（现状非性能敏感）。

## Open Questions

（无——两项边界裁决已闭环：严格字面 rxrust；enum 单表达式 `match` 保留为例外①。）
