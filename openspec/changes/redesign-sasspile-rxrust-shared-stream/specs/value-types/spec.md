# Spec Delta — value-types(数/单位/calc/list/map/颜色/字符串/插值)

## Purpose

定义 SCSS **值域**的对外行为契约(数字单位代数、calculation 表达式、list/map、颜色/CSS Color 4、
字符串/插值),逐条可回溯 `sass-spec/spec/{values,operators,expressions,core_functions}/**` 与
`element-plus`。实现约束见 `reactive-stream-architecture`;本文件只锁可观察行为。

## ADDED Requirements

### Requirement: 数字与单位代数

number MUST 携带单位并实现换算表:length(px/cm/mm/in/pt/pc/q/em/rem/vh/vw/vmin/vmax/ch/ex)、
angle(deg/grad/rad/turn)、time(s/ms)、frequency(Hz/kHz)、resolution(dpi/dpcm/dppx)、`%`。同单位可
加减;单位less MUST 采纳对方单位;**不可换算/不兼容单位相加 MUST 报错**;乘除按单位代数产生复合单位。
退化/负零/精度按金样。`1/2` 在普通值上下文 MUST 视为斜杠分隔 list 而非除法。

参照:`values/numbers/**`、`values/numbers/units/**`、`operators/**`。

#### Scenario: 同单位相加
- **WHEN** `a { w: 10px + 5px }`
- **THEN** `a { w: 15px; }`

#### Scenario: 单位less 采纳
- **WHEN** `a { w: 10px + 3 }`
- **THEN** `a { w: 13px; }`

#### Scenario: 不兼容单位报错
- **WHEN** `a { w: 5px + 3s }`
- **THEN** 非法运算错误(按 error 金样判定)

#### Scenario: 斜杠不作除法
- **WHEN** `a { w: 1/2 }`
- **THEN** `a { w: 1/2; }`(斜杠 list)

### Requirement: calculation 表达式树

`calc()`/`clamp()`/`min()`/`max()`/`math.*` MUST 建为**可部分化简的表达式树**:同类可折叠
(`calc(1px + 2px)`→`3px`);混单位/含 `%`/`var()`/未知 MUST **原式保留**并规范化嵌套括号;`min/max`
跨单位比较按金样;`round(..., strategy)`/`math.ceil/floor/abs/sqrt/pow/log/exp` 按 Sass 语义。CSS 原生
`calc(var(...))` 等 MUST NOT 被强算。

参照:`values/calculation/**`(calc/*、min/max/clamp/round/*)。

#### Scenario: 可化简折叠
- **WHEN** `a { b: calc(1px + 2px) }`
- **THEN** `a { b: 3px; }`

#### Scenario: 不可化简原式保留 + 括号展平
- **WHEN** `a { b: calc(1px + (2% + var(--c))) }`
- **THEN** `a { b: calc(1px + 2% + var(--c)); }`

### Requirement: 列表(list)

list MUST 支持空格/逗号/**斜杠**三种分隔符、可 bracketed `[..]`、可嵌套;索引 1-based、负索引从尾、越界
MUST 报错;`nth/length/index/append/join/set-nth/separator/is-bracketed` 按金样。单元素逗号 list MUST
序列化带尾逗号 `(1,)`。

参照:`values/lists/**`、`core_functions/list/**`。

#### Scenario: 负索引
- **WHEN** `a { b: nth((c d), -1) }`
- **THEN** `a { b: d; }`

#### Scenario: 越界报错
- **WHEN** `a { b: nth((c d), 0) }`
- **THEN** 错误(第一参数须 1..n)

#### Scenario: 单元素列表尾逗号
- **WHEN** 对单元素位置参数组 `meta.inspect`
- **THEN** 得 `(1,)`

### Requirement: map

map MUST 保**插入序**、键相等按 Sass 值比较;`get/has-key/keys/values/merge/set/remove` 按金样;缺失键
`get` MUST 返回 `null`(序列化为空);非 map 参数 MUST 报错兜底。`(...)` 空 map、嵌套 map/list 按金样。

参照:`values/maps/**`、`core_functions/map/**`。

#### Scenario: 取键
- **WHEN** `a { b: map.get(("k":5), "k") }`
- **THEN** `a { b: 5; }`

#### Scenario: 缺失键为空
- **WHEN** `a { b: map.get(("k":5), "z") }`
- **THEN** `a { b: ; }`(null→空)

### Requirement: 颜色与 CSS Color 4

`Color` MUST 支持 rgb/hsl/hwb + alpha、`#hex`/`#fff`/8/4 位、`rgb(0 255 0 / .2)` 现代语法;通道
`red/green/blue/hue/saturation/lightness/alpha/blackness/whiteness`;`adjust/change/construct/scale/
mix/complement/invert/grayscale/lighten/darken/…`;**色域空间** lab/lch/oklab/oklch/srgb/srgb-linear/
display-p3/prophoto-rgb/rec2020/xyz/a98-rgb 及 `color.space()/to-space()/to-gamut()/is-in-gamut()/
channel()/*-powerless()`;跨空间换算与 gamut 映射按金样。命名色(MUST 与原样透传并存)。

参照:`values/colors/**`、`core_functions/color/**`、`core_functions/modules/color/**`。

#### Scenario: 通道
- **WHEN** `a { b: color.red(#ff0000) }`
- **THEN** `a { b: 255; }`

#### Scenario: mix
- **WHEN** `a { b: color.mix(red, blue) }`
- **THEN** `a { b: #800080; }`

### Requirement: 字符串与插值

`#{expr}` MUST 作用于选择器/属性名/声明值/指令参数/`@media`/map 键,与相邻文本紧邻拼接(无额外空白);
插值内未定义变量 MUST 求值失败(丢该声明或按金样报错)。字符串 MUST 管引号态(quote/unquote/length/
slice/index/to-upper-case/to-lower-case/insert);`ident` 转义/规范化按金样。

参照:`values/interpolation/**`、`values/strings/**`、`core_functions/string/**`。

#### Scenario: 选择器插值
- **WHEN** `$k: 2; .col-#{$k} { w: 1px }`
- **THEN** `.col-2 { w: 1px; }`

#### Scenario: 未定义插值丢声明
- **WHEN** `a { b: #{$nope} }`
- **THEN** 该声明丢弃(按金样)
