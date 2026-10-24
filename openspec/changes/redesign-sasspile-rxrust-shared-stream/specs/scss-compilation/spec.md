# Spec Delta

## Purpose

定义 `sasspile` 把 SCSS 源编译为展开 CSS 的对外行为契约：输入到输出/错误的可观察结果，
每条断言可回溯到 `sass-spec` 的 `.hrx` 金样或 `element-plus` 真实语料；不约定内部实现。

## ADDED Requirements

### Requirement: 顶层编译产出展开 CSS

`sasspile` MUST 提供把单个 SCSS 源文本编译为**展开（expanded）CSS** 的公开入口；输出 MUST 与
`sass-spec` 对应金样的 `output.css` 逐字节一致（首行 `@charset` 除外的规范化差异需与金样一致），
纯 CSS 语句 MUST 原样透传。源中 `//` 行注释 MUST 被剥离、MUST NOT 出现在输出。

参照：`sass-spec/spec/css/**`、`sass-spec/spec/syntax/**`；`element-plus` 各 `.scss` 编译结果。

#### Scenario: 纯 CSS 透传
- **WHEN** 输入为不含 Sass 特性的普通 CSS 声明块
- **THEN** 输出等价的展开 CSS，选择器与声明顺序不变

#### Scenario: 行注释被剥离
- **WHEN** 源含 `// comment`
- **THEN** 输出不含该注释，且其余内容不变

### Requirement: 嵌套选择器组合

`sasspile` MUST 把嵌套规则按 Sass 展开语义组合到祖先选择器：后代组合以空格连接；`&` MUST 被替换为
父选择器完整文本；多层嵌套 MUST 递归展开为文档序的顶层规则；带声明的中间块 MUST 产出自身的规则。

参照：`sass-spec/spec/directives/**`、`sass-spec/spec/values/mixins.hrx` 中的嵌套用例。

#### Scenario: 后代嵌套展开
- **WHEN** `a { b { c: d; } }`
- **THEN** 产出 `a b { c: d; }`

#### Scenario: 父选择器引用
- **WHEN** `a { color: red; &:hover { color: blue; } }`
- **THEN** 依次产出 `a { color: red; }` 与 `a:hover { color: blue; }`

### Requirement: 变量定义与替换

`sasspile` MUST 支持 `$var: value` 定义、重定义与在值/选择器中引用；变量可见性 MUST 遵循 Sass
文档序与块作用域语义（全局按文档序可见，块内定义不外泄，除非为全局作用域）；裸未定义 `$var` 在
普通上下文 MUST 逐字兜底保留。

参照：`sass-spec/spec/variables/**`。

#### Scenario: 全局变量替换
- **WHEN** `$x: 3px; a { w: $x; }`
- **THEN** 产出 `a { w: 3px; }`

### Requirement: 插值 `#{...}`

`sasspile` MUST 支持 `#{expr}` 插值，作用于选择器、属性名、声明值与指令参数；插值结果 MUST 与
相邻文本紧邻拼接（不加额外空白）；插值表达式内引用未定义变量 MUST 使求值失败，并按金样处理
（丢弃该声明或报错）。

参照：`sass-spec/spec/values/interpolation/**`、`sass-spec/spec/values/strings.hrx`。

#### Scenario: 选择器插值紧邻拼接
- **WHEN** `$k: 2; .col-#{$k} { w: 1px; }`
- **THEN** 产出 `.col-2 { w: 1px; }`

### Requirement: `@if`/`@else` 条件分支

`sasspile` MUST 支持 `@if`、`@else if`、`@else`，按 Sass 真值语义（仅 `false` 与 `null` 为假）
选择唯一命中分支，其余分支 MUST 被完全丢弃（其内容不出现在输出）。

参照：`sass-spec/spec/directives/if/**`。

#### Scenario: 命中 else 分支
- **WHEN** `@if false { x { y: 1; } } @else { z { w: 2; } }`
- **THEN** 仅产出 `z { w: 2; }`

### Requirement: `@each` 列表迭代

`sasspile` MUST 支持 `@each $v in <list>`（列表可为空格/逗号序列、值列表或 map 键值对），对每个
条目以其为循环变量展开 body 一次，展开结果按条目顺序进入输出。

参照：`sass-spec/spec/directives/each/**`。

#### Scenario: 逗号列表展开
- **WHEN** `@each $a in b, c { .#{$a} { d: $a; } }`
- **THEN** 依次产出 `.b { d: b; }` 与 `.c { d: c; }`

### Requirement: `@for` 计数循环

`sasspile` MUST 支持 `@for $i from <a> to <b>`（不含 `b`）与 `@for $i from <a> through <b>`（含 `b`），
按序展开 body，循环变量 `$i` 绑定当前整数值。

参照：`sass-spec/spec/directives/at-for/**`、`sass-spec/spec/directives/**`。

#### Scenario: to 区间展开
- **WHEN** `@for $i from 1 to 3 { .c#{$i} { w: $i; } }`
- **THEN** 依次产出 `.c1 { w: 1; }` 与 `.c2 { w: 2; }`

### Requirement: `@while` 条件循环

`sasspile` MUST 支持 `@while <cond> { <block> }`，在条件为真时反复展开 body，直至条件为假；body 内
对循环变量的重定义 MUST 影响下一轮判定。

参照：`sass-spec/spec/directives/while/**`。

#### Scenario: 递减到假终止
- **WHEN** `$i: 3; @while $i > 0 { a#{$i} { n: $i; } $i: $i - 1; }`
- **THEN** 按 `$i` 从 3 到 1 各产出一条规则后停止

### Requirement: `@mixin` 与 `@include`

`sasspile` MUST 支持 `@mixin` 定义（含带默认值的形参、`@content` 块、`@yield`/var-args）与
`@include` 调用，把 mixin 体按实参代入展开到调用处的选择器上下文中。

参照：`sass-spec/spec/directives/mixin/**`、`sass-spec/spec/values/mixins.hrx`。

#### Scenario: 带参 mixin 展开
- **WHEN** `@mixin m($c) { color: $c; } a { @include m(red); }`
- **THEN** 产出 `a { color: red; }`

### Requirement: 用户自定义函数 `@function`

`sasspile` MUST 支持 `@function` 定义（含形参默认值与 var-args）与 `@return`，并可在值/表达式中
以 `name(args)` 调用；调用 MUST 按 Sass 语义求值并代入返回值。

参照：`sass-spec/spec/callable/**`、`sass-spec/spec/directives/function/**`。

#### Scenario: 函数返回参与运算
- **WHEN** `@function twice($n) { @return $n * 2; } a { w: twice(3px); }`
- **THEN** 产出 `a { w: 6px; }`

### Requirement: `@extend` 与哨兵选择器合并

`sasspile` MUST 支持 `@extend <selector>`（含 `%placeholder` 哨兵）把被扩展选择器的声明并入扩展者
的规则，并按 Sass expanded 语义合并选择器（去重、组合子与 specificity 有序），结果与金样一致。

参照：`sass-spec/spec/directives/extend/**`。

#### Scenario: 哨兵 placeholder 被扩展
- **WHEN** `%c { color: red; } a { @extend %c; }`
- **THEN** 产出 `a { color: red; }`，且 `%c` 不单独输出

### Requirement: `@media` 与 `@supports` 冒泡

`sasspile` MUST 支持 `@media`/`@supports` 查询，把其中的规则按 Sass 媒体冒泡语义提升到顶层，
嵌套媒体 MUST 合并查询条件；`@media` 内的选择器 MUST 组合父选择器。

参照：`sass-spec/spec/directives/media/**`、`sass-spec/spec/directives/supports/**`。

#### Scenario: 媒体内选择器组合
- **WHEN** `a { @media screen { b { c: d; } } }`
- **THEN** 在 `@media screen { ... }` 内产出 `a b { c: d; }`

### Requirement: 模块系统 `@use` 与 `@forward`

`sasspile` MUST 支持 `@use "path" [as ns]` 加载模块并以命名空间访问其成员（变量/函数/mixin），
`@forward` 转发；同一模块被多处 `@use` MUST 只编译一次并把结果喂给各引用方（多播语义），命名空间
冲突与唯一名 MUST 符合 Sass 规则。

参照：`sass-spec/spec/values/modules.hrx`、`sass-spec/spec/core_functions/**` 带命名空间用例。

#### Scenario: 命名空间访问变量
- **WHEN** `_a.scss` 定义 `$x: 1`，主文件 `@use "a"; b { w: a.$x; }`
- **THEN** 产出 `b { w: 1; }`

### Requirement: 内置函数求值

`sasspile` MUST 实现 Sass 内置函数库（`math.*`、`color.*`/旧式颜色函数、`string.*`、`list.*`、
`map.*`、`selector.*`、`meta.*`），对参数按 Sass 语义求值并返回规范化的 `Value`；非 Sass 内置的
CSS 原生函数名（如 `url`/`calc`/`min`/`max`）MUST NOT 被当作内置求值，而 MUST 逐字保留。

参照：`sass-spec/spec/core_functions/**`（含 content/**、* 各域）。

#### Scenario: 内置函数生效
- **WHEN** `a { w: math.div(6, 2) + px; }` 类用例
- **THEN** 与对应 `.hrx` 的 `output.css` 一致

#### Scenario: CSS 原生函数保留
- **WHEN** `a { background: url(http://x); }`
- **THEN** `url(...)` 原样出现在输出

### Requirement: 值类型与 CSS 序列化

`sasspile` MUST 以强类型建模 Sass 值（Number 带单位、String 含引号态、Color、List、Map、Bool、
Null），并在 `to_css` 时按 Sass 规范化输出：逗号规范为 `, `、空白折叠、相邻插值紧邻拼接、数字
单位与精度按金样；纯未求值文本 MUST 逐字节透传。

参照：`sass-spec/spec/values/**`。

#### Scenario: 逗号规范化
- **WHEN** 值含 `a,b`（ Sass 会规范化的上下文）
- **THEN** 输出为 `a, b`

### Requirement: 运算符与数值单位

`sasspile` MUST 支持算术运算符（`+ - * / %`）、比较（`== != < > <= >=`）、布尔（`and or not`）
与 Sass 单位代数（同单位可加减、不同单位报错、乘除按规则），且 MUST NOT 把选择器组合子 `a > b`
误当比较、MUST NOT 把 `font: 12px/1.5` 的 `/` 误当除法。

参照：`sass-spec/spec/operators/**`、`sass-spec/spec/values/calculation/**`、`sass-spec/spec/expressions/**`。

#### Scenario: 同单位相加
- **WHEN** `a { w: 10px + 5px; }`
- **THEN** 产出 `a { w: 15px; }`

#### Scenario: 斜杠在 font 中不作除法
- **WHEN** `a { font: 12px/1.5 sans-serif; }`
- **THEN** 输出保留 `12px/1.5`，不作除法求值

### Requirement: 编译错误形态

`sasspile` MUST 对 `sass-spec` 标注为 error 的用例（语法错误、未定义引用、非法运算、参数不匹配等）
产出结构化错误：错误类型/消息/源码位置 MUST 与金样 `error` 文件的判定一致（消息措辞差异以
`ledger` 的归一化比较为准）。

参照：`sass-spec/spec/**` 中以 `error` 为期望的用例。

#### Scenario: 未定义函数调用报错
- **WHEN** 调用不存在的函数且不符合内置兜底
- **THEN** 编译返回错误且与金样错误判定匹配
