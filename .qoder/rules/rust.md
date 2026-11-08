---
trigger: always_on
alwaysApply: true
---

## 技术栈与语言
- Rust edition 2024，toolchain 1.99。所有对话用中文，代码/命令/文件名/注释用中文
  （2026-11-05 由 English 改过来；**存量代码注释正按 crate 逐批改写**：`crates/sasslint/src`（`tasks.md` 9.29）、
  `crates/ledger/src`（9.30）、`builtins` 登记骨架 + math 域（9.31）、`builtins/color/` 四文件（9.32）与
  `builtins/color/{amount,write}.rs`（9.33）与 `color/{channels,primaries,read,channel}.rs`（9.34）与
  `color/{text,space}.rs`（9.36）与 `color/mod.rs`（9.37，**`color/` 目录自此做完**）与 `env.rs`+`scope.rs`
  （9.38）与 `axes.rs`（9.39）与 `calls.rs`（9.40）与 `unit.rs`+`calc.rs`+`value.rs`（9.41）与
  `lib.rs`+`lexer.rs`+`trace.rs`（9.42）与 `expr.rs`+`operations.rs`（9.43）与 `exec.rs`+`loops.rs`
  （9.44）与 `brackets.rs`+`structure.rs`（9.45）与 `calculation.rs`（9.46）与
  `selector.rs`+`phases.rs`（9.47）已做完，各自 `regress=0 net=0`；**`sasspile/src` 顶层自此全部
  中文化**；`builtins/` 11 文件经 9.31~9.33 全部中文化，9.55 收尾核验（旧账「余约 847 行」是
  注释总行数而非待译行数，经现量复核订正）并顺修 3 文件 5 条 strict doc 断链（父模块外层 doc
  抬层解析域 / 私有模块不可直引），使该域 `--document-private-items` 断链归零；
  9.48~9.54 转入 `crates/sasspile/tests/`，43 件全部中文化（首批十件 + `math_*.rs` 四件 +
  `color_*` 十五件 + 基础件十四件）；9.55 核验 `builtins/` 收尾并顺修域内 5 条断链，9.56
  清全 workspace 21 条 rustdoc 告警（抬层域相对链接 / 私有项不可链改纯文本 / 尖括号参数包
  反引号），**全 workspace `cargo doc`（默认与 strict）自此零告警**；
  （此前 9.41/9.42 与 9.43 草稿把这份清单漏计成「余 4 文件」，漏掉的正是 `exec.rs`/`brackets.rs`/
  `calculation.rs` 三件 —— 现量复核：`tasks.md` 9.29 附近的原始清点列过 `exec.rs` 204、`brackets.rs` 136、
  `calculation.rs` 127，与现测逐字一致，已在 9.43 条订正；9.45 完成后余 3 文件 210 行经逐件现量复核，
  9.46 完成后余 2 文件 83 行经逐件现量复核，9.47 完成后 `sasspile/src` 顶层余 0 行。）
  每批一个 `regress=0 net=0` 的纯文本 commit（每批 250~350 行；会话容量不足时允许缩到单文件，闸口不得减）。
  同一批试点在上一会话因中文输出错字率过高而回退，见 `tasks.md` 9.28 —— 所以每批收尾必须「读回全文逐句核」，
  只扫 diff 不够；行数与宽度一律按 `wc -l` / `wc -m` 写口径（编辑器会把末行后的幻影空行也算进去）。
  **中文化不等于沿用旧文**：9.32/9.33 两批共查出七条注释在断一件归档已推翻或无根据的事（`rgb` 颜色答 hsl 轴、
  `$hue:` “无行点名”、`change` 无行说 no-op、`lab(50% 20 -30)` 当输出、`axis()` 自称不读 lch/oklch、
  legacy 字节侧的百分比臂无 fixture、“五条原本通过的行”复取不到），它们全是继承而非新写——
  改写每一句时都要现量它对不对，错的当场改对。9.34 另立两条收尾规矩：**送审的读数必须是刚跑过的**（本轮把
  “奇数反引号行 HEAD=0”报错了，实际 read/channel 各有 2 处）；**同一处事实只住一个文件**
  （`tasks.md` 9.20 与 `color/primaries.rs` 对同一行和写了两个不同尾数，已订正并把单一住所归注释）。
  中文错字靠定量的怪字表收尾：`grep -n "拄\|殫\|一遗\|癲" crates/`（本会话连错三轮“抄”→“拄”，同一个字靠眼睛抽不干净）。
  **全称否定句与指针是重灾区**：9.35 查出两条把“我们还没做”写成“语料没要求”的注释（“没有任何一行语料走到这个
  拒答”、“legacy 访问器保文本——语料要的就是这个”），另有一条指向不存在记录的指针（“`tasks.md` 9.9 记着 `$space:`
  未闭”，而 9.9 区间内 `$space` 命中 0）；归档里 `error/non_legacy` 共 15 行逐字要的是错误。
  改写到“无行/零命中/没有 fixture”或一个 `tasks.md` 节号时，必须先跑一条 `grep` 或
  `ledger failures … --count`，把那个数或命中位置写进注释。9.36 真正执行了一次这条规矩，发现四类新故障，都是同一成因：
  **现在时的全称句会过期**（“目前有一张表与声明的白不一致”——那批 `7e-8` 早被拟合行顶掉，现量只数到 5 行且不包含
  那行 white）；**全称句没写作用域**（“没有任何 fixture 把缺通道与 alpha 配对”——空间族里就有，
  `color/alpha.hrx::none/slash/blue`；该句只对存字节那四表示成立）；**舍入转述当逐字金样**
  （`color(display-p3 0.25 0.4 0.77)` 全仓 0 命中，真值是 `0.2498513331 0.3952400722 0.77356175`）；
  **同名 key 跨文件用省略号**（`…::lightness/fraction` 在 `change/hsl.hrx` 与 `adjust/hsl.hrx` 里值不同）。
  所以：全称句必带作用域名，金样必逐字可 grep，“目前/现在”类句式只在刚重测过时才写。
  9.38 又探到两个新样式：**一个计数不得同时当两件事的凭据**（“该簇 30 行红”与“其中报这个错的只有 1 行”
  是两个 needle，写成一句就把红行数读成错误数）；**口径不同的两个数必须都写**
  （`Error: Undefined variable.` 全仓 23 次/15 文件，按 `hrx.rs:24` 排除 `non_conformant` 后是 20 次/12 文件）。
  同理：**审计闸不可用时就不提交**（本轮 `gc-auditor` 未注册，改动在工区多停一轮才入库，不得改用
  内置代理顶替）。
  9.39 再把两句本身钉住：**“每个常数都是 fixture 行”这类全体式必须附一份“哪些是推导”的名单**
  （`127.5`/`0.7163120567`/`1.01 / 1.41`/`720` 全语料 0 命中，它们就是推导）；**同一个量在注释里只能有一个尾数**
  （HEAD 同时写了 `71.6312056863` 与 `71.6312056738`，前者 0 命中 —— 归档逐字是后者）。
  另记两个工具坑：`perl -CSD -i -pe` 对中文串改写会静默失败（只能用编辑工具）；`ledger passed … --count`
  不是开关且用前缀匹配，“某簇多少绿行”只能由 `failures --count` 相减得出。
  9.40 立两条：**任何“为 0”的 grep 读数必须当场带阳性对照**（`grep -c '"var"'` 对已登记的 `"div"`/`"rgb"` 也是 0，
  分不出“缺席”与“在表里”；能用的是 `grep -rn 'name: "var"'` = 0 对 `name: "math.div"` = 1）；
  **送审的读数必须贴命令原输出，不得凭“刚看过”**（本会话已两次报错自己量过的数：9.34 的“HEAD 奇数反引号=0”
  与 9.40 的同一句，真值都是 2）。
  9.37 又发现三个新类（都是“凭据本身”而非句式）：**引用凭据必标明它是 input.scss 还是 output.css**
  （`values/colors/alpha_hex/initial_letter` 的 `four-digit: #AbCd` 只在前者，后者逐字要 `rgba(170, 187, 204,
  0.8666666667)`，所以“声明里的 hex 保住拼法”是实现选型而非语料律，那一行至今红）；**key 名必逐个取回**
  （本批查出三个不存在的：`four_args/clamped::red`（真名 `red/finite`）、`hsl/three_args/units::hue/turn`
  （真名 `hue/angle`）、`alpha.hrx::color/named` 归错了行（`color.alpha(red)`→`1` 属 `color/max`））；
  **逐字错误消息不得截断也不得拆行**（截到句号前且不写省略号 ⇒ grep 整条 0 命中；拆成两行写同样）。
  另：`ledger passed <run> <key> --count` 的 `--count` **不是开关**（被当多余位置参数静默忽略），且 `passing()`
  用前缀 `LIKE` 而非子串 —— 所以“某簇多少绿行”只能由 `failures --count` 与总数相减得出，不得拿
  `passed … --count` 当阳性对照（上一会话注释里的“0 green”正是这么来的，已删）。
  仍保持原文的只有四类，因为它们是「可复取凭据」而不是叙述：代码标识符与函数名、命令行、
  文件名与 `sass-spec` 的 case key、以及逐字引用的金样输出或错误消息
  （如 `Expected 0.5px to have unit "%" or no units.`）——译了就 grep 不回归档原文。
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
   色彩三族与 math 域的已定定律均已成文，见 `state-of-the-port.md`）。当前基线 hrx **3457/11,533**（30.0%，**实质通过 3292** + 空期望蒙对 165，skipped 1264；
   A/B 记录 = `reports/hrx.db` 里的具名 run `zh-comments-calls-v2`（终树；与 `zh-comments-axes-v3`
   `regress=0 net=0`，只差 9.40 的注释语言与被修正的凭据；这条纯文本链往前依次是
   9.39/9.38/9.37/9.36/9.35/9.34/9.33/9.32/9.31/9.30/9.29 的 `zh-comments-axes-v3`、`zh-comments-exec-env-v2`、
   `zh-comments-color-core-v2`、`zh-comments-color-space-v2`、`zh-comments-color-math-v2`、
   `zh-comments-color-tables-v2`、`zh-comments-builtins-write-v2`、`zh-comments-builtins-color-v2`、
   `zh-comments-builtins-math-v2`、`zh-comments-ledger-v2`、`zh-comments-sasslint-v2`，再到
   `change-in-carried-space-v6`。该 v6 又是 feat 入库的
   `change-in-carried-space-v5` `regress=0 net=0`，只差 9.28 的审查收口。2026-11-04：**`color.change` 能写在
   带自己空间的色上**，13 个 Color-4 空间共 197 行里 9 个文件全绿——名字属于空间本身（`$red`/`$x`/`$chroma`），
   而 **`%` 的底数是逐通道的不是逐空间的**（`lab` 自己里面 `lightness 30%`→30、`a -40%`→-50），所以新表
   `color/channels.rs::percent_divisor(空间, 通道)` 是唯一住所，`Space::percent_full` 只是它的 `通道 0` 行；
   hue 折成一圈（`400deg`→40），**负彩度折 hue+180**（`lch(50% 20 30deg)` 的 `$chroma: -10` → `lch(50% 10 210deg)`）。
   两个“拒接”形状也有金样（`Expected 0.5px to have unit "%" or no units.` / `Expected 20% to have an angle unit
   (deg, grad, rad, turn).`），而“不是数字”仍 decline（`unexpected error` 一度 19→22 后按这条闸改回）。
   **`regress=0 net=243`**（本会话单批最大），`ledger shift` = **`changed=48 closer=0 farther=0
   error_expectation=0 shape_of_text=48`**（44 条嵌套行归共用的 lab/oklab 表、4 条等 `color-mix` 发射；
   `negative_zero` 那行只属于 net 口径）；无效错误仍 19，空库复算同读数；本会话九批累计
   **`regress=0 net=959`**。`builtins/color/modify.rs` 到 512 行 ⇒ 拆为签名层与写入层 `write.rs`（拆分 `changed=0`）。
   数字与空余项全在 `tasks.md` 9.27；其 14 条 finding 的收口（含三处伪造/失效的金样 key 引用与
   `unit::is_angle` 的合并）在 `tasks.md` 9.28。
   其前一批 run `change-writes-absence-v4`（终树；与 `change-writes-absence-final` `regress=0 net=0`，
   两者只差注释与一条不可达臂。2026-11-04：**`color.change` 能把“缺”写进字节通道**
   ——`change/rgb.hrx::{red,green,blue}/none` 要 `rgb(none 0 0)`/`rgb(0 none 0)`/`rgb(0 0 none)`，
   `change/space.hrx::missing/arg/legacy/same/implicit` 要 `color.change(pink, $red: none)` → `rgb(none 192 203)`
   ——所以 `Color::with_channel` 的值位取成 `Option<f64>`；只有 `change` 能写缺，`adjust`/`scale` 的读者继续拒绝。
   **`regress=0 net=4`**，`ledger shift` 的 `changed=0`（本会话首个一个形状变化也没有）；本会话八批累计
   **`regress=0 net=716`**；无效错误仍 19，空库复算同读数。本批犯了一个**次序错**并被闸拦住：把“先读参数、
   再读当前字节”换倒，`ledger diff` 当场报 `regress=42`（那 42 条是 `scale/error/units/*.hrx` 的错误行，
   语料的消息来自读参数那一步，`modify.rs` 自己写过这点但我没读它就动手）——教训是改一条链之前先读它
   自己标下的“为何是这个顺序”。`modify.rs` 因新注释越 500 线 ⇒ `scale_toward`/`step`/`keep` 入 `amount.rs`（拆后 `net=0`）。
   剩下的写入缺：key 口径（以 `/none` 结尾）**45** 条、input 口径（实参含 `: none`）**51** 条，各自等的律
   （39 条空间色接线 + 6 条 `$space:` 写入、5 条 axes 带缺、1 条 alpha）全在 `tasks.md` 9.25。
   【现量订正：那 39 条空间色接线已由 9.27 做掉 —— 其中 key 以 `/none` 结尾的 39 条已绿 33 条，
   剩 6 条（`change/{lab,lch,oklab,oklch}::lightness/none` 4 条与 `change/{lch,oklch}::hue/none` 2 条）
   仍在等“axes 三元组能携缺”；当时写的“5 条 axes 带缺”是 legacy `hsl` 3 + `hwb` 2 的合计。】
   其前一批 run `legacy-missing-channel-v3`（与它 `regress=0 net=0`；2026-11-04：**legacy `rgb` 三元组现在自己携带
   “缺”**——`to_space/rgb/rgb.hrx::missing/red` 要 `rgb(none 20 30)`（保 `none`、保 byte 尺度、换空格形）而同打在空间色上
   的 `to_space/srgb/rgb.hrx::missing/red` 要 `rgb(0%, 20%, 30%)`（代入），两行合起来定“缺属于 legacy 侧”，故 `Channel` 取
   `Option<f64>` 而不往 `InSpace` 搭第二条路；按位/派生代入/全缺 powerless 三律各只一处判定，源侧读数扩到 legacy 即转出）。
   **`regress=0 net=41`**；本会话七批累计 **`regress=0 net=712`**；无效错误仍 19，空库复算同读数；`ledger shift`：
   `changed=13 closer=0 farther=0 tie=0 error_expectation=0 shape_of_text=13`（13 行从“无答案”变“有答案但差 9~10 位”，
   归属用新增的等价断言量而不是说）。中途本批自己引入过一个**造值缺陷**（`adjust(rgb(none 0 0), $red: 10)` 答了
   金样禁止的 `rgb(10, 0, 0)`），现改为在被点名通道缺失时拒绝；逐字报出错消息仍未做。守卫 `tests/color_legacy_missing.rs`；
   `color/mod.rs` 越 500 线后拆出 `color/channel.rs`（拆后 `net=0`）。数字与未做项全在 `tasks.md` 9.23，
   审查收口（含四个“同一事实多住所”的合并：`Representation::writes_bytes`/`Color::triple`、删 `source` 里的
   死算术、`has_unwritten_channel`、`is_missing_word`）在 `tasks.md` 9.24。
   其前一批 run `transfer-segments-confirmed`（= 拆后终树；它的拆前树就是
   `rec2020-audit-closure` = `79ebe15` 本身，两者 `regress=0 net=0`。本批：量具拆开（`tests/common/fit.rs`）+
   **三个空间的分段支各自 A/B，答案不同所以不得互推类比**：`srgb` 的 `12.92` 臂被正向确认（并有
   `tests/color_srgb_segment.rs` 钉住）、`rec2020` 的 `4.5×` 臂在编码侧被拒、`prophoto` 的臂靠 `changed` 保留；
   具体读数只住 `tasks.md` 9.21/9.22，这里不拷数字。**段区的拐点（`0.04045`/`0.0031308`）仍是转录**：
   拟合行最小打印通道 `0.1` 只能给它上界，而量具无法只改阈值做 A/B。量具里“哪些空间线性印出”的名单已删
   （与 `Space::transfer()` 飘过，而把 `color` 抬上公共面不由工具决定），准入现在只由 `verdict` 判；
   `fit.rs` 的数学接进了 `tests/calibrate_fit.rs`（含反向对照：错的阵必被拒）。
   其前一批 run `rec2020-no-offset-v3`（同日：**`rec2020` 的转移曲线两个常数以
   及分段支都不是语料的**——原写 `((v+α−1)/α)^(1/0.45)`（`α=1.09929682680944`），语料是**纯有符号幂 `2.4`**（无任何
   偏移）；分开它们的是假设扫描的最坏残差（`2.4/1.0` → `4.9e-11` holds；原写法 → `0.214`；数值只住 `tasks.md` 9.20），
   而“删掉 `4.5×` 分段支”是 A/B 定的（保留 → `net=85`，删除 → `net=126`；语料正向偏好纯幂，但该读数只覆盖
   **编码侧**：`smallest_linear_channel=0.00163 < 0.081` 而 `smallest_curved_printed_channel=0.0689 > 0.081/4.5`，
   解码臂仍零约束）。同一探针对 `prophoto`：`net=0` 但 `ledger changed` 查出 **11 行变值且臂侧靠金样**
   （`to_space/oklab/prophoto_rgb.hrx::missing/non_lightness`：金样 `0.016`，留臂 `0.0159999996`，删臂 `0.0215443466`），
   所以它的臂**保留的理由是 changed 读数而不是“不可证伪”**——只看 `net` 会得出完全错的理由（`missing/*` 行两例都红）。
   表：第一行三项拟合后覆盖（差 `3.76e-6/2.00e-7/4.03e-6`）；第二行两项虽差不过自身散布，但白行（曲线无关）直接
   锁住整行行和 → **联合约束优先于单项平局**（新口径，见 9.20）。**`regress=0 net=141`**；本会话六批累计
   **`regress=0 net=671`**；无效错误仍 19，空库复算同读数；`ledger shift`：**`changed=142 closer=131 farther=0
   tie=0 error_expectation=0 shape_of_text=11`**（本会话首个 `farther=0`）；中间变体曾绿的 16 行已逐条具名（全为末位 1，
   **两侧都有**：rec2020 作目标用逆阵、作源用正阵）。守卫 `tests/color_rec2020.rs`。
   其前一批 run `audit-closure-prophoto`（同日：补跑 prophoto 那批的审查，把 `Space::white()` 的 `_ =>` 兜底改为
   穷举 13 变体，并把 rec2020 的 `7e-8` 行和缺陷从“曲线假设未定”里拆出来——拆后行为无变，`regress=0 net=0`）。
   其前一批 run `prophoto-solved-v2`（2026-11-03：`prophoto-rgb` 的 primaries 整表改为从
   `to_space/prophoto_rgb/xyz.hrx` 拟合（九项均超出自身逐项散布），而拟合出的表**自身就落在 D65 白上**，故
   `Space::white()` 不再把该空间归入 D50——白点由表携带，不再两处各自声明（该白行改前 X 差 `3.4e-2`；`4.2e-2` 是
   `[0][0]` 项自己的差，两个对象不得写混）。
   **`regress=0 net=110`**；本会话五批累计 **`regress=0 net=530`**，无效错误仍 19，空库复算同读数；
   `ledger shift`：`changed=173 closer=151 farther=10 tie=0 error_expectation=0 shape_of_text=12`，**`farther=10` 全部含
   `rec2020`**（那家的表仍是抄的，两边错原本互相抵消）。量具新增两个读数：`smallest_curved_printed_channel`（实测
   prophoto 作源的行最小非零通道 `0.1 > 1/16` ⇒ **解码分段支零约束**；口径仅限“本拟合读到的行”——语料里
   prophoto 作**目标**时确有 `0.0010727236` 这样的通道，那一批约束的是编码支 `1/512`）与把“哪侧带曲线”收为枚举。
    primaries 表因此拆到 `color/primaries.rs`（拆分前 `wc -l` 量到 503，该瞬时态未入库不可重现；可重现的是
   `git show 1ea3325:crates/sasspile/src/color/space.rs | wc -l` = 485 与拆后 409/119）。守卫 `tests/color_prophoto.rs`。
   其前一批 run `a98-curve-and-table`（2026-11-03：**`a98-rgb` 的传递曲线方向被反接**
   ——`Transfer::A98` 用 `1/2.19921` 解码、`2.19921` 编码，而语料是 `linear = printed^(563/256)`；指数本身也取整错了。
   分开它们的是“在该假设下拟合后的残差”，而量具现在会直接判：`hypothesis_holds=false` 时 **exit 4**（候选与残差表只住
   在 `tasks.md` 9.17，这里不拷数字）；同一探针证明 `prophoto-rgb` 的方向本来就是对的。
   矩阵按 9.16 的“只覆盖能分辨的项”重解（仅 `[0][0]` 保留发表值），**`regress=0 net=96`**；本会话四批累计
   **`regress=0 net=420`**，无效错误仍 19，空库复算同读数；`ledger shift`：`changed=177 closer=160 farther=3
   tie=0 error_expectation=0 shape_of_text=14`，**`farther=3` 具名**：`to_space/a98_rgb/hsl::{gray,white}`（金样中性，
   色相由噪声决定——距离度量把无意义的色相算了进去）、`to_space/a98_rgb/rec2020::out_of_range/far`）。
   守卫 `tests/color_a98.rs`。其前一批 run `display-p3-solved-final`（2026-11-03：display-p3 的 Z 输出行后两项从语料解出，
   **`regress=0 net=165`**；其余七项因“拟合差不过自己的逐项散布”而保留转录值——整表换成拟合值实测 `net=136`，
   从另一侧拟合的整表 `net=168` 但破 `regress=0` 于 `to_space/display_p3_linear/srgb.hrx::out_of_range/far`；
   到该批为止三批累计 `regress=0 net=324`，无效错误仍 19，空库复算同读数；`ledger shift`：
   `changed=321 closer=241 farther=63 tie=1 error_expectation=0 shape_of_text=16`，**`farther=63` 全为“两侧之一带未校准族”**
   ——旧抄表的两个错原本部分抵消，不得把它当 `net` 的反面证据）。其前一批 run `missing-rectangular-v2`（同日本会话：`none` 缺通道在
   **索引对齐族**（rgb 系 + `xyz`/`xyz-d50`）之间按位置传播，`regress=0 net=44`；`ledger shift` 报
   `changed=169 … shape_of_text=169`——注意 shape 与 distance 在量具里是互斥分支，那 `closer=0 farther=0` 是**未测**而不是“没动”。
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
