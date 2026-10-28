# 跨会话防 GC 回归的防线设计（sasspile）

日期：2026-10-28 状态：**待你批准**（本文件只做设计，未动任何配置/代码）
适用范围：`crates/sasspile` 及其铁律（`.qoder/rules/rust.md`）

## 1. 问题的实测形状（不是"忘了规则"）

`rust.md` 是 always-on，新会话照样加载全文；本轮仍在同一会话里出错 **9 次**。所以失效点不是"规则没
送到"，而是下面六类。任何防线必须先对上号，否则就是又写一遍没人读的教条：

| 编号 | 实测失效 | 本轮样本 |
|---|---|---|
| F1 | 把**陈旧摘要/旧 dump 的记忆**当证据 | "指数的打印侧"候选（实地 grep 后：语料一处也没有）；"precision 108 行"（把 `values/numbers` 前缀数当子目录数，真值 30 例） |
| F2 | **伪造字面量**：金样文本、case 名、测试输入 | `$red: 500`（真值 200）、"hsl 轴夹 0–100"（是 legacy 的律）、case 名 `alpha_none`（真名 `none`）、`math.hypot(3, 4)`（语料无此行）、`tan(math.$pi / 4)`（真行是 `0.5rad * math.$pi`） |
| F3 | 把**"我的选择"钉成语料律** | `tests/builtins.rs` 拿 `math.max(1px, 2q)` 当"表外单位 decline"的例子，`q` 一进共享表立刻红 |
| F4 | **结构退回 GC/命令式**（铁律本体） | 历史上的 `Vec<Rule>`+递归、`from_fn`+`VecDeque`、`Stage/fail/drain`、`scan_map` 返 `Vec` 的批量外壳；本轮新形态：两条打印律写在两处（`calc_form` vs `calc_product`）、换算表两份 |
| F5 | **读数来自不产生它的命令** | `unit.rs` 编译错未入眼，却把旧 run 的 `net=179` 当新读数（真值 188）；`--include=input.scss` 在 hrx 归档上必然返回 0，差点得出"无 cheap win" |
| F6 | 长会话尾部**判断力衰减**（锚定 + 上下文将满） | 同一轮里既写了正确的 `match (space, writing)`，又把 scale 的金额用 `byte()` 读 |

结论：**F4 可以确定性机检；F1/F2/F3/F5 是"证据纪律"，只能靠"把真值变成一次工具调用 + 把调用变成闸门"来防；
F6 只能靠"换上下文重审"来防。** 由此得到五层。

## 2. 设计原则（三条，不可协商）

1. **"我记得"不是证据。** 只有工具输出算证据；任何数字/文本/清单都必须由本会话内的某次调用产生，并被逐字引用。
2. **单一真源。** 基线数字只存 `reports/hrx.db` 的具名 run + `rust.md` 头部；`README.md`/`design.md` 只放指针，
   本轮已按此把 `design.md` 里的 `635/11,533` 换成指针（写数字就会漂）。
3. **写者与审者不同上下文。** 防 F6 的唯一办法是让**新鲜上下文**（子代理/新会话）用同一份判据重审，
   且只允许引用文件与金样行作证据。

## 3. 五层防线

### L1 `guard` MCP server —— 把真值做成一次零成本调用

新 crate `crates/guard`（bin `guard-mcp`，复用 `sasslint` 的规则与 `ledger` 的读数，不新写第二套逻辑、
不开第二个 sqlite）。工具集：

| 工具 | 返回 | 治哪个 F |
|---|---|---|
| `baseline{}` | 当前 run 名、`total/pass/pass_substantive/pass_empty_expectation/pass_rate`、无效错误计数、门禁命令清单（全部现读：`reports/hrx.db` + `tests/hrx.rs` 的 `SPEC_DIRS`） | F1 F5 |
| `verbs{need}` | 在**钉住的** rc.6 源码（`$CARGO_HOME/registry/…/rxrust-1.0.0-rc.6/src/{observable,factory}.rs`）上 grep 出的算子/源清单命中情况；无命中 → 返回固定文案"框架无此动词 = 阻塞项，上报，不许自造机制" | F4 |
| `fixture{case}` | 从 `.hrx` 归档按 `<===> <case>/…` 边界切出的 **input/output/error 原文**（含首行 `Error:`），并给出文件与偏移 | F2 |
| `gc_scan{path}` | `sasslint` 规则 + 结构度量命中（见 L2 清单），按文件:行输出 | F4 |
| `claim{run, expects}` | 断言"这些 case 必须 PASS""这些必须仍 FAIL 且原因是 X""该增量 net=+N"，返回违反项；不满足 exit≠0 | F1 F6 |
| `report{base, run}` | `ledger diff` + 无效错误两侧读数的**一段可贴进 commit message 的文本** | F5 |

启动自检：`guard` 校验它 grep 的 rxrust 目录名与 `Cargo.lock` 里的版本一致，不一致直接报错退出
（防"按别的版本的 API 写码"——本仓库已因 0.15/rc.6 差异返工过）。

### L2 机器闸：编辑即检，且规则本身要有阳性对照

- **扩 `sasslint` 规则**：写本节时我按 `rust.md` 里“四条铁律机器闸”的说法以为只有 4 条，实地读
  `crates/sasslint/src/lib.rs` 后真实是 **12 条**（`no-tree`/`no-relay-vec`/`no-string-machine`/
  `no-recursive-descent`/`no-from-fn-driver`/`no-work-queue`/`mutable-accumulator-drives`/`no-sort-for-order`/
  `no-batch-shell`/`require-err-channel`/`require-slot-before-subscribe`/`no-imperative-loop`）。
  所以只补真正缺的**三**条（已覆盖的形状不重造——本设计自己的“单一真源”原则同样适用于它自个体量）：
  `no-stage-layer`（`type Stage`/`fn drain`/`fn fail` 自造框架层）、`no-else-if-chain`（`} else if` 链代替 `filter`）、
  `single-table`（同一事实的第二张常量表：文件里出现 ≥4 个单位字面量的 `match` 即报，`unit.rs` 那张真源除外），
  全为确定性文本/结构判据。
- **规则的规则**：每条新规则必须配一个 `tests/` 内的**植入反例**测试（内嵌 fixture 字符串，断言命中）与
  一个**合法形状不误伤**测试（现有 `legal_stream_shapes_are_not_flagged` 的下界做法）。没有阳性对照的规则不许合入
  ——本轮两次"为 0 的读数"都是量具自己坏了。
- **触发点**：每次编辑批次后必须跑（宿主若支持 PostToolUse hook 则自动跑并把结果注入上下文，见 L5）；
  提交前 `cargo test -p sasslint` 与 `cargo run -p guard -- bin report <base> <run>` 的 exit code 是唯一凭据。
- **不改 git 配置**：`core.hooksPath` 一类属宿主/用户设置，**需你明确点头**才动；默认走"显式命令 + 规则要求"。

### L3 `gc-auditor` 子代理 —— 新鲜上下文重审

- 定义：项目内自定义 subagent（`.qoder/` 下，与 `create-subagent` 的形状一致），提示词只含三样：
  反例索引（`rust.md` 的「反回归自查」条目号）、`guard verbs` 的动词清单、被改文件的 diff + 相关 `.hrx` 原文。
- 硬约束它的输出：每条 finding 必须给 `file:line` + 金样 case 名，**禁止无引用的结论**；不得提出"放松规则/
  改名绕过/先记 TODO"三类建议（这三条都是本轮真实出现过的诱惑）。
- 调用时机：任何触及 `crates/sasspile/src/**` 的批次结束后、提交之前；以及每次"我声称某特征可用"之前
  （由 `claim` 提供数字）。
- 与既有 `CodeReview` 子代理的关系：安全类审查仍走 CodeReview；`gc-auditor` 只管架构/铁律/证据纪律，两者不混。

### L4 教条的重排 —— 短 always-on + 一次显式起手式

- `rust.md` 瘦身：always-on 段只留 **≤40 行**（三条不可协商 + 禁区一句话 + 指向 `guard`/`tasks.md`/source-analysis）。
  长文档在深会话尾部会被稀释，这是 F6 的直接表现，不是猜测。
- 新 skill `sasspile-start`（slash `/sasspile:start`）：机器化执行并**打印**——`guard baseline`、
  `gc_scan` 当前命中数、`tasks.md` 的下一个未做增量、F1–F6 六条自检问句。要求我复述读数；复述不上即说明没读。
- 反例索引（本仓库踩过的 GC 形态）从 `rust.md` 迁入该 skill 的 reference，并与 L2 的规则名交叉引用，
  避免同一律在两处以不同措辞漂移（本轮 `design.md`/`rust.md`/报告三处写数字，只有一处后来被改对）。

### L5 Qoder 插件 —— 让防线随仓库分发、可自检

- 新目录 `plugins/sasspile-discipline/`，形状照搬仓内既有先例 `plugins/rxrust-source-analysis/`：
  `.qoder-plugin/plugin.json` + `skills/`（`sasspile-start`、reactive doctrine）+ `agents/gc-auditor.md` +
  `mcp`（`guard-mcp` 注册片段）+ `scripts/validate.sh` + `SOURCE.json`。
- `validate.sh` 做三件事：① `guard baseline` 能读到非空读数且 `total` 等于 `tests/hrx.rs` 的 `SPEC_DIRS` 实测数；
  ② 植入反例文件后 `gc_scan` 必须逐条点名；③ 移除植入后 `count=0`。即"防线自己也有阳性对照"。

## 4. 每层拦不住什么（诚实边界）

| 层 | 拦不住 | 缓解 |
|---|---|---|
| L1 | 我**没调**工具就动手 | 由 L4 的 always-on 三条 + L3 的事后审成对约束；`commit` 里若出现未被本会话读数支持的数字，审计必须点名 |
| L2 | **语义级 GC**：把状态藏进 `scan_map` 的 `Acc`、换个函数名绕过文本判据 | 规则是代理不是证明（`rust.md` 早已写明）；放宽规则/改名仍属违规，按反例索引改数据分解 |
| L3 | 同一模型的**同错**（审计者也觉得合理） | 只允许引用文件+金样；把审计输出与 diff 一并入库，事后 `git log` 可追；关键结论改由 `claim` 的数字而非自然语言承载 |
| L4 | 我不跑起手式 | L2/L5 的机器触发兜底（宿主 hook 支持时自动跑） |
| L5 | 宿主不支持 hook/MCP 时的降级 | 全部机制都有 CLI 等价物（`cargo run -p guard -- bin …`），文档只描述 CLI，MCP 是加速而非前提 |
| 全部 | **F6 的根**：上下文耗尽本身 | 明确策略：接近上限就压缩（保留结论/状态/未完清单），不换会话；跨会话靠 L1 重取真值而非靠记忆 |

## 5. 验收（fitness functions，全部要跑真命令）

1. **植入反例**：临时文件含 `Vec<Rule>` + 行首 `for` + `fn drain()` + 第二张单位表 ⇒ `gc_scan` 4/4 点名，exit≠0。
2. **谎报读数**：`claim --run <x> --expect "core_functions/math/variables::pi=PASS"` 在一个不存在/仍败的 run 上必须失败。
3. **伪造字面量**：提交前把测试里的字符串字面量与 `fixture` 输出比对，不一致即拒（`guard` 提供 `--check-literals`）。
4. **冷启动实验（真正判据）**：全新上下文，只给一句"继续 sasspile"。三次里必须每次都：先跑 `start`；
   引用的基线与 `guard baseline` 逐字相同；实现批次后触发 `gc-auditor` 且其 finding 均带 `file:line`。
   任一失败 = 补闸，不写"下次注意"。
5. **机制不改行为**：装配完成后 `ledger diff <上一个 run> <新 run>` 必须 `net=0 regress=0`（防线只读实现）。

## 6. 分期与工作量（决策已定，无需再选）

| 期 | 内容 | 触不触代码 | 量级 |
|---|---|---|---|
| S1 | `rust.md` 瘦身 + `sasspile-start` skill + `gc-auditor` 子代理定义 | 纯配置/文档 | 小 |
| S2 | `sasslint` +5 规则，每条配植入反例与下界测试 | 改现有 crate | 中 |
| S3 | `crates/guard`（MCP server，复用 sasslint/ledger）+ 版本自检 | 新 crate | 中大 |
| S4 | `plugins/sasspile-discipline/` 打包 + `validate.sh` | 新目录 | 中 |
| S5 | hook 接线（宿主支持时）；否则在 `start` 输出里印"批次后必跑命令" | 配置 | 小 |

顺序理由：S1/S2 当天即可改变行为；S3 把"读真值"的成本降到一次调用；S4 让防线可分发、可自检；S5 最后做，
因为它依赖前四期稳定，且可能需要你批准改宿主设置。

## 7. 明确不做

- 不再扩写"靠自觉"的长文档（F6 已证明越长越无效）。
- 不开第二个 sqlite / 不写快照文件（`ledger` 的存在理由就是取代 `reports/hrx-*.tsv` sprawl）。
- 不放松任何现有机器闸、不靠改名绕闸。
- 未经你批准不动 git 配置、不加依赖、不改 `.qoder` 之外的宿主设置。
