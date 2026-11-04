# 移植现状长叙述（每相/每族做到哪、为何这么判）— 从 `.qoder/rules/rust.md` 的起手节迁入；逐条实测证据在 `openspec/changes/redesign-sasspile-rxrust-shared-stream/tasks.md`

> 由 `.qoder/rules/rust.md` 迁出（2026-10-28）。

   `Lexer/Structurize/Exec/Render` 在 `crates/sasspile/src/phases.rs`；0′.1/.2/.3/.4/.6/.7 均已合入：真 `Err` 通道已接且
   `UndefinedVariable` 已致命（守卫：头部只认 `#{}`、special 调用参数、plain-CSS at-rule 体）、选择器/属性名是
   `src/selector.rs` 的结构化值（`&` 就是词素，`str::replace` 已删）、四条铁律已成 `sasslint` 机器闸（CI 门已去
   `#[ignore]`）。P1 已落：具名实参不当引用（`expr::named_argument`）、标识符 `+`/`-` 拼接
   （`expr::concatenation`）、绑定旗标剔除（`lexer::without_flags`）、**调用脊柱**（`calls.rs`+`builtins.rs` 注册表：
   `math.div/round/abs/ceil/floor/min/max/unit/is-unitless/percentage/compatible`，含 arity 与类型消息金样）、
   `Value::Bool` 与 Sass 数字序列化（**一律截到小数点后 10 位，小数位不够 10 位的原样输出** ——
   2026-10-27 先由颜色金样纠正为“自然表示不超 10 位**有效**数字则原样”，同日再由
   `core_functions/math/{sin,tan}.hrx::negative_zero_fuzzy` 与 `values/numbers/precision.hrx::very_small/*`
   推翻：那个“有效数字”豁免会把 `1e-12` 原样打出去，语料要 `0`，已删）、**1.4a `Calculation` 第一片**（`src/calculation.rs`：`Value::Calculation`，
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
> 基线数字不在本文重复：唯一副本在 `.qoder/rules/rust.md`「📌 基线」。本文只写增量的历史。

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
     • **当时列为“未做”的三行，现在只剩两行**（run `precision`，net 4，1585→1589；regress=0，空库复算
       同读数）：`sin/tan(-1e-12)` → `0` **不需要** fuzzy 比较，需要的就是上面那条“一律截到 10 位小数”，
       挡住它的正是旧的“10 位有效数字就原样”豁免（已删，四行全绿，`tests/math_trig.rs` 把它们钉回断言）。
       另两行仍红：`atan2(1cm, -10mm)` → `135deg` 要长度因子精确（我们落在 `135.0000000003`）；
       `atan2(1px, 1deg)` 要 `deg` 进维度表（`dimension()` 故意只收已实证的长度/时间/em）。
     • **下一刀已落地：`math.pow`/`sqrt`/`log` 与非有限值的打印形**（新 `builtins/math/exponents.rs`；
       上一轮量定的“30 行 precision 卡在 `math.pow` 未注册”完全成立，108 行 `math/pow/*` 也一样）。
       run `exponents`：**net 188，1589→1777（实质 1612）**，regress=0，空库复算同读数，无效错误仍 53。
     • **非有限数没有裸 CSS 写法** ⇒ 写进 `calc( )`，且单位在括号内各自带 `1`：`calc(NaN)`、
       `calc(infinity)`、`calc(-infinity)`、`calc(NaN * 1deg)`、`calc(infinity * 1px)`、`calc(-infinity / 1px)`。
       **不能复用 `Unit::calc_form`**（它把首个分子单位写成 `<幅值>px`，对 NaN 就成 `NaNdeg`）
       ⇒ 新增 `Unit::calc_product`；一起变绿的有 `values/numbers/{precision,degenerate,bounds,very_large}`
       与 `atan2`/`asin`/`acos`/`sin`/`cos`/`tan` 的无穷行。
     • `number_arg` 之外新提出 `math.rs::unitless(name, value, unit)`：`$x: Expected 1px to have no units.`
       这条措辞现在由反三角、`sqrt`、`pow`、`log`（含可选的 `$base`）共读 —— 同一律不写两遍。
     • 其它逐行的细节：`pow(0, -2)` → `calc(infinity)`、`pow(-0.0, -3)` → `calc(-infinity)` 而
       `pow(-0.0, -3.000000000001)` 又回 `calc(infinity)`（只有奇整数翻转零的符号，
       `pow/base_negative_zero.hrx::with_exponent/*`）；`math.log(2, null)` = 自然对数（**写出的 `null`
       就是“没传 base”**，`log.hrx::base/null`）；`sqrt(-0.0)` → `-0`。金样单测 = `tests/math_exponents.rs`
       （五个）+ `tests/math_trig.rs` 补上 `calc(NaN * 1deg)` 三行。
     • 仍未注册：`random`（8）、`math.$pi`/`$e` 等模块变量（14，tan/log 的多行也卡在它）；
       `atan2(1cm,-10mm)` 要精确长度因子、`atan2(1px,1deg)` 要 `deg` 进 `dimension()` 两行也仍红。
     • **第三片：`math.clamp`/`math.hypot` 已注册，且换算表归一**（新 `builtins/math/combining.rs`，
       run `ordering`：net 47，1777→1824（实质 1659），regress=0，空库复算同读数，无效错误仍 53）。
       表归一：`math.rs` 自带的十位小数长度表已删，`to_base`/`dimension` 改读 `crate::unit::conversion`
       —— `hypot(3cm, 4mm*10, 5q*40, div(6in,2.54), 7px*div(96,2.54))` → `11.6189500386cm` 需要 `q`，
       而 `q` 只在共享表里（两张表就是本会话反复出错的地方）。新增单一比较 `compare`+`compare_named`
       （后者把两个参数名都写进消息，`min`/`max` 用一个不名的消息，两者不能混用）。
     • `clamp` 的取平方向由三行 `preserves_units/{number,max,min}` 定：向内用 `<`（等于上界时给上界），
       向外用 `<=`（等于下界时给下界），且返回**原写形式**（`0.75turn` 不被改写成 deg）；
       `clamp(1, 2, 0)` → `1` 说明折叠是“先贴上界再托下界”，独立两比较会给 `0`。
     • **`module.$name` 是一个词素（run `module-constants`，net 3，1824→1827，无效错误 53 → 20）**：
       `lexer.rs` 新增 `Tok::MemberVar(namespace, name)`，在 `$` 开始的那一字符处把尾巴是 `.` 的待定词切成
       模块前缀（`take_module_prefix`）—— 不能在 `expr` 里拆字符串：“`math.$pi` / `math . $pi` / `5.$pi`”
       在 `Word`+`Var` 分开时根本不可区分（第五次词法粒度实证）。配套：`Expr::MemberVar`、
       `builtins::module_value("math", name)`（`math.rs::constant` 的七个值逐个抄自 `variables.hrx` 的行，
       包括 `$min-number` 是**最小 denormal** 而非 MIN_POSITIVE）、`render` 里回写 `{module}.${name}`
       （未知名字如 `upstream.$a` 必须逐字，不能报 `Undefined variable.` —— 那就是那 33 行的死因）。
       仍未做：`math.$pi * 1e15` 类乘法要的是**指数字面量** `1e15` 被当数读（下一刀）；
       `math.$pi: 0;` → `Cannot modify built-in variable.`（赋值目标还不认带命名空间的名字）。
     • **指数字面量 + “常量参与的乘法”（run `exponents-parse`，net 7，1827→1834，无效错误 20 → 18，
       regress=0，空库复算同读数）**：两半缺一不可。
       ① `operations::number_lexeme` 的数字扫描在 `e`/`E` 处不能停 —— 以前 `1e15` 被读成“数 1 + 单位
       `e15`”（`e15` 这个单位根本不存在，只是扫描断点）；`exponent_len` 只在 `e` 后真有（可带号的）数字时
       纳入，所以 `1e`/`1e-` 仍是单位 `e`（无金样，不外推）。金样：`calc(1e2px)` → `100px`。
       ② `expr::from_tokens` 的 `has_dynamic` 支以前**直接**走插值，于是含模块常量的乘法只会把常量印在
       字面 `* 1e15` 旁边；现在先试 `arithmetic`、失败再回落插值 —— 回落不会放宽任何东西，因为
       `operand_expr` 只认 `Word` 数、已注册调用、计算、`Tok::MemberVar`，普通 `$x`/`#{}` 没有操作数臂
       就会在“每个 span 都要出一个操作数”的计数守卫上失败（语料里**没有**任何一行乘用户变量）。
       新增：`variables::{pi,e,epsilon,min_number}` 4 行 + `no_operator::number/exponent` +
       `tan::{asymptote,negative_asymptote}/radian`（`math.tan(0.5rad * math.$pi)` → `16331239353195370`）。
     • **内置模块成员只读（run `readonly-module`，net 7，1834→1841，regress=0，无效错误仍 18）**：
       `exec::declare` 的目标多一型 `Target::ReadOnly`，判据是“模块表真的认这个名字”而不是“命名空间叫
       math”（所以 `upstream.$a: 0;` 依旧当普通属性，不新增一个没人声明的错）；新致命
       `ErrorKind::ReadOnlyVariable` 已入 `FATAL`。至此 `variables.hrx` 14 行全绿。
     • **“指数的打印侧”这个候选已当场推翻**：我上轮说 `color/mix` 要 `2.147483648e14deg` 这种形式，
       那是从旧 dump 记来的；实地 grep 全语料后：**没有任何一行期望输出要求科学计数法**（命中的 `000e00`、
       `00e100` 是 unicode-range 与十六进制色值里的字面 e），而 `calc(1e2px)` → `100px` 恰好要求**展开**。
       mix 剩下的红行是 **`none` 通道值**（`rgb(none none none)`、`color.mix(…, $method: …)`）。
       不得重启“给输出加指数形式”这条路。
     • **第九次“旧断言前提变了”**：`tests/calculation.rs` 的 `calc(1e2px)` 当时钉的是逐字文本，但它自己的
       注释就写了“修好后这行必须改 100px” —— 这不是假断言而是**到期 TODO**，按注释翻过去即可。判据：这一类
       背后有金样行撑着（与第八次“钉的只是我的选择”不同）。
     • **第八次“旧断言前提被实现推翻”**：`tests/builtins.rs` 拿 `math.max(1px, 2q)`/`math.compatible(1px, 2q)`
       当“表外单位 decline”的例子，`q` 一进共享表这两条就红了 —— 它们钉的本来是我的选择而不是语料行，
       现换成真正无证的 `rem`/`ch`，并把 `q` 的现在行为（`2q`/`true`）一并钉住（无任一 2q 的 fixture 行，
       hrx 也无回归 —— 表外/表内的界限以 `unit::UNITS` 为准，不再拿“我认识哪些单位”当证据）。
   而 `complement` 暂不能动，因为
   `complement(turquoise)` → `#e04050`（整字节）与 `complement(rgba(turquoise, 0.7))` → 百分比形态这对差别
   还没解释清（已当待测现象记录在 tasks.md 1.5 ⑧，不猜原因）。
   地形已量录在 tasks.md 1.5（期望值 ~880 条 / 期望错误 ~227 条），两条要点：
   **最小切口是 `core_functions/color/rgb/three_args/*`，不是 `values/colors/**`**（后者几乎全要颜色 `==`
   比较 + lab/lch/oklab 全空间，按本 change 自己的划分属 P8）；前置的 `builtins` **按域拆目录**已做（
   `builtins/mod.rs` + `math.rs` + `color.rs`）。再往下是 1.7 其余分派 → **P7 诊断消息**：
   语料里大量 `error` 用例缺的是**消息**而不是错误管道，现已积到单条级别（例：
   `math/unitless::error/wrong_name` 要 `Undefined function.`，即 1.4d 唯一未复现的候选行）。
## 壳路（`brackets::CALCULATION_NAMES`）与 P4.2 门槛（2026-10-28 实测）

• **壳名集共 17 名**：`calc/min/max/clamp`（1.4a 三族）+ 四个反三角
  `acos/asin/atan/atan2` + `sqrt/exp/log/pow/sin/cos/tan/hypot/rem`。准入判据只有一条：**该件自己的
  `values/calculation/<name>.hrx::simplification` 行陈述了壳律**（名字保留、能算的算掉、读不成的原样留在壳里）。
  折叠在哪里停下是两个不同的原因，不得再合并成一句：“长度与角度无公共进制”是我的虚构 ——
  `hypot(…, -7px + 4em)` 不折是因为 `em` 是上下文相对单位（`unit::conversion` 故意不含它，
  `core_functions/math/comparable.hrx::unit/to_different`），而 `log(…, var(--e))` 保留是因为该实参
  根本读不成值。
• **`abs`/`sign` 试过、量过、又撤了**（不是没考虑）：加进去是 `regress=0 net=5`，但 `ledger changed`
  指出两行新值与金样相反（`abs::preserves_units` 出 `-1.75px`、金样 `1.75px`；`sign::preserves_units`
  出 `-1.75px`、金样 `-1px`）。因由是结构性的：**单参壳声称的值就是它的实参（恒等）**，这对
  `min.hrx::simplified/only`（`min(1px)` → `1px`）对，对名字自己还要对值再运算的 `abs`/`sign` 不对。
• **arity 律已收口**：`calculation::one_argument_value` 规定**只有单参计算才可声称有值**，两个构造点
  （非 `calc` 壳、`calc` 的多参支）一起走。原由：`folded` 的 `find_map` 取首个带值 part，于是
  `math.div(1, min(10, 20))` 出 `0.1` —— 语料无行能把这件事照出来，`regress`/`net` 全盲。`rem` 只因为
  这条闸才留在名单内（否则它的行会产出 `-0.1428571429` 这种发明值）。
• **`values/calculation` 剩下的收益全部汇到 P4.2**：裸全局名注册、`simplified/*` 比较求值族（16 行）、
  `case_insensitive`、`error/unit(s)/*` 消息族——都要求“用户函数能赢过全局内置”先成立。理由（实测）：
  `min.hrx::overridden` 定义 `@function min($arg1, $arg2) {@return $arg1}` 并调 `min(2, 1)`，金样要 `2`。
• **P4.2 本身是能力不是判据**：无块 at-rule 在事件层**没有形状**（`MsgAcc::push_decl` 只在 run 含 `:` 时
  发 `Event::Decl`，`@return $arg;` 整 run 被丢），声明体在 `Scope.inert` 处被整块丢弃（`inert` 与“循环内
  嵌套块”共用，所以收集必须按 callable 归属分流）；`grep -rn '@return\|Return' crates/sasspile/src` → 0。
  设计件在 `openspec/changes/redesign-sasspile-rxrust-shared-stream/design.md` §9（D1–D6 + 首组 RED 行 +
  风险退路），逐条带 `file:line` 已经 `gc-auditor` 核过。簇量级：`callable` 42 失败/29 通过、
  `directives/function` 27/12、`callable/parameters` 18/0；全库 `overridden` 语料 22 段、**当前为红 21 行**。

架构结论**不得**凭记忆重推；已作废方向（逐行教程、`crates/docsizer`）不得重启——docsizer 已从 workspace
成员里删除（再引用会直接弄坏构建），其教程 change 与 `docs/rxrust-from-scratch/` 仅作历史。

## 全量代码审查驱动的五批修复（2026-10-29）——不新增能力，只消缺陷与重复真源

本段只指路，不复述读数（基线数字唯一副本在 `.qoder/rules/rust.md` §基线，逐批读数与三条无金样的律在
`tasks.md` 9.6）：括号深度律收成 `lexer::Tok::net` 一份、hue 周期律只留 `axes::axes_to_rgb` 一处折返、
`@for` 区间不再物化（`loops::Walk`）、`expr.rs` 拆回门槛内、阶段 span 补 `module` 且 `compile` span 带 `id`。
**它们没让任何一行 hrx 变绿**（`regress=0 net=0 changed=0`），所以不得当作能力引用。
仍未做：`@for` 扇出的**载体**（下文 9.8 段；不是“扇出住哪个内核”——那个已定在结构相切、`Exec` 重灌）；
块级 `Env` 作用域（tasks 3.6/4.1）、`calc.rs` 的 `.ok()` 与 `expr.rs` 的 `?` 判据不一、design.md:47 的全局 fold pass 均未动。

已推进一步的（2026-10-29 9.7→9.8）：`@for` 的**一到多扇出已成立**且**边界已归位**——结构相
（`structure::MsgAcc{depth, cut}`）在闭合自己的 `}` 时发一个 owned `Event::For{head, body}`，
`Exec` 侧的 `Scope::loop_`/`ForLoop{body, open}`/`capture`/`replay` 四符号已删（自查用 `grep -rEn`，
BRE 版是条永远为绿的量具），`directives/for/for.hrx::to_scope` 为实质通过。
**未做的仍是载体**：`exec::unroll`/`pass` 以 `try_for_each` + 每轮一次 `body.clone()` 重灌 `step`，
而 `flat_map` 这个动词在 rc.6 里存在，所以这是**未做的选型**（`State` 改为流携带的累加子 = 0′.5），
不得说成框架限制。逐条开放项与读数在 `tasks.md` 9.7/9.8。

## 色彩空档拆到“常数层”（2026-11-03，9.14）——常数不得再靠手抄

• **白点适应表与参考白现在都只有一个来源**。`BRADFORD`（D50→D65）由
  `cargo run -q -p sasspile --example calibrate_space` 从 `to_space/xyz_d50/xyz.hrx` 解出（第一列走
  `out_of_range/far` 的 `1e6` 输入⇒元素可知到 `~1e-16`；另两列走 `white`+`out_of_range/near`，行列式 `1.6`；
  第一版用 `(0.1 0.2 0.3)`/`(0.2 0.4 0.8)` 这一对，行列式 `0.04`，把语料自身的 `5e-11` 打印舍入放大四十倍，已换）。
  `D50` 不再是字面量而是 `d50()` 导出（srgb primaries 行和 → 反解适应），所以**中性是构造性事实**；
  本批之前它单独把 `to_space/srgb/lab.hrx::white` 的色度顶到 `1.274e-7`。
• **剩下的是精度天花板，不是数学缺口**：十位小数的行只能给出每元素 `~5e-11` 的表，所以由它反解的一侧
  仍有三行差最后一名（具名在 `tasks.md` 9.14 与 `tests/color_adaptation.rs` 头部）；它们**故意不入守卫**。
  拿它们当“进步”或当“已接金样”都不准；拿它们当“下一批还要改适应该表”也不准——要么找到语料能支持更高
  条件的行（全仓 `to_space/**` 里只有 X 通道有 `1e6` 量级的行），要么就接受这个界。
• **primaries 表仍是手抄的，但错的只是第 9 位小数**（本批未碰，`farther=101` 里 96 条的两侧之一带现代空间）：
  量具对 `xyz → display-p3-linear` 解出的矩阵，其逆与 `space.rs::matrix` 里那份表逐项差到 `1e-11`，只有两项例外：
  `[2][1]` 差 `5.0e-9`、`[2][2]` 差 `6.9e-9`——而 `to_space/xyz/display_p3.hrx::white` 本编译器得
  `1.0670616622 0.9807650128 0.9623860263`，语料得 `1.067061664 0.9807650127 0.9623860213`
  （逐项差 `1.8e-9`/`1e-10`/`5e-9`）。量级对得上，所以这不再是“表错一大截”而是同一批转录噪声。
  **订正（本会话自己犯的严重记录错）**：上一批本节写过“与金样差到 7%，金样是 `1 1 1`”——那个 `1 1 1` 是我**凭预期补全
  被 `cut -c1-230` 截断的 want 字段**造出来的，语料从来没这么写（输入 `color(xyz 1 1 1)` 不是白点，答案当然不是 `1 1 1`）。
  教训入 `tooling-redlines.md`：被自己截断的输出不是读数，宁可重跑也不准补全。下一批按同一方法从
  `to_space/xyz/<space>_linear.hrx` 解（线性传递⇒仍是纯线性，不沾曲线）。
• **但“下一批”碰到的不是精度，是一个反接的曲线（9.17）**：`a98-rgb` 的 `Transfer` 把指数装在错误的方向上
  （`1/2.19921` 解码、`2.19921` 编码），而语料是 `linear = printed^(563/256)`。这一对看上去对称（`(1,1,1)`
  在任何指数下都是 1），所以前几批都没报出来；分界的则是**残差**：量具现在接受一个待验的解码指数并在线性化读数上
  拟合，`563/256` → `5.0e-11`（语料第十位半步）、取整的 `2.19921` → `4.3e-6`、原方向 `0.4547` → `6.6e9`。
  同一探针对 `prophoto-rgb` 给出 `1.8` → `4.9e-11` vs `1/1.8` → `7.3e6`，说明它的方向本来就对——
  这是一条分支的错，不是约定的错。曲线修好后同一规则只换“超出自身逐项散布”的矩阵项，+96 行。
  `rec2020` 不同：它的曲线带 `α` 偏移，纯幂假设拟合出来的东西会被**verdict 判为不成立**（不是按名字拒：
  早期注释说“分段空间列不进可解名单”，实测 `-- rec2020 xyz 2.2` 照样吐表并 exit 0，现已改为 exit 4）。
• **同一方法的下一个空间把“白点住在哪里”也收掉了（9.18）**：`prophoto-rgb` 的表拟合后九项全部超出自身散布，
  整表归语料；而拟合出的表**自身就落在 D65 白上**，所以 `Space::white()` 不再把它归入 D50——以前是“表一套参考、
  常数另一套参考”，同一个事实住两处。+110 行（基线 3028，26.3%）。**两个盲区现在都是读数而不是推测**：
  `smallest_curved_printed_channel` 说 prophoto **作源**的那八行从不低于 `0.1`，即**解码分段支零约束**（作目标时
  语料确有 `0.0010727236` 这样的通道，那一批约束的是编码支 `1/512`——盲区只在解码支）；“哪侧带曲线”
  从 `"from"/"to"` 字符串收为枚举（改时编译器立刻报 `non-exhaustive patterns`，说明字符串版根本封不住写错的标签）。
  出处注释把 `space.rs` 顶到 503 行⇒五张 primaries 表（四张字面 + `srgb` 委派）拆到 `color/primaries.rs`，
  拆后重测 `net=0` 确认行为无变。
• **补跑的审查把“白点只声明一次”这句话只做到了一半（9.19）**：表归一了，`Space::white()` 却仍靠
  `_ => White::D65` 给未列出的变体默认回答——同一文件自己的 `primaries()` doc 就是用这个理由拆掉旧
  `unwrap_or(&BRADFORD)` 的，所以已改为**穷举 13 个变体**。金样直接指出了一个被我归错类的缺陷：
  `to_space/rec2020/xyz.hrx::white` 写 `color(xyz 0.9504559271 1 1.0890577508)`，而抄来的 rec2020 表行和到
  `color(xyz 0.9504559271 0.9999999304 1.0890577507)` — 输入 `(1,1,1)` 对任何曲线不变，那 `7e-8` 是**表**的错，
  不得计在“曲线假设未定”头上。以后拿“行和”校一张表是免费的曲线无关约束。
• **那条免费约束当场把一个曲线缺陷分开了（9.20）**：给量具加 `α` 参数去测“带偏移的假设”，结果语料用的是
  **无偏移的纯有符号幂 `2.4`**（原写法 `((v+α−1)/α)^(1/0.45)` 残差 `0.214`，`2.4/1.0` 残差 `4.9e-11`；六个候选全表
  在 `tasks.md` 9.20）。更重的一条方法论：**分段支不能靠类比删，也不能只看 `net`**——rec2020 的 `4.5×` 臂删了多得
  41 行（语料正向偏好，且该读数只说**编码侧**：线性最小通道 `0.00163 < 0.081`，而打印侧 `0.0689 > 0.081/4.5`）；
  prophoto 的臂删了 `net=0`，看上去“看不见”，但 `ledger changed` 查出 **11 行变值且臂侧靠金样**
  （`to_space/oklab/prophoto_rgb.hrx::missing/non_lightness`：金样 `0.016`，留臂 `0.0159999996`，删臂 `0.0215443466`）——
  那些是 `missing/*` 行，两例都红，`net` 永远闭嘴。教训：**删一条分段支必须同时读 `changed`**，否则会把
  “臂是对的”误记成“臂无关紧要”。同样，“联合
  约束优先于单项平局”是新的覆盖理由：两项各自差不过自身散布，但它们构成的整行行和被白行直接钉住，拟合值
  满足而转录值差 `1.1e-10` → 拟合值有权覆盖（+15 行）。本会话首个 `farther=0`（closer=131）；代价是中间变体曾绿
  的 16 行回到红态（全为末位 1，两侧都有：作目标用逆阵、作源用正阵），已逐条具名而不是藏起。
• **分段支不是一个律，是逐空间各自的读数（9.21、结论入 9.22）**：同一个实验（删臂→看语料怎么说）在三个空间
  给出三个不同答案：`srgb` 的 `12.92` 臂**被正向确认**（删臂会丢行，且有几行两例都红、只有在 `changed`/`shift`
  里才看得见）；`rec2020` 的 `4.5×` 臂在**编码侧被拒**（解码侧仍零约束）；`prophoto` 的 `1/16` 臂**靠 `changed`
  保留**。具体读数只住 `tasks.md` 9.21/9.22，这里不拷数字。**拐点（`0.04045`/`0.0031308`）仍是转录**：
  源侧最小打印通道 `0.1` 只能给它一个上界，而量具没法只改阈值做 A/B（它只能整臂删）——这条限制已写在
  `tests/color_srgb_segment.rs` 头部，不让它读起来像已证。
• **两台工具在本会话里各自拆了一次（9.21/9.22）**：例件从 491 行降到三百行内，归档读取在
  `tests/common/hrx_read.rs`、数学在 `tests/common/fit.rs`，而 `fit.rs` 现在真的接进了
  `tests/calibrate_fit.rs`（合成行解回已知阵 + 错的阵必被 `verdict` 拒，双向都有）——因为库里每一张
  贴上去的拟合表都是这几个函数产的。`calibrate_space.rs` 里那张“哪些空间线性印出”的名单已删：
  它与 `Space::transfer()` 已经飘过（9.19 记的），而把 `color` 模块抬到 crate 公共面不该由工具默默决定；
  现在准入只由残差判决定（曲线侧当直线拟合会被 exit 4 拒掉，实测）。
• **缺通道已经拆成四块，其中一块已落（9.15）**：索引对齐族（`srgb`/`srgb-linear`/`display-p3*`/`a98-rgb`/
  `prophoto-rgb`/`rec2020`/`xyz`/`xyz-d50`）之间**按位置保留 `none`**（缺者仍作为 `0` 入算术），**任一侧**为派生族时
  代入 `0`，全缺则全缺——但 **`hsl` 形体例外**：它没有印 `none` 的手段，所以
  `to_space/srgb/hsl.hrx::missing/all` 答 `hsl(0, 0%, 0%)` 而不是 `hsl(none none none)`。注意“目标为派生”方向的行
  仍红在第十位（那是 9.14 的天花板，不是本律验过了）。判据入口是一个 `Space::is_derived()`（一族划分只写一次，
  `is_functional` 与缺通道传播都读它，因为“两个臂列表就是一个划分的两个答案”）。
  **剩下三块不得当作已做**：(a) 源为 legacy 形式时要 `color::Channel` 能携带“缺”（值模型改动，230 行）；
  (b) polar 对——**不是“金样互斥”**而是“22 行里 20 行合一条候选律，只有 `to_space/lch/lab.hrx::missing/{lightness,non_hue}`
  两行破律”（破的是 `lab` 自己，`oklch` 同形合律），逐行在 `tasks.md` 9.15；
  (d) **`hwb` 目标有裸金样，不是“没定形体”**：`to_space/hwb/hwb.hrx` 不用 mixin，`::in_range`→`hsl(0, 55.5555555556%, 45%)`
  而 `::missing/non_hue`→`hwb(10deg none none)`，“带不带缺”决定形体是语料自己的 区分；而
  `builtins/color/transfer.rs` 现在根本不接 `hwb` 目标，它的那句“靠 mixin 打印” 只对 `to_space/srgb/hwb.hrx` 成立。
• **当年列的 (a) 已做了一半，而且方向是被两行金样定的（9.23）**：`Channel` 的数值位取成 `Option<f64>`，
  legacy `rgb(none 20 30)` 不再“解不了就逐字回显”。选这一侧而不是“把带缺的颜色当成 `InSpace`”，靠的是同一调用
  在两种源上的不同答案：`to_space/rgb/rgb.hrx::missing/red` 要 `rgb(none 20 30)`（保 `none`、保 byte 尺度，
  但**换成空格形**，因为逗号形没有 `none` 的写法），而 `to_space/srgb/rgb.hrx::missing/red` 要 `rgb(0%, 20%, 30%)`
  （缺位代入）。也就是说：**缺在谁身上，谁就带它出去；不在谁身上，目标形态自己算。** `converted()` 里那三条
  律一行没加，只是源侧读数扩到了 legacy 三元组——“一个划分两份说法”的坑这次提前避开。
  这一批留下的不是“做完了”，而是三件更具体的事：`hsl`/`hwb` 的 `none` 需要 `Space::Hsl`/`Space::Hwb` 与轴数学
  （不只是解析，且该簇里有我还没读懂的形状：`hsl(none none none)` 转 `hwb` 竟打 `red`）；转换后该打**命名色**
  的一簇（金样答裸命名色的行 = 67，按 `grep -rh "^  b: [a-z]*;$" …/to_space/ | wc -l` 量，分布在**目标**为 `rgb` 的
  归档如 `to_space/hwb/rgb.hrx::black` 要 `black` 而我打 `#000000`）；以及缺 **alpha** 与
  “修改缺通道”的逐字错误消息（语料已把消息写全：`grep -rc "currently support modifying missing channels" …` = 25 行，5 个归档）。`is_derived()` 这个名字在这一批后已经会误导：
  它现在区分的是“索引对齐”，不再是“XYZ 导出”——`hsl`/`hwb` 将来入表时必须收成一个三值判据或改名。
• **两条方法论收获**（都是量出来的，不是推的）：(1) “从逐字回显变成有答案但差 9~10 位”**不能归给本批也不能
  归给天花板**，除非有等价断言——新测试要求 `rgb(none 20 30)` 与 `rgb(0 20 30)` 在四个派生目标下逐字相同，
  这才把残差钉在共用的 cone/primaries 表上；(2) 守卫条目自己也会过期：`color_scale.rs` 把
  `color.scale(rgb(none none none))` 钉成“保持原文”，而 `scale/no_channels.hrx::missing` 的金样是一个答案——
  删条目不是放松守卫，是语料换了判据，且必须写明“是哪一行、在哪个 run 里转绿的”（该 key 基线 1 / v1 0，
  不是 v2——写错 run 名也是一种不可复现的引用）。
• **“缺”现在也能被写进去（9.25）**：`change/rgb.hrx::{red,green,blue}/none` 与 `change/space.hrx::
  missing/arg/legacy/same/implicit` 四行金样把 `Color::with_channel` 的值位推成 `Option<f64>`，而 `adjust`/`scale`
  继续拒绝 `none`（没行请它们移动一个缺）。命名色被写入缺后不再打回关键词（`pink` → `rgb(none 192 203)`），
  因为“要说出 `none` 就得换形”跟 9.23 那条是同一回事的两个方向。余下的 51 条不是同一个缺口：39 条等
  “`change` 打在带空间的色上”整条接线（连百分因子、hue 折叠、负彩度折 hue+180，以及四行金样竟要求发出
  `color-mix(in lab, …)` 文本）、6 条等 `$space:` 写入、5 条等 axes 三元组能携缺、1 条等 `alpha: Option<f64>`。
  本批留下两条教训。**(1) 次序是承重的**：把字节臂里“先读参数、再读当前值”换倒，`regress=42`——
  那 42 条错误行的消息就是从读参数那一步出来的，而 `modify.rs` 自己早已写下这句话；我没读它就动手。
  改一条链之前先读它自己标下的“为什么是这个顺序”，这句话比任何新闸都便宜。**(2) “簇多大”要给口径，而口径不是全集**：
  9.23 写的“`change/rgb::` 4 + `change/space::missing/arg` 7 = 11”本批先被我判为“跑不出来的数”——**那是错的**
  （按前缀口径逐字可复现），真正的毛病是**集合选错**：那 4 里混进一条无关的 `green/out_of_range`，而同一律按
  key 口径（以 `/none` 结尾）是 48 条、按 input 口径（实参含 `: none`）是 55 条，两者之差就是
  `change/space::missing/arg/*` 那 7 条（key 名字不结尾于 `/none`）。教训比“记得写命令”进一步：**任何单一口径
  都只能回答它自己那个问题**，把它的数当作“该律未做全集”会把后面几批引向错误的规模估计（而把自己的错
  说成“同一个错第两次犯”也是一次过度更正——它同样是没量就写）。

