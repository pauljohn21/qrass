# Spec Delta — scss-compilation(全部功能的对外行为契约)

## Purpose

定义 `sasspile` 把 SCSS 源编译为**展开 CSS** 的完整对外功能契约:覆盖值/单位/calculation、选择器/
`@extend`、callable(mixin/function/`@content`)、模块 `@use`/`@forward`/`@import`、`core_functions`、
原生 CSS at-rules、directives、变量作用域、诊断。每条 `WHEN/THEN` MUST 可回溯到 `sass-spec/*.hrx`
金样或 `element-plus` 真实语料;实现"怎么做对"由 `reactive-stream-architecture` spec 约束,本文件只锁
**可观察行为**并就地标注易被写坏成 GC/命令式的点。**唯一参照 = `sass-spec` + `element-plus`,严禁参照
dart-sass 源码/文档推断行为。**

> 约定:参照路径相对 `sass-spec/spec/`。`:todo:`/`non_conformant`/`libsass-*` 用例不作通过判据。

## ADDED Requirements

### Requirement: 顶层编译与注释

`sasspile` MUST 提供 `async compile(source) -> Result<String,SassError>`,产出 expanded CSS,与金样
`output.css` 逐字节一致(`ledger` 归一化)。`//` 行注释 MUST 剥离;`/* */` 保留(MUST NOT 出现在输出
之外的诊断除外)。纯 CSS 语句 MUST 原样透传,选择器/声明顺序不变。

参照:`css/plain/**`、`css/comment.hrx`。

#### Scenario: 纯 CSS 透传
- **WHEN** 输入不含 Sass 特性
- **THEN** 输出等价展开 CSS,顺序不变

#### Scenario: 行注释剥离
- **WHEN** 源含 `// c`
- **THEN** 输出不含该行,余者不变

### Requirement: 规则发射时机(声明段冲刷)

输出文档序 MUST 由"何时发射"保证,而**不是**由事后排序保证。依据 `sass-spec/css/style_rule` 的
`declaration/interleaved/**`:一个块的声明按**段**冲刷 —— ① 遇到嵌套子块**开启**时,先把 enclosing 块的
待定声明段冲刷为一条规则(不 pop 该块);② 块**关闭**时冲刷剩余段;③ 因此一个块被子块隔开时会输出**多条**
同选择器的规则(`around_style_rule`:`.a{b:c}` → `.a .d{e:f}` → `.a{g:h}`)。MUST NOT 采用"一个块只出一条
完整规则 + 按块开启位置排序"的近似(它包不住 ③,且会掩盖真错)。无声明段或根层(空选择器)MUST 不发射。

#### Scenario: 子块前的声明段先出
- **WHEN** 输入 `.a { b: c; .d {e: f} }`
- **THEN** 输出依次为 `.a{b:c}`、`.a .d{e:f}`;`.a` 在**开启 `.d` 时**就冲刷,而非等自己关闭后

#### Scenario: 一块被拆成多段
- **WHEN** 输入 `.a { b: c; .d {e: f} g: h; }`
- **THEN** `.a` 出现两次(各含一段),中间夹 `.a .d`;不得把 `b:c` 与 `g:h` 合并进一条

### Requirement: 嵌套选择器组合(`&`、后代、`@at-root`)

嵌套 MUST 按 Sass 展开语义组合祖先:后代以空格连接;`&` MUST 替换为父选择器完整文本(含 `&:hover`、
`&-suffix`);多层递归展开为文档序顶层规则;带声明的中间块 MUST 产出自身规则。`@at-root
(without: media)` MUST 让规则脱离媒体上下文但保留选择器。

参照:`directives/at_root/property_only.hrx`、`css/selector/**`。

#### Scenario: 后代嵌套
- **WHEN** `a { b { c: d; } }`
- **THEN** `a b { c: d; }`

#### Scenario: 父引用 + 声明
- **WHEN** `a { color: red; &:hover { color: blue; } }`
- **THEN** 依次 `a { color: red; }`、`a:hover { color: blue; }`

#### Scenario: @at-root 脱媒体
- **WHEN** `@media print { a { @at-root (without: media) { b: c } } }`
- **THEN** 顶层产出 `a { b: c; }`

### Requirement: 变量作用域、`!global`、`!default`

`$var: value` 支持定义/重定义/引用;可见性 MUST 遵循块作用域 + 文档序(块内定义不外泄)。`!global`
MUST 把写回提升到全局(element-plus BEM mixin 靠 `$B`/`$E … !global` 传环境态)。`!default` MUST 仅在
未定义时赋值,且与 `@use … with` 配置协作时已配置值 MUST 令 `!default` 失效。裸未定义 `$var` 在普通
上下文 MUST 逐字兜底。

参照:`variables/**`、`libsass/variable-scoping/**`;`element-plus` `mixins/mixins.scss` 的 `$B !global`。

#### Scenario: 全局替换
- **WHEN** `$x: 3px; a { w: $x; }`
- **THEN** `a { w: 3px; }`

#### Scenario: 块内不外泄
- **WHEN** `a { $x: 1; b: $x; } c { d: $x; }`
- **THEN** `a{b:1}`;`c` 中 `$x` 未定义(逐字或按金样报错)

#### Scenario: !global 写回
- **WHEN** `a { $x: 1 !global; } b { c: $x; }`
- **THEN** `b { c: 1; }`

### Requirement: 值类型与 CSS 序列化

MUST 以强类型建模 `Value`:带单位 Number、引号态 String、Color、List(space|comma|**slash** 分隔、
可 bracketed)、Map、Bool、Null;`to_css` MUST 规范化(逗号→`, `、空白折叠、插值紧邻拼接、数字精度按
金样)。单元素列表 MUST 带尾逗号。未求值文本逐字透传。

参照:`values/**`(numbers/lists/maps/colors/identifiers)。

#### Scenario: 逗号规范化
- **WHEN** 值 `a,b`(需规范化上下文)
- **THEN** `a, b`

#### Scenario: 斜杠列表保留
- **WHEN** `a { b: 1/2 }`
- **THEN** `a { b: 1/2; }`(斜杠分隔 list,非除法)

### Requirement: 数字单位代数

number MUST 携带单位并实现 Sass 单位规则:同单位可加减;不同单位相加 MUST 报错;单位less 侧 MUST
采纳对方单位;乘除按单位代数;跨类别换算按 `sass-spec` 表(angle/time/length/…)。精度/退化值
(`-0`、极小)按金样。

参照:`values/numbers/**`、`operators/**`、`libsass/units/**`。

#### Scenario: 同单位加
- **WHEN** `a { w: 10px + 5px }`
- **THEN** `a { w: 15px; }`

#### Scenario: 混单位报错兜底
- **WHEN** `a { w: 5px + 3s }`
- **THEN** 编译错误按金样判定(非法运算)

### Requirement: calculation 表达式(calc/clamp/min/max)

`calc()`/`clamp()`/`min()`/`max()`/`math.*` 计算 MUST 建为**可部分化简的表达式树**:同单位折叠
(`calc(1px + 2px)`→`3px`);跨单位/含 `%`/`var()` **不可解析则整体原式保留**,并规范化嵌套括号
(`calc(1px + (2% * var(--c)))`→`calc(1px + 2% * var(--c))`)。MUST NOT 把无法化简的 calc 强算成数。

参照:`values/calculation/**`。

#### Scenario: 可化简折叠
- **WHEN** `a { b: calc(1px + 2px) }`
- **THEN** `a { b: 3px; }`

#### Scenario: 不可化简原式保留
- **WHEN** `a { b: calc(1px + 2%) }`
- **THEN** `a { b: calc(1px + 2%); }`

### Requirement: 颜色与 CSS Color 4

`Color` MUST 支持 rgb/hsl/hwb + alpha,并(阶段性,依 design P1/P8)支持全色域空间(lab/lch/oklab/
oklch/srgb/display-p3/…)、通道访问、`color.space()/to-space()/to-gamut()/is-in-gamut()/channel()`、
`adjust/change/construct`、gamut 映射与 `*-powerless`。十六进制/`#fff` 等原样序列化。

参照:`values/colors/**`、`core_functions/color/**`。

#### Scenario: 通道访问
- **WHEN** `a { b: color.red(#ff0000) }`
- **THEN** `a { b: 255; }`

### Requirement: 列表与 map

list MUST 支持 space/comma/slash 分隔、bracketed、嵌套、index(1-based,负索引、越界报错);map
MUST 支持 `(k: v)`、键序保持、`map.get/has-key/keys/values/merge/set/remove`、键相等语义。内置函数
MUST 按 Sass 语义,`url()/calc()` 等 CSS 原生 MUST NOT 被当 Sass 求值。

参照:`values/lists/**`、`values/maps/**`、`core_functions/{list,map}/**`。

#### Scenario: 取 nth
- **WHEN** `a { b: nth((c d), 2) }`
- **THEN** `a { b: d; }`

#### Scenario: map.get
- **WHEN** `a { b: map.get(("k":5), "k") }`
- **THEN** `a { b: 5; }`

### Requirement: 字符串与插值

`#{expr}` MUST 作用于选择器/属性名/声明值/指令参数,结果与相邻文本紧邻拼接;插值内未定义变量 MUST
使求值失败(丢该声明或报错)。字符串引号态 MUST 保留/按需去除(quote/unquote/length/upper/lower)。

参照:`values/interpolation/**`、`values/strings/**`、`core_functions/string/**`。

#### Scenario: 选择器插值
- **WHEN** `$k: 2; .col-#{$k} { w: 1px; }`
- **THEN** `.col-2 { w: 1px; }`

#### Scenario: 未定义插值丢声明
- **WHEN** `a { b: #{$nope}; }`
- **THEN** 该声明被丢弃(输出 `a {}` 或空,按金样)

### Requirement: 结构化 Selector 与选择器函数

MUST 有结构化 selector 模型(compound/combinator/伪类),支持 `&`、`:is/:not/:where`、`selector.parse/
nest/unify/is-superselector/append/replace/simple-selectors`。选择器组合子 `a > b` MUST NOT 被当比较求值。

参照:`css/selector/**`、`core_functions/selector/**`、`libsass/selectors/**`。

#### Scenario: selector.nest
- **WHEN** `a { b: selector.nest("c, d e", "f") }`
- **THEN** `a { b: c f, d e f; }`

### Requirement: `@extend` 与 placeholder

`@extend <selector>` MUST 把被扩展目标的声明并入扩展者,并按 Sass expanded 语义合并选择器(去重、
组合子/specificity 有序、**链式/多目标不动点**、`:is()` 等选择器伪类内并集、超选择器裁剪)。
`%placeholder` MUST NOT 单独输出。`@extend … !optional`(目标缺失)MUST 不报错。

参照:`directives/extend/**`(注意含 `:todo: dart-sass` 者不作判据)。

#### Scenario: placeholder 被扩展
- **WHEN** `%c { color: red } a { @extend %c }`
- **THEN** `a { color: red }`,无 `%c` 输出

#### Scenario: 选择器伪类并集
- **WHEN** `:is(midstream){@extend upstream} downstream{@extend midstream} upstream{a:b}`
- **THEN** `upstream, :is(midstream), :is(midstream, downstream) { a: b; }`

### Requirement: `@if`/`@each`/`@for`/`@while` 控制流

`@if`/`@else if`/`@else` MUST 按真值(仅 `false`/`null`/空为假)选唯一命中分支,其余内容丢弃。`@each`
MUST 支持 list/map 迭代;`@for` 支持 `from…to`(不含)/`through`(含),**且起止颠倒时 MUST 递减而非输出空**
(`directives/for/for.hrx::inclusive_backward`);`@while` 条件循环且体中对循环变量的
重定义影响下一轮。**这些是流式扇出/选支,不是命令式重放**(见 reactive spec)。

当前实现边界(逐项已量):`@for` 仅在**头部为字面整数**(`@for $v from <int> through|to <int>` 恰好六个词素)
时展开;循环体中的**声明**逐轮重放到**循环所在的规则**(不开新规则、也不打断父块待定声明段),而体内嵌套块、
变量/表达式界限、`@each`/`@while`/`@if` 仍**未实现**(分别 3.2b/3.2c/3.2d/3.2)。界限无法读出时
MUST NOT 猜测区间(`for.hrx::error/from_float` 期望的是 `1.5 is not an int.` 报错,而不是“凑”一个循环)。

参照:`directives/if/**`、`each.hrx`、`for/**`、`while.hrx`。

#### Scenario: 命中 else
- **WHEN** `@if false { x{y:1} } @else { z{w:2} }`
- **THEN** 仅 `z { w: 2; }`

#### Scenario: @each 展开
- **WHEN** `@each $i in a, b { .#{$i} { d: $i } }`
- **THEN** `.a { d: a; }` `.b { d: b; }` 按序

#### Scenario: @for 区间
- **WHEN** `@for $i from 1 through 3 { .x#{$i}{b:$i} }`
- **THEN** `.x1{b:1}.x2{b:2}.x3{b:3}`

#### Scenario: @for 声明体重放到所在规则
- **WHEN** `a { @for $i from 1 through 5 {b: $i;} }`
- **THEN** 一条规则 `a { b: 1; b: 2; b: 3; b: 4; b: 5; }`(`for.hrx::inclusive_forward/scss`)

#### Scenario: @for 递减与空区间
- **WHEN** `@for $i from 5 through 1` 与 `@for $i from 1 to 1`
- **THEN** 前者依次为 5,4,3,2,1;后者不产生任何声明(空区间不回包、也不拆外层规则)

#### Scenario: 界限读不出时不猜
- **WHEN** `@for $i from 1.5 through 2 { b: $i; }`
- **THEN** 不得展开为任何循环体(当前表现为体未求值);等 P7 后应报 `1.5 is not an int.`

### Requirement: `@mixin`/`@include`/`@content`/`@yield`

`@mixin` MUST 支持形参(默认值、`$args...` var-args、named-splat)与 `@content` 块;`@include` MUST 把
mixin 体按实参代入**调用处选择器上下文**展开;`@content` MUST 携带调用点块。`@include a(...)` 与裸
`a` 形式 MUST 支持。参数元组序列化按 `meta.inspect($args)`(positional `(1,)`、named `(c:1)`、空 `()`)。

参照:`directives/mixin/**`、`callable/**`、`values/mixins.hrx`。

#### Scenario: 带参 mixin
- **WHEN** `@mixin m($c){ color:$c } a { @include m(red) }`
- **THEN** `a { color: red; }`

#### Scenario: @content
- **WHEN** `@mixin wrap{ a{ @content } } @include wrap{ b:c }`
- **THEN** `a { b: c; }`

### Requirement: `@function`/`@return` 与调用

`@function` MUST 支持形参默认/var-args、`@return`,并可在值/表达式中 `name(args)` 调用,按 Sass 语义
求值代入;`meta.get-function`/`call`/`apply` MUST 支持函数引用一等值。

参照:`directives/function/**`、`callable/**`。

#### Scenario: 返回值参与运算
- **WHEN** `@function twice($n){ @return $n*2 } a { w: twice(3px) }`
- **THEN** `a { w: 6px; }`

### Requirement: 内置函数调用分派（签名驱动）

当一个声明值**整体**是一个调用且 callee 在注册表里有签名时，MUST 按签名绑定实参并求值；参数个数与
签名不符时 MUST 终止编译并报出规范自己的消息（而不是默默不报错地输出逐字文本）。`$name:` 形实参 MUST 当作
**参数标签**按名绑定（与书写顺序无关），不得去环境里查这个变量（参 1.2b）。

无签名或当前代数无法表达的值 MUST 保持逐字文本，不得猜测：callee 未知（如 `color.hwb`）、调用只是长串的
一部分（`math.div(6, 3) * 1px`）、结果需要**复合（有理）单位**（`math.div(6in, 3s)`、`math.div(6c, 3d)`）。
括号边界在词法上是**粘在词素里**的（`(` 与 `)` 非 STRUCTURAL且 `(` 不冲刷），读调用时先局部规范化括号边界，
不得因此改动上游分类（`url(http://x)`、`-a-calc(` 的行为不变）。

参照：`core_functions/math/div.hrx`（已落地的签名：`math.div`）。

#### Scenario: 单位律按金样
- **WHEN** `math.div(6, 3)` / `math.div(6px, 3)` / `math.div(6px, 3px)` / `math.div(6in, 3px)`
- **THEN** 依次为 `2` / `2px` / `2` / `192`（可换算单位先换算再约，`1in = 96px`）

#### Scenario: 具名实参与顺序无关
- **WHEN** `a {b: math.div($number2: 3, $number1: 6)}`
- **THEN** `b: 2`（两个 `$label:` 按参数名绑定，均未报 `Undefined variable`）

#### Scenario: arity 消息就是规范的消息
- **WHEN** `math.div(6)` 与 `math.div(6, 3, 1)`
- **THEN** 分别报 `Missing argument $number2.` 与 `Only 2 arguments allowed, but 3 were passed.`
      （单/复数两侧各自变化：`Only 1 argument allowed, but 2 were passed.`）

#### Scenario: 不知道的就不猜
- **WHEN** `color.hwb(180, 50%, 50%)` 或 `math.div(6in, 3s)` 或 `math.div(6, 3) * 1px`
- **THEN** 逐字输出原表达式（不产出臆造的数值，也不臆造错误）

### Requirement: 模块系统 `@use`/`@forward`/`@import` 与加载器

`@use "path" [as ns|as *|as null] [with (…) | (…)]` MUST 加载模块、按命名空间访问成员、`with` 配置
上游 `!default`;`@forward` MUST 支持 `as`/`show`/`hide`/`with` 透传(含 BEM mixin 内部 `@forward`)。
`@import`(legacy)MUST 支持:**原生 CSS `@import` 原样透传并上提到文件顶排在样式规则前**;Sass 部分
内联并带 `[import]` deprecation。加载器 MUST 解析 partial `_x.scss`/`x.scss`/index/显式扩展名。同一路径
模块 MUST 只编译一次(多播语义)。CSS `@import` 排序 MUST 与金样一致。

参照:`directives/use/**`、`forward/**`、`import/**`;`element-plus` `mixins/mixins.scss`(`@forward`)。

#### Scenario: 命名空间成员
- **WHEN** `_a.scss` 有 `$x:1`,主文件 `@use "a"; b { w: a.$x }`
- **THEN** `b { w: 1; }`

#### Scenario: with + !default
- **WHEN** `@use "o" with ($a: configured)`,`_o.scss` 有 `$a: original !default; b{c:$a}`
- **THEN** `b { c: configured; }`

#### Scenario: CSS import 上提
- **WHEN** `@import "rule"; @import "import";`(`rule.scss` 有样式、`import.scss` 有 `@import url(...)`)
- **THEN** `@import url(...)` 排在其前

### Requirement: core_functions(全局 vs 命名空间)

内置函数 MUST 分域提供(math/string/list/map/color/meta/selector);**命名空间形态**(`sass:x` →
`x.y()`/`@use "sass:x"`)与**全局裸名**(legacy,带 `[global-builtin]` deprecation)MUST 分别可用。未知名
MUST `Err→原样` 兜底,`url/calc/min/max` 等 CSS 原生 MUST NOT 被当内置求值。`meta.*`(type-of/inspect/
keywords/exists?/function-exists/get-function/get-mixin/load/load-css/css/feature-exists/get-module)
MUST 支持(自省一等值/模块,与 callable+module 深度耦合)。

参照:`core_functions/**`、`core_functions/global/**`、`core_functions/meta/**`。

#### Scenario: 命名空间 math
- **WHEN** `@use "sass:math"; a { w: math.div(6,2) }`
- **THEN** `a { w: 3; }`

#### Scenario: 原生函数保留
- **WHEN** `a { background: url(http://x) }`
- **THEN** `url(http://x)` 原样

### Requirement: 原生 CSS at-rule 与 custom property 透传

未知 at-rule、`@charset`/`@font-face`/`@keyframes`/`@propset`、CSS `@supports`、unicode-range、
`!important`、转义 MUST 原样透传且不破坏内容与顺序。custom property `--*` MUST 保留原始值字面(内
**不做 Sass 求值**),但其中 `#{}` 插值 MUST 生效;`--*: ;` 空值 MUST 保留。`v-bind()`(Vue 构建期
语法)不属 Sass 范围(交上层预处理)。

参照:`css/**`(custom_properties/plain/unknown_directive/keyframes/font-face/unicode_range/url)。

#### Scenario: custom property 不求值
- **WHEN** `a { --x: 1 + 2 }`
- **THEN** `--x: 1 + 2;`(保留字面)

#### Scenario: 未知 at-rule 透传
- **WHEN** `@unknown-thing { a: b }`
- **THEN** 原样出现

### Requirement: `@media`/`@supports` 冒泡与查询合并

嵌套 `@media`/`@supports` MUST 冒泡到顶层并**逐查询合并**可合并者(`(a)` 套 `(b)`→`@media (a) and (b)`),
不可合并者保留分层;`@media` 内选择器 MUST 组合父选择器。媒体内声明与顺序 MUST 与金样一致。

参照:`css/media/bubbling.hrx`、`css/supports/**`。

#### Scenario: 媒体内选择器组合
- **WHEN** `a { @media screen { b { c: d } } }`
- **THEN** `@media screen { a b { c: d; } }`

### Requirement: `@debug`/`@warn`/`@error`

`@debug`/`@warn` MUST 产出诊断(warning 流)不中断;`@error` MUST 中止并产出 error。诊断文本格式
(级别/`[id]`/source span/trace)MUST 按 `sass-spec` 判定(`ledger` 归一化);deprecation `id` 为硬判。

参照:`directives/debug.hrx`、`warn.hrx`、`error.hrx`;各 `warning`/`error` 期望块。

#### Scenario: @error 中止
- **WHEN** `@error "boom"`
- **THEN** 编译返回 error,消息/位置按金样

### Requirement: @extend/BEM 与 element-plus 端到端(集成判据)

`sasspile` MUST 端到端编译 `element-plus` theme-chalk 的 BEM mixin DSL(`b()/e()/m()/when()` 依赖
`!global` 环境态 + `@content` + `@use as *`/`@forward` + 选择器插值),输出与自基线一致且不劣化、
无 panic。此判据不依赖 `@extend`。

参照:`element-plus/packages/theme-chalk/src/**`。

#### Scenario: BEM 展开
- **WHEN** `@mixin b($n){ .#{$namespace}-#{$n}{@content} } @include b(button){ color:red }` 类构造
- **THEN** 生成对应 `.el-button { color: red; }` 结构的 BEM 规则

#### Scenario: 整库编译
- **WHEN** 编译 theme-chalk 入口
- **THEN** 无 panic、输出稳定(非回归自基线一致)

### Requirement: 编译错误形态(诊断契约)

对 `sass-spec` 标注 error 的用例(语法/未定义引用/非法运算/参数不匹配/模块冲突),`sasspile` MUST 产出
结构化错误并经 `ledger` 归一化判定与金样一致;MUST NOT panic/unwrap 作控制流。可恢复与致命错误的
边界按金样。

参照:`sass-spec/spec/**` 中以 `error` 为期望者。

#### Scenario: 未定义函数
- **WHEN** 调用不存在的内置/用户函数且非 CSS 原生
- **THEN** 返回错误并与金样错误判定匹配
