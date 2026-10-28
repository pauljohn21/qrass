# sasspile 全语言框架设计 — 完成报告

> 说明:目标完成后系统要求生成 canvas 报告,但项目级 canvas 目录
> `~/.qoder/projects/-Users-panglijun-rust-qrass/canvases` 在沙箱可写范围之外,且本机无
> canvas SDK 声明可核对。故以等价 Markdown 交付(数据全部内联,无截图 —— 本阶段是文档/设计工作)。

## 1 · 成果摘要

系统性吃透整个 `sass-spec`(值/单位/calculation、选择器/extend、callable、模块 use/forward/import、
core_functions、css at-rules、directives、variables 作用域),产出**覆盖全语言、自洽、可定稿**的 OpenSpec
框架设计。核心架构定为**混合式**:普通求值是冷流算子,`@extend` / 媒体冒泡 / 模块是**物化 owned 结构上的
全局 pass**;真并发只在模块加载(order 键保序);入口 `async compile` 为唯一出口,终点 `into_future().await`。
**本阶段未新增产品实现**,产品树保持绿色。

| 指标 | 值 |
|---|---|
| `openspec validate --strict` | ✅ valid |
| `design.md` | 153 行 / 10 节 |
| 三份 spec 需求数 | 33(scss-compilation 20 + reactive 6 + conformance 7) |
| 遗留 Open Questions | 0(已裁定) |

## 2 · 关键步骤(逐域取证 → 六要素 → 全 change 对齐 → 裁定)

| 步骤 | 内容 | 证据(实测样本) |
|---|---|---|
| 逐域取证 | 吃透 8+ 域写入契约 | calc 不可化简原式保留;`1/2`=斜杠 list;`@extend` 链式 + `:is` + `:todo: dart-sass` |
| 模块/导入 | 加载器与排序契约 | `@use … with`/`!default` 多文件;`@import` CSS 上提排序;`@at-root (without: media)` |
| 可调用 | 参数/实参绑定 | `meta.inspect($args)` → positional `(1,)`、named `(c: 1)`、空 `()` |
| design 六要素 | 数据模型/6相管线/并发保序/子系统契约/P0–P8/已定决策 | `design.md` 10 节齐备 |
| 对齐全 change | 去旧局部教条 | proposal/README/tasks/3 specs 统一“混合式 + 唯一出口 compile” |
| 裁定 2 问 | Open Questions → 已定决策 | 依“唯一参照禁 dart-sass”铁律 + sass-spec 硬约束诊断文本 |

## 3 · 变更文件(仅 `openspec/` 文档,无产品码)

- `openspec/changes/redesign-sasspile-rxrust-shared-stream/design.md` — 核心设计 · 153 行 · 10 节
- `…/proposal.md` — 升格全语言;去 parse.rs/全程 Shared 旧述
- `…/tasks.md` — 重生成为 P0–P8 阶段 + scoped regress 口径
- `…/README.md` — 一句话摘要对齐
- `…/specs/scss-compilation/spec.md` — 20 条行为需求(覆盖全语言)
- `…/specs/reactive-stream-architecture/spec.md` — 6 条混合式架构契约
- `…/specs/sasspile-conformance/spec.md` — 7 条唯一出口 + regress 闸

## 4 · 验证证据

| 检查 | 结果 |
|---|---|
| `openspec validate --strict` | Change is valid |
| `cargo build -p sasspile` | 无 error / 无 warning |
| `cargo test --all-targets` | 全过(compile 8 …);0 failed |
| `cargo clippy --all-targets -D warnings` | exit 0 |
| `sasslint crates/sasspile/src` | count = 0 |
| hrx harness 基线(`ledger`) | 560 → 561,regress = 0 |

## 5 · 最终结论

本阶段目标达成并逐项可验;实现属**“用户定稿后”的下一阶段**(从 P0 harness 起步)。

**定稿前请确认两个已定决策（可一票否决，即改）：**
1. 诊断纳入 v1 达标：`output.css` / `error` / `warning` 三类别 **scoped 基线**，文本走 `ledger` 归一化比对。
2. `element-plus` 只作**非回归自基线**，**绝不用 dart-sass 生 golden**（守“唯一参照”铁律）。

---

## 6 · 现状校正（2026-10-25，逐行读完 rc.6 源码 + P0′ 开工后）

上文是当时快照，结论部分已被源码级事实推翻，以下四项以本节为准：

| 当时说法 | 现况 |
|---|---|
| “流算子 + 物化结构上全局 pass”即可 | 不够：**每相必须是真算子内核**（`struct{source}` + `Observer` 持状态 + 三层 impl）。旧写法 `scan_map→Vec→flat_map(from_iter)` 是批量外壳，已于 P0′.1/.2/.4 重写（`crates/sasspile/src/phases.rs`） |
| “并发 merge 后 `sort_by_key(order)` 保序” | 作废：保序靠动词（`concat_map`/`concat_all` 或串行）；`merge_all` 并发额度满会**丢排队内层的发射**，排序救不回。`order` 键已删除 |
| “终点 `into_future().await` 即可” | 补充实测限制：要求**恰好一个值**（空→`Empty`、多→`MultipleValues`）且**丢弃订阅句柄**（不可取消） |
| 基线 560→561 | 现为 **595 → 598**（+3 来自规范驱动的发射时机更正）；`sass-spec` 全量实测 14,345 条 input / 11,808 条 output |

新增权威文档：`docs/rxrust-source-analysis/09-cancellation-and-terminal-ordering.md`（取消/锁/终止投递次序
不变量 + rc.6 “自带注释与实现不一致”清单）。防回归入口：`README.md`「Anti-GC red lines」、
`.qoder/rules/rust.md`「新会话起手」与扩充后的「反回归自查」（新增批量外壳/排序代替时机/兜底当错误策略/假并发四行）。
待做（tasks.md）：0′.3 真 `Err = SassError` 通道、0′.6 四条 sasslint 机器闸、0′.7 `Selector`/`PropName` 结构化。

> ⚠️ 上面这行已过期，以下面一节为准（不作历史改写，只加校正）。

## 第二次校正（2026-10-26，源码级）

| 上次说法 | 现况 |
|---|---|
| 待做 0′.3 / 0′.6 / 0′.7 | **均已合入**：真 `Err` 通道已接且 `UndefinedVariable` 已致命（三道守卫：头部只认
  `#{}`、special 调用参数、plain-CSS at-rule 体）；四条铁律已编成 `sasslint` 机器闸，CI 门 `gate_sasspile_is_clean`
  已去 `#[ignore]`；选择器/属性名是 `src/selector.rs` 的结构化值（`&` 成词素，`str::replace` 已删）。**P0′ 已收尾** |
| 基线 595 → 598 | **595 → 635/11,533**（5.51%）。量具换了三项：key = 相对 `spec/` 全路径（旧制式 11,486 条里有
  1,135 个重复 key）、`error` 用例比对金样 `Error:` 首行（旧口径“任何 Err 都算过”使一阶改动虚报 +385，真值 +1）、
  安全子集补上 `operators`/`parser`/`variables`（曾有 79 例从未计量，而任务书自己引用 `operators/**`） |
| 未提及 | 新铁律三条（均已在 `rust.md`/`README.md`）：**名字的位置不是引用**（`@for $i` 头部 / `f($hue: 1)`
  具名实参 / `!default` 旗标，共三类假错）、**分类粒度不能超过词法**（`-a-calc(`、`&`、带引号串均需先成词素）、
  **量具覆盖面要等于计划引用的簇，任何“为 0”读数先过阳性对照** |
| 未提及 | `crates/docsizer` 已从 workspace 成员里删除（悬空引用会让所有 `cargo` 命令失败）；其逐行教程 change
  与 `docs/rxrust-from-scratch/` 仅作历史，不得 apply 或重启 |

现行权威入口：`openspec/.../design.md` + `tasks.md`（每个结论背面的实测数字都记 在那里）、`README.md`
「Anti-GC red lines」（现为 9 条）、`.qoder/rules/rust.md`。A/B 记录 = `reports/hrx.db` 里的具名 run
（该库不入库跟踪，只存本地），散存快照文件的做法已被废除。

## 第三次校正（2026-10-28，值建模层）

上一节的表仍成立，只是基线与“谁在防 GC”这两项变了：

| 上次说法 | 现况 |
|---|---|
| 基线 635/11,533（5.51%） | **1,841/11,533（16.0%，实质通过 1,676 + 空期望 165）**，具名 run `readonly-module`；
  无效错误（“该通过的行意外报错”）长期基线 **53 → 18**。逐个增量在 `tasks.md` ㉑–㉜ |
| “四条铁律已成 sasslint 机器闸” | 仍是 4 条，但已证明**它们是代理而非证明**：本轮反复被推翻的不是代码而是
  **文档里的断言**（9 次）：伪造金样文本、伪造 case 名、把“我的选择”当成语料钉住（`math.max(1px, 2q)`）、
  把旧 dump 的记忆当证据（“指数打印侧”候选被当场推翻）、以及“读数只能来自产生它的那条命令”（编译失败时
  后续命令吐的是旧入库值）。这些不是 GC，但同属“拿不该信的东西当依据” |
| 值建模只到 `Expr`/`Value` | 新增三条已实测的建模律：① `module.$name`（`math.$pi`）在 **lexer 层就是一个词素**
  （`Tok::MemberVar`）—— `Word+’.’+Var` 分开后下游不可再合并，而“未知命名空间逐字保留”把 33 行意外错误直接
  消掉；内置模块成员**只读**（`exec::Target::ReadOnly`，判据是模块表认得这个名字而非命名空间文本）；
  ② 指数字面量属数（`operations::number_lexeme` + `exponent_len`；`e` 后无数字仍是单位 `e`，无金样不外推）；
  ③ 含常量的乘法**先试 `arithmetic` 再回落插值**，回落安全由 `operand_expr` 的窄读法保证 |
| “一张表一份规则”只在文档里说说 | 已用实现固定：换算表只剩 `unit::UNITS`（`unit::conversion`），`math.rs`
  自带的十位小数长度表已删；比较只剩 `math.rs::compare`/`compare_named`（同一规则、两种消息）；
  色彩三族共用 `color/modify.rs::route(given, writing)` × `Space`；无单位拒语只有 `builtins/math.rs::unitless` |

防 GC 的下一层防线（跨会话）已写成设计：`docs/superpowers/specs/2026-10-28-cross-session-anti-gc-regression-design.md`
——五层（不依赖记忆的硬闸 / 钩子 / 子代理 / MCP 单真源 / 可分发插件），每层写明能拦什么、拦不住什么、
以及验收办法。
