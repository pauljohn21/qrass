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
   `Value::Bool` 与 Sass 数字序列化（**自然表示不超 10 位有效数字则原样，否则截到 10 位小数**——
   2026-10-27 由颜色金样纠正，原先记的“10 位有效数字”对 `67.8431372549%` 是错的）、**1.4a `Calculation` 第一片**（`src/calculation.rs`：`Value::Calculation`，
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
   **以上均不得重做/重新发明**。当前基线 hrx **1585/11,533**（13.7%，**实质通过 1420** + 空期望蒙对 165；
   A/B 记录 = `reports/hrx.db` 里的具名 run `trig`）；无效错误仍 **53**（曾一度因定长 arity 把
   Microsoft-filter 重载报成致命错而变成 55，靠 `ledger failures … --count` 拦下）。
   唯余 0′.5（共享态 `C::RcMut`/`C::RcCell`，现无可改之物，触发点在 P4 `@content` / P5 模块多播）。
   下一主战场：**1.5 色彩（已大半）**。`src/color/*` + `src/builtins/color/*` 已落 `rgb`/`rgba`：颜色**记住它是用什么
   表示法创建的**（unitless 通道→`rgb(18, 52, 86)`、有 `%` 通道→全部转 `%`（`173`→`67.8431372549%`）、
   hex 造的且 opaque→`#112233`、一旦半透→`rgba(17, 34, 51, 0.5)`），通道入口夹紧；**声明里的裸 hex 仍走 `Raw`**
   （`#AbCd` 连大小写都要保留）。 rgb 那批 +30、hsl 那批 +35、颜色名+访问器那批 +20，全部
   在 `core_functions/color` 内且每批 `regress=0`。注册表已加 **optional 参数**，且“带名字的 variadic”能绑
   标签（`rgb($red: 0, …)`、`hsl($hue: 0, …)` 已绿；“必需参数缺失”只约束定长签名——但 arity 错误仍在进 body
   之前，所以带 CSS 重载的名字一律用 variadic，否则 filter 写法会被误报致命）。已落 `color.red/green/blue/alpha`
   访问器，颜色名最初取自一个名字表依赖（**现已换成 `color`，见下**；**只用数据**，解析与打印不从它来；
   已拿 6 个语料自陈的名字做抽检）；`color.hue/saturation/lightness` 也已落（legacy 那 17 条金样行用的都是
   `hsl(…)` 色；单位律由金样给出：hue → `359deg`，saturation/lightness → `42%`）。
   **颜色名的最终依赖 = `color`（linebender，活跃）的 `parse_color`**：`color-name` 因不再维护而被剔除
   （未知 bug 风险比它的多 7 个别名更有分量）；`color` 词表缺的 7 个 `-grey` 拼法用“拼法折叠 `grey`→`gray`”
   补齐，**不录入任何手敲 RGB 值**。已落 **rgb↔hsl 换算**（`src/axes.rs`，纯 f64，故意不取整）**与它的读取面**
   （`Color::hsl_channel` 走 `axes_of()`，所以 `color.hue(#abcdef)` → `210deg`、`color.saturation(#abcdef)` →
   `68%`、`color.lightness(#abcdef)` → `80.3921568627%`，三条来自
   `core_functions/modules/color/{hue,saturation,lightness}.hrx`；当时“全语料无此类金样”的断言是**取证面太小**——
   只读了同名的 legacy 文件。反向 `hsl`→`rgb`（`color.red(hsl(…))`）**仍 decline 且有依据**：grep 全 `spec/`
   找此类调用**一条也没有**，不得拿“255”当金样说话）。
   **打印律**：通道不是整字节就打百分比（`grayscale(#633736)` → `rgb(30%, 30%, 30%)`），整字节就打
   数值/hex（`grayscale(#494949)` → `#494949`）——取整属于打印。轴变换已统一到
   `Color::transform_axes`（`darken`/`lighten`/`saturate`/`desaturate`/`adjust-hue`/`grayscale` 全部已接）。
   **decline 律（本轮实测钉下）**：被 builtin 拒绝的调用必须保留**词元原文**且只替换其中的**变量读取**
   （走 `expr::interpolated`）：实参内部空格要留（`rgb(none 52 66)`、`attr(c, %)`），未绑定标签要留
   （`math.max($number1: 1, 2)`），而 `$c` 必须变成它的值（`saturate($c)` → `saturate(1)`）。用求值后的
   实参重建文本已实测为 **40 条回归**，不得重试。**颜色文件已拆三处**：`src/color/mod.rs`（数据与读取）、
   `src/color/ops.rs`（轴算术 `transform_axes`/`axes_of`/`mixed`）、`src/color/text.rs`（表示法与舍入律，
   `fractional`/`percent_form`/`legacy` 全在本文件私有）。反向写名已落，但它是**按来源**而非按值：只有由关键字创建的
   颜色（`Representation::Named`）才在“opaque + 整字节 + 匹配关键字”时回写名字（`lighten(red, 0%)` → `red`、
   `rgb(transparent, 1)` → `black`），而 `rgb(0, 0, 0)` 仍打 `rgb(0, 0, 0)`（六行金样），不匹配则回退到
   hex（`complement(turquoise)` → `#e04050`）。148 个**只有拼法**在 `color/text.rs::NAMES`，值全向
   `color::parse_color` 要（写错只会退化成 hex，不会打错颜色）。已落 **`color.mix`**（legacy sRGB）：公式是
   **alpha 加权**（`mix(rgba(#91e16f, 0.8), rgba(#0144bf, 0.3))` → 首色权重 0.75 ⇒ `145×0.75+1×0.25=109` =
   `42.7450980392%`；镜像行权重 0.25 ⇒ `37` = `14.5098039216%`），alpha 取平均，结果**继承第一个颜色的表示法**，
   权重**单位一律当百分数读**（`50px` 也是 50）。`Color::of` 现在**拒绝含括号的文本**：否则
   `color(display-p3 …)`/`lab(…)` 会被顺手转成 sRGB —— 那类行仍旧算红（pass 看不出），但内容**是错的**；
   现在逐字。**未识别标签已收口**：绑不上签名的 `$label:` 现在报 `ErrorKind::UnknownParameter`（非致命），
   `exec::evaluated` 看到它就**不重渲染**、直接留原文（`color.mix(…, $method: oklab)` 不再被 legacy 错答）；
   “带不带标签”只有 registry 能定 —— 在词元层用 `named_argument` 猜会把 `saturate($c)` 弄红（实测 regress=1）。
   剩余未解决：hsl 源不混色（decline）。`-grey` 拼法回写已定为
   **不可测**（语料无 `-grey` 输出）。**现代一参色语法已落**（`rgb(18 52 86)`/`hsl(0 100% 50%)` → legacy 逗号形
   输出，不需新数学）：新值形 `Value::Space` 只在**实参**内产生，**任一分量读不出就整个逐字**（所以
   `rgb(none 52 66)` 不会变成“缺一个通道的颜色”）；`+`/`-`/`*` 在任何分量里都使这串不再是列表。两条新律：
   **被夹紧的百分比失去 `%` 拼法**（`rgb(100.1% 0 0)` → `rgb(255, 0, 0)`，而恰好 `100%` 不动就保留 `%`），
   且表示法在**夹紧之后**才决定；hsl 轴**不对称**（负饱和→ 0，上界与亮度都不夹）。现代形的 ` / alpha` 也已落：
   语料用 `rgb(list.slash(0 255 127, 0))` 与 `rgb(0 255 127 / 0)` **期望同一输出**自己声明了模型 ⇒ alpha 是
   **斜杠列表（`Value::Slash`）的第二元**，不是除法；只有“独占一个分量的 `/`”算分隔符，粘在分量里的
   `127/0.3` 仍走算术。**仍缺**：`list.slash` 未注册、`none` 值（注意语料对 `rgb(0 255 127 / none)` 全逐字，对 `hsl` 却半重格式化为
   `hsl(180deg 60% 50% / none)` —— 两者不是同一条规则）。
   **绑定规则按签名种类分叉**（同一规则套两类签名会静默丢标签）：**定长**签名拿“第一个缺位之前”的槽
   （第一个缺位的必需参数就是语料的 `Missing argument $x.`）；**variadic** 签名拿**全部已绑定的槽**按声明顺序
   递（实参个数由 body 自己判）——否则 `rgb($channels: 0 255 127)` 的 `channels` 落在第 5 槽而 body 拿到空列表
   （实测就是靠这条收到 4 行）。`channels` 是真参数名：语料自己的错消息就用 `$channels:` 前缀
   （`rgb/error/one_arg.hrx`）。**尺子**：`builtins/color.rs` 已拆为 `builtins/color/{mod.rs,constructors.rs}`
   （net=0 已验）；`color/mod.rs` 的轴算术已拆到 `color/ops.rs`（`ledger diff rgb-to-hsl-axes color-ops-split` =
   net=0 regress=0，`mod.rs` 降到 359 行）。**`color.channel` 的读取面已落**（`Color::channel(name, space)`，
   17 行金样）：无 `$space:` 只读颜色自己的空间，跳空间必须标签自己说（`pink` 的 hue 在 `$space: hsl` 下是
   `349.5238095238deg`）；`srgb` 是字节/255。通道名在 body 那里是 `Value::Raw("\"red\"")`（与 `math.unit`
   同形），只剥一层引号，不重切文本；`$space:` 读不到时**直接 decline**（否则 hwb 的请求会被当成 hsl 答）。
   **下一个要量的**：`color()` 文本色（`channel/srgb` 仍败 9 行）、`lab`/`lch`/`oklab`/`oklch` 新空间、
   `list.slash` 与颜色 `==`。**`hwb` 已落**（`Color::hwb` + `hwb`/`color.hwb` 注册 + `axes::{hwb_to_hsl,rgb_to_hwb}`）：
   打印用 hsl 形体但值仍存 whiteness/blackness；**w+b 溢出在创建时就归一化**（否则 `color.channel` 会读到
   `70%` 而非 `58.3333333333%` —— 那是错值不是缺能力）；非角单位当普通数用（`hwb(1in,…)` → `hsl(1,…)`）；
   整字节且恰为关键字的 hwb 色**回写关键字**（`color.hwb(0,0%,0%,100%)` → `red`）而 `rgb(0,0,0)` 不会
   —— 这个来源差异语料未解释，故逐 fixture 记录，不推广到 `mix`/`adjust` 结果。`Color::{byte,mixed}`
   对 `Hsl | Hwb` 仍 decline（无此类金样）。**`color.change` 已收 rgb 通道**（18 行）：body 现在能拿到
   “这个实参绑到哪个参数”（`Builtin::labelled: Option<LabelledBody>`，**新增字段而非改 `apply` 类型** ——
   后者要改 29 个 body，本轮已在宽改动上错过两次）；没名字时 `$red: 500` 与 `$green: -50` 在 body 那里
   完全一样，而语料给两个不同的颜色。新律：**越界的颜色一律打 hsl 形体**（`color.change(black, $blue: 256)` →
   `hsl(240, 100.7874015748%, 50.1960784314%)`），因此 `Color::with_channel` **不夹紧**（夹紧仍只发生在构造子）。
   仍红：`*/none`（要 `none` 值）、`alpha_arg`（要 `$alpha:`，是否夹紧无金样）、`change/hsl` 36 + `change/hwb` 15
   与现代空间 ~230 行。**`change` 现在也写 hsl 轴与 `$alpha:`**（net 34，1151→1185）：
   `Color::with_axes` 是 `change` 自己的入口，**不夹紧**（`color.change(plum, $saturation: 120%)` →
   `hsl(300, 120%, 74.7058823529%)`），而 `transform_axes`（**legacy** `saturate`/`darken`/`grayscale` 那族）仍夹紧 ——
   两条相反的律都有金样，所以是两个入口而非一个开关（`color.adjust` 后来也不走 `transform_axes`，见下）。
   配套四条细则：“越界就打印**写入的那组轴**”（`Color.authored`，先例是 `calc::Content{folded, printed}`；
   从字节反推会把 `hsl(0,100%,120%)` 弄成 `hsl(180,-100%,120%)`）、**判越界按写入的轴、容差用
   `Channel::whole` 同源那个 5e-11**（`$lightness: 0.5%` 反推出 -1.1e-15 这种“零的浮点余尘”，裸 double 比较会让它
   “越界”；而“先 `round()` 再比”太粗，会把 `color.adjust(plum, $saturation: 53%)` 的 255.19 字节当成在色域内，
   语料却要 `hsl(300, 100.2868217054%, 74.7058823529%)`）、**两轴下界不对称**（负 saturation 仍在色域内打字节
   `change/hsl::saturation/below_range`，负 lightness 不在 → `hsl(0, 100%, -20%)`）、`format_number` 截断后要把 `-0`
   归零（`rgb(1%, -0%, -0%)` → `rgb(1%, 0%, 0%)`）。**`change` 还能写 whiteness/blackness（net 12，1185→1197）**：
   **写与读不对称** —— `color.channel` 跨空间**读**必须给 `$space:`，而 `color.change(#cc6666, $whiteness: 80%)`
   不给标签就能写（`change/hwb.hrx`，名字本身就定了空间）；新入口 `Color::{with_hwb, hwb_of}`，且
   “w+b 溢出按和归一”已抽成 `axes::normalise_hwb` 单一来源（三处共用，写三份就会漂三份）。
   同时把**“是否整字节”的判据定成可推导的容差**：`Channel::whole()` 用 `format_number` 最末一位的一半
   （10 位小数 ⇒ 5e-11），因为 `color.change(#993333, $blackness: 80%)` 算出的 `51.000000000000014` 会
   被 `fract() != 0.0` 误判成非整而打 `rgb(20%, 20%, 20%)`（语料要 `#333333`）——与“判越界前先取整”同属
   “取整属于打印”。`change/hwb::alpha_input`/`alpha_arg` 与 `rgb(#123, 0.5)` 三者对不上任何一条统一规则
   （带 alpha 要百分比、不带要 hex、后者要数字），**仍留红不拟合**。`change` 簇内仍败 335（73 要消息）。`change/rgb::green/out_of_range` 与 hsl 公式不符（要 hue 120/S +100%，公式给 300/-100%），
   **未解释不当已解决**，不为单行拟合。
   **`color.adjust` 已落（run `adjust-legacy`，net 105，1197→1302；regress=0，空库复算同读数）**：它与 `change`
   共用 `builtins/color/modify.rs::route`，只差一个 `Writing::{Set,Shift}`（**一个路由 + 一个枚举，不是两份代码** ——
   同一条律写两遍就是本轮 `fractional()`/色域判定漂移的成因）。边界**逐条来自金样、不是一条统一律**：
   字节夹紧 `0–255`（`adjust/rgb.hrx::red/above_max` 是 `$red: 200` 而非 500）；hsl 轴**不接 legacy 夹紧**
   （`adjust/hsl.hrx::saturation/max_remaining` 保留 100.2868217054% 打 hsl，而同一移动的
   `saturate.hrx::two_args/max_remaining` 打 `#ff7eff` ⇒ `adjust` 也走 `with_axes`，`transform_axes` 只留给 legacy 族），
   但 **saturation 下界仍夹 0**（`saturation/{min,min_remaining,arg_below_min}` 要那个灰），lightness 两头都不夹；
   hwb **先加再按和归一**（`hwb.hrx::whiteness/max` → `rgb(87.5%,…)`）；**没点任何通道是 no-op**、
   只给 `$alpha:` 仍移动 alpha 且夹 `0–1`（`no_channels.hrx::{positional,named}` 与 `alpha/*`；`change` 无此行故仍
   decline）；`units.hrx::alpha/*` 说明 alpha 取任意单位的量值。金样单测 = `tests/color_adjust.rs`（六条律各自成测试，
   并把 `saturate` 的相反律钉在同一条里）。`adjust` 簇仍败 **376**：`hsl/rgb/units/no_channels` 四组已归零，
   `adjust/error/**` 75（要消息，与按目录计数有重叠）、`global.hrx` 2（要 `adjust-color()` 别名）、
   `hwb::{alpha_input,alpha_arg}` 2（与 `change` 同因留红），其余在现代空间 `.hrx` 里（属 P8）。
   **`color.scale` 已落（run `scale-legacy`，net 57，1302→1359；regress=0，空库复算同读数）**：同一条 `route`，
   第三个 `Writing::Scale` —— 通道**朝符号指的那头移动“剩余距离的份额”**（`sienna` 的 red 160 加 12% ⇒
   `160 + (255−160)×0.12`；`lightcoral` 的 240 减 33% ⇒ `240 − 240×0.33 = 160.8`），因此**永远走不出色域**：
   `100%` 正好落在边界（这就是 `scale/rgb.hrx` 没有 `above_max` 而 `adjust/rgb.hrx` 有的原因），
   hwb 仍是“先缩放再按和归一”（`scale/hwb.hrx::whiteness/max` → `rgb(83.3333333333%,…)`），alpha 的界是 `0–1`。
   • **本刀的真错（我自己造的）**：第一版把 scale 的金额用 `byte()` 读 —— `100%` 就成了 **255 个字节**而不是
     “100% 的份额”，16 条 `scale/rgb` 全跑出色域。金额一律取**量值**（`number()`，与轴同一读法）；这条区别已
     写进代码注释并钉在 `tests/color_scale.rs`（关键字结果与小数结果各钉一次）。
   • **`$hue:` 在 scale 里不是“无行”而是有消息**（本段原先说“无行 ⇒ decline”，已被 `colour-diagnostics` 纠正）：
     `scale/error/{args,polar}.hrx` 要 `Error: $hue: Channel isn't scalable.`；而 `scale/*.hrx` 确实不用它做别的。
     教训：“没行”不等于“有 decline 金样”，**要连 `error/` 子目录一起查完再说**。
   • **三函数×三空间不再搭 if 链**：改成 `let space = match (named == 0, !bytes.is_empty(), !hwb.is_empty())`+
     `match (space, writing)`（`Space{Nothing,Bytes,Hwb,Axes}`）——第四个 writing 进来时不必重读一条守卫链。
   • **已注册两个名字**：`color.scale` 与 legacy 全局别名 `scale-color`（`scale/global.hrx::legacy`、
     `core_functions/global/color/scale`、`modules/color/scale` 各 1 行）。净增 57 = rgb 19 + hsl 14 + hwb 14 +
     no_space 7 + scale/global 1 + modules/color/scale 1 + global/color/scale 1；`scale` 簇仍败 **369**：
     `scale/error/**` 125（要消息）、现代空间与 `$space:`（`space` 13、`out_of_gamut` 4、`no_channels` 3、
     `global::non_legacy` 1 —— 它要 `$chroma:`/`$space: oklch` 被当 legacy 答，按 decline 律逐字）。
   `change`/`adjust`/`scale` 三者的 legacy 子集至此都已收；剩下的两大块是**现代空间**（属 P8，需 `color()`
   文本色与 lab/lch/oklab/oklch 换算）与 **`*/error/**` 要消息**（属 P7）。**而 P7 的第一片已落**
   （run `colour-diagnostics`，net 75，1359→1434；regress=0，空库复算同读数，无效错误仍 53）：
   • **新的 `builtins/color/amount.rs`** = “参数能不能读 + 不能读时语料怎么说”那一半（从 `modify.rs` 拆出；
     拆后 `modify.rs` 450 行、`amount.rs` 147 行，不拆就过 500 硬线）：`axis`/`share`/`byte` 三个读数 +
     `not_a_number`/`expected_percent`/`decline` 三条消息。拆的理由
     就是“同一条律写两遍会漂”：`scale` 的金额取量值（份额），`adjust`/`change` 的 rgb 通道取 `byte()`（`50%` = 127.5）。
   • **三族措辞不同，全部逐行照抄**：`change` 说 `c is not a number or unquoted "none".`（它**也**收无引号 `none`，
     那是“缺通道”另一条律），`adjust`/`scale` 连 `none` 也说 `is not a number.`；只有 scale 要求**每个**金额带 `%`
     （`Expected 1 to have unit "%".`），而 `adjust`/`change` 只对 whiteness/blackness 要求（`units/hwb.hrx`）；
     scale 还查 `-100%…100%` 范围（`bounds.hrx::legacy/*`）与 `$hue: Channel isn't scalable.`。
   • **新增三个致命 `ErrorKind`**：`NotAColor`（`$color: 1 is not a color.` —— **只对数字说**，现代色仍 decline）、
     `NotAString`（`$space: 1 is not a string.`）、`Channel`（通道名/范围类：`Channel isn't scalable.` 与
     `Expected -100.001% to be within -100% and 100%.`）。未越界的 `$space:` 字符串仍 decline（现代空间属 P8）。
   • **两处脊级改动（注册表）**：① 声明了参数名的 variadic **空实参时递给 body**，因为 body 知道缺哪个
     名字 —— `color.change()` 要 `Missing argument $color.`、`saturate()` 要 `Missing argument $amount.`，而
     `hypot/max/min`（无声明名）仍用原来的 `At least one argument must be passed.`（已验那三行仍绿）；
     ② `Given` 多一个 `label: bool`（实参是否写了 `$name:`）+ `Given::positional()`，因为语料规定这三个函数
     **只允许一个位置实参**（`Only one positional argument is allowed. All other arguments must be passed by name.`）
     —— 去掉标签后 `color.change(red, 1)` 与 `color.change($color: red, $red: 1)` 在 body 那里完全一样。
   • **本轮靠量具拦下的两个错**：第一版给 `$hue: none` 上了类型消息，把一条**期望输出**的行（
     `change/hsl::hue/none`）变成“意外错误”（53→54）——`ledger failures … --count` 直接拓出来；另一个是我
     又在注释里写了不存在的 case 名 `scale/error/type.hrx::alpha_none`（真名就是 `none`）——第七次同类错。
     75 行分布：三个 `error/type` 共 35、scale 的 `error/units/*` 14、change+adjust 的 `units/hwb` 8、
     `error/args` 7、`bounds` 2、`polar` 1，外加 8 条 legacy 访问器/`saturate` 的 `too_few_args`。
     金样单测 = `tests/color_diagnostics.rs`（九条消息）。**接着的 P7 第二片：两个空间的名字写在一条调用里
     要按名字拒绝。**（run `mixed-channel`，net 21，1434→1455；regress=0，空库复算同读数，无效错误仍 53）：
     第一个“能定空间”的组说空间，第一个外来的名字被点名 ——
     `color.change(red, $red: 1, $hue: 1)` → `Error: $hue: Color space rgb doesn't have a channel with this name.`、
     `color.scale(red, $saturation: 1%, $blackness: 1%)` → `…$blackness: Color space hsl …`
     （`{change,adjust,scale}/error/mixed_formats.hrx` 各 7 行）。这取代了原来的“混空间就 decline”两条守卫。
     **`hue` 不决定空间（它同时属于 hsl 与 hwb）**：第一版拿“任意匹配名”选空间，把
     `color.change(blue, $hue: 150, $whiteness: 20%, $blackness: 40%)` 这类正常 hwb 写入拒成了
     `$whiteness: Color space hsl doesn't have a channel…` —— **5 条已通过的行变成回归、无效错误 53→62**
     （再一次：这类“FAIL→FAIL / 期望输出变成错”只有 regress + `--count` 看得见）。现在表法是
     “空间名 / 能定空间的名字 / 它拥有的名字”三元组，共享的 `hue` 不在“能定空间”列里。
     仍未做：`$chroma:` 这类**谁都不有的名字**（需把未绑定标签递进 body，而
     `Given.param: &'static str` 装不下源码里的任意标签 —— 这是个真阻塞项，不是没写），
     以及现代色（`color(srgb …)`）的名字拒绝（读不出那个色就不知道它在哪个空间）。
     `builtins/color/mod.rs` 已到 474 行、`modify.rs` 462 行 —— 两者再扩都得先拆。
     **`math` 的三角族已注册（run `trig`，net 130，1455→1585；regress=0，空库复算同读数，无效错误仍 53）**：
     选它先量过地形 —— “函数根本没注册”在 `core_functions/math` 就占 266 条要输出的行（而候选“把未绑定标签
     递进 body”只有 5 行可达，所以没动那个形状）。新文件 `builtins/math/trig.rs`（与父文件同名的子目录布局，
     `mod trig;` 在 `builtins/math.rs`）共用父级的 `number_arg`/`comparable`：
     • **直接三角函数**（`sin`/`cos`/`tan`）：无单位参 **就是弧度**（`math.cos(1)` == `math.cos(1rad)`，
       `cos.hrx::{unitless,rad}`），角单位换算（`1grad` = 0.9°），其他单位致命拒绝
       `Expected 1px to have an angle unit (deg, grad, rad, turn).`（`cos.hrx::error/unit`）；结果无单位。
     • **反三角**（`asin`/`acos`/`atan`）要无单位比值（`$number: Expected 1px to have no units.`，
       `asin.hrx::error/units`），**答度数**；`atan2($y,$x)` 先统一维度再取比值，答度数，
       它的两个不可比消息措辞不同（`$x: … and $y: … have incompatible units.` vs
       `… (one has units and the other doesn't).`）——**$x 总在前**。
     • **符号律（本轮新量出来的）**：**携带的** `-0.0` 保留符号（`math.sin(-0.0)` → `-0`、
       `math.atan(-0.0)` → `-0deg`、`atan2(-0.0, 0)` → `-0deg`），而**落在**零上的运算丢掉符号
       （`math.round(-0.2)` → `0`；`angle_of` 的 mod-360 折叠与 `clamp_alpha` 也是）——所以上一条“判越界
       前先取整”写的“`format_number` 把 `-0` 归零”已收窄为：**打印层保留精确的 `-0`，由四舍/折叠/夹紧自己丢**
       （`f64::rem_euclid` 会保留 `-0.0`，这一点必须在折叠处显式归正）。
     • **三行没做也不装成做了**（金样在，律不在本刀）：`sin/tan(-0.000000000001)` → `0` 要“足够接近整数就印
       整数”的精度律（`values/numbers/precision/**` 共 108 行要输出，下一个就取它）；`atan2(1cm, -10mm)` →
       `135deg` 要长度因子精确而非语料那 10 位小数读数；`atan2(1px, 1deg)` 要 `deg` 进维度表
       （`dimension()` 故意只收已实证的长度/时间/em）。已钉的 `tests/math_trig.rs` 只断言能过的行，
       这三行在文档里写明留红。
   而 `complement` 暂不能动，因为
   `complement(turquoise)` → `#e04050`（整字节）与 `complement(rgba(turquoise, 0.7))` → 百分比形态这对差别
   还没解释清（已当待测现象记录在 tasks.md 1.5 ⑧，不猜原因）。
   地形已量录在 tasks.md 1.5（期望值 ~880 条 / 期望错误 ~227 条），两条要点：
   **最小切口是 `core_functions/color/rgb/three_args/*`，不是 `values/colors/**`**（后者几乎全要颜色 `==`
   比较 + lab/lch/oklab 全空间，按本 change 自己的划分属 P8）；前置的 `builtins` **按域拆目录**已做（
   `builtins/mod.rs` + `math.rs` + `color.rs`）。再往下是 1.7 其余分派 → **P7 诊断消息**：
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
- **`HRX_DB` 的相对路径按 crate 目录解析**：`HRX_DB=target/x.db` 会变成`crates/sasspile/target/x.db`，因目录不存在而
  直接报 `unable to open database file`（好在是 exit 1，不是静默错）。空库复算一律用**绝对路径**。
- **harness 不比对 `warning` 段**：不少行同时期望一条 deprecation 警告（如 `core_functions/modules/color/hue::`
  期望 `color.hue() is deprecated`）。这类行“输出对、警告缺”，**不得据此宣称 deprecation 已实现**；
  单例文件（section 直接叫 `input.scss`）的 key 尾部为空（`…hue::`）是合法的，不撞名。
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
