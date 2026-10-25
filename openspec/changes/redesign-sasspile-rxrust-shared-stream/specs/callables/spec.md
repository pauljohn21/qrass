# Spec Delta — callables(mixin/include/@content、function/return、参数/实参)

## Purpose

定义 SCSS **可调用域**的对外行为:`@mixin`/`@include`/`@content`/`@yield`、`@function`/`@return`、
参数与实参绑定(位置/命名/默认/var-args/named-splat)、函数/mixin 一等引用。逐条可回溯
`sass-spec/spec/{callable,directives/mixin,directives/function,core_functions/meta}/**` 与
`element-plus`(BEM mixin DSL)。实现(一到多 = 多播)见 `reactive-stream-architecture`。

## ADDED Requirements

### Requirement: `@mixin` 定义与 `@include` 调用

`@mixin` MUST 支持形参:位置、默认值、`$args...` var-args、以及其后 `@content`;`@include` MUST 支持
带/不带括号、位置实参、命名实参 `a($k: v)`、`$list...` splat。调用 MUST 把 mixin 体按实参代入展开到
**调用处的选择器上下文**。`@mixin` 定义 MUST 只解析一次、被多处 `@include` 复用(一到多)。

参照:`directives/mixin/**`、`values/mixins.hrx`、`callable/parameters.hrx`。

#### Scenario: 带参 mixin 展开
- **WHEN** `@mixin m($c){ color: $c } a { @include m(red) }`
- **THEN** `a { color: red; }`

#### Scenario: var-args 收集
- **WHEN** `@mixin m($a, $rest...) { x: length($rest) } @include m(1,2,3)`
- **THEN** `x: 2;`

### Requirement: `@content` 与块传入

`@include m { … }` MUST 把块作为 `@content` 注入 mixin 内出现处,并保留**调用处选择器上下文**
(mixin 内 `&`/嵌套以调用点为准)。`@yield`/named content blocks 按金样。`@content` 缺失而 mixin 用了它
MUST 报错。

参照:`directives/mixin/**`、`core_functions/meta/accepts_content.hrx`、`content_exists.hrx`。

#### Scenario: @content 携选择器上下文
- **WHEN** `@mixin wrap { a { @content } } @include wrap { b: c }`
- **THEN** `a { b: c; }`

### Requirement: `@function` 定义与调用

`@function` MUST 支持形参(默认/var-args)、`@return`;在值/表达式中 `name(args)` MUST 按 Sass 求值并
代入返回值;递归与提前 `@return` MUST 正确。函数名转义/custom-ident 按金样。

参照:`directives/function/**`、`callable/**`、`expressions/functions.hrx`。

#### Scenario: 返回值参与运算
- **WHEN** `@function twice($n){ @return $n*2 } a { w: twice(3px) }`
- **THEN** `a { w: 6px; }`

### Requirement: 参数/实参绑定与 `Argument`

参数绑定 MUST 匹配位置+命名、默认值填充、多余实参/缺参报错;`$args` var-args MUST 收集为(可能带命名
的)参数元组,`@apply`/named-splat 传递。`meta.keywords($args)` MUST 返回命名部分 map;`meta.inspect`
对位置/命名/空分别序列化为 `(1,)`/`(c: 1)`/`()`。

参照:`callable/arguments.hrx`、`callable/parameters.hrx`、`core_functions/meta/{keywords,apply}.hrx`。

#### Scenario: 命名 + 位置混合
- **WHEN** `@mixin a($b, $c: 1){ x:$b y:$c } @include a(2, $c: 3)`
- **THEN** `x: 2; y: 3;`

#### Scenario: keywords 检查
- **WHEN** `@mixin a($args...){ n: meta.keywords($args) } @include a($c: 1)`
- **THEN** `n: (c: 1);`

### Requirement: 一等函数/mixin 引用与自省

`meta.get-function/get-mixin` MUST 返回可调用一等值(跨模块、作用域正确);`meta.call/apply` MUST 以
引用调用;`meta.function-exists/mixin-exists/accepts-content` MUST 按金样。`meta.type-of` 对函数引用
MUST 返回 `function`/`mixin`。

参照:`core_functions/meta/{get_function,get_mixin,call,function_exists,mixin_exists}.hrx`。

#### Scenario: 取并用函数
- **WHEN** `@function f($x){@return $x} a { b: meta.call(meta.get-function("f"), 1) }`
- **THEN** `a { b: 1; }`
