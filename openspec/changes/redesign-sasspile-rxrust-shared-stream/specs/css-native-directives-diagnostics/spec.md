# Spec Delta — css-native-directives-diagnostics(原生 CSS、控制流指令、变量作用域、诊断)

## Purpose

定义 SCSS 的**原生 CSS 面**、**控制流指令**(`@if/@each/@for/@while`)、**变量作用域**,以及
`@debug/@warn/@error` 与 deprecation 的对外行为。逐条可回溯
`sass-spec/spec/{css/**,directives/{if,each,for,while,debug,warn,error}.hrx,variables/**}`。
实现(控制流=flat_map/filter,见 `reactive-stream-architecture`)不在本文件约束内。

## ADDED Requirements

### Requirement: 原生 CSS at-rule 与值透传

未知 at-rule、`@charset`、`@font-face`、`@keyframes`(含百分比帧)、`@propset`、`@supports`、
unicode-range(`U+00-FF`)、`!important`、CSS 转义 MUST **原样透传**且保内容与顺序,纯 CSS 声明块
MUST 不改变。`//` 行注释 MUST 剥离,`/* */` MUST 保留。CSS 自定义媒体查询/选择器里的 Sass 插值仍生效。

参照:`css/{unknown_directive,charset,font-face,keyframes,propset,supports,unicode_range,important,
escape,url,style_rule}.hrx`。

#### Scenario: 未知 at-rule 透传
- **WHEN** `@unknown-thing { a: b }`
- **THEN** 原样输出,不报错

#### Scenario: keyframes 保序
- **WHEN** `@keyframes x { from { a:b } to { c:d } }`
- **THEN** 结构与顺序不变

### Requirement: CSS 自定义属性(custom property)

custom property `--*` MUST 保留其原始值字面(**内部不做 Sass 求值**,即使含 `/`、`()`、操作符);
其中 `#{}` 插值 MUST 生效(可能为空);空值 `--x: ;` MUST 保留;`var(--x)` 引用 MUST 原样透传。

参照:`css/custom_properties/**`。

#### Scenario: 不求值
- **WHEN** `a { --x: 1 + 2 }`
- **THEN** `--x: 1 + 2;`(保留字面)

#### Scenario: 插值生效
- **WHEN** `a { --x: #{""} }`
- **THEN** `--x: ;`(插值为空)

### Requirement: 控制流指令 `@if`/`@each`/`@for`/`@while`

`@if`/`@else if`/`@else` MUST 按真值(仅 `false`/`null`/空为假)选唯一命中分支,其余分支内容 MUST NOT
出现在输出。`@each` MUST 支持 list 与 map(单变量=值、双变量=键值),按条目序展开。`@for $i from a to b`
(不含 b)/`through b`(含 b)。`@while cond` MUST 重复至假,且体中对循环变量的重定义影响下一轮判定。

参照:`directives/if/**`、`each.hrx`、`for/**`、`while.hrx`。

#### Scenario: 唯一命中
- **WHEN** `@if false { a{b:c} } @else if true { d{e:f} } @else { g{h:i} }`
- **THEN** 仅 `d { e: f; }`

#### Scenario: map 双变量迭代
- **WHEN** `@each $k, $v in (a: 1, b: 2) { .#{$k} { v: $v } }`
- **THEN** `.a { v: 1; }` `.b { v: 2; }`

### Requirement: 变量块作用域与写回

变量 MUST 遵循块作用域与文档序可见性;块内定义 MUST NOT 外泄(除非全局)。`!global` MUST 把写回提升到
全局;`!default` MUST 仅在未定义时赋值(与 `@use … with` 协作时已配置令其失效)。裸未定义 `$var` 普通
上下文 MUST 逐字兜底。element-plus BEM mixin 依赖 `@include b()` 内 `$B/$E … !global` 传环境态。

参照:`variables/**`、`libsass/variable-scoping/**`、`element-plus` `mixins/mixins.scss`。

#### Scenario: 不外泄
- **WHEN** `a { $x: 1; b: $x } c { d: $x }`
- **THEN** `a{b:1}`;`c` 中 `$x` 未定义(兜底/按金样报错)

#### Scenario: !global 提升
- **WHEN** `a { $x: 1 !global } b { c: $x }`
- **THEN** `b { c: 1; }`

#### Scenario: 旗标不是值的一部分
- **WHEN** `$a: b !default !default;` 后 `c {d: $a}`(及 `!global` 同形)
- **THEN** `c { d: b; }` —— 重复旗标仅 deprecated 但仍可解析,且旗标 MUST NOT 出现在输出里
  (`variables/double_flag.hrx::default` / `::global`)

#### Scenario: 普通声明的 `!` 文本逐字保留
- **WHEN** `.x { --important: value !important; --multiple: !important !important; }`
- **THEN** 两个自定义属性值原样输出(它们不属上述绑定旗标剔除)
  (`css/custom_properties/exclamation.hrx`)

### Requirement: `@debug` / `@warn` / `@error` 与 deprecation

`@debug` MUST 输出调试值(流式诊断,不中断);`@warn` MUST 输出 `[…]`/消息形式的 warning 不中断;
`@error` MUST 中止并产出 error。所有诊断 MUST 携带 source span;deprecation MUST 带 `[id]` 与
`sass-lang.com/d/<id>` 链接;格式按 `sass-spec` 判定(`ledger` 归一化)。

参照:`directives/{debug,warn,error}.hrx` 及各 `warning`/`error` 期望块。

#### Scenario: @error 中止
- **WHEN** `@error "boom"`
- **THEN** 编译返回 error,消息/位置按金样

#### Scenario: @warn 不中断
- **WHEN** `a { b: c } @warn "heads up"`
- **THEN** `a { b: c; }` 仍产出,附 warning
