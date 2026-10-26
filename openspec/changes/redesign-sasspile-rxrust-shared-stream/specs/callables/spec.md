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

### Requirement: 具名实参的位置是标签,不是引用

实参列表里的 `$name:`(`(` 粘合在 callee 名后或 `,` 之后开始的实参)MUST 按**参数名**理解,
**不得**去环境查找该名字;把它当变量引用会对外观正常的程序报出假的 `Undefined variable`
(实测影面 1,611 条在用用例,主要为 `core_functions/color` 的具名写法)。区分必须落在**词素**上:
调用的 `(` 与 callee 粘合为一个词素(`Word("hwb(")`),而 map 字面量的 `(` 是独立词素(`Word("(")`),
故 `($z: null)` 里的 `$z` 仍按 `map` 键的引用处理。调用尚未参与求值(属本能力边界)时,含具名实参
的整个值 run MUST 保持原样文本,不得部分求值。

参照:`core_functions/color/hwb/four_args::named`、`core_functions/color/is_powerless/named`、
`core_functions/map/has_key.hrx`(具名实参内嵌套 map 字面量)。

#### Scenario: 全具名实参不报错
- **WHEN** `a {b: color.hwb($hue: 180, $whiteness: 30%, $blackness: 40%, $alpha: 0.4)}`
- **THEN** 编译不得失败(四个 `$` 名字都是参数名);待 P4 调用求值后输出金样 `hwb(180, 30%, 40%, 0.4)`

#### Scenario: 不得退成万能兜底
- **WHEN** `a { b: hwb($missing) }`($missing 是真实引用,无 `name:` 形)
- **THEN** 仍 MUST 在真 `Err` 通道上以 `UndefinedVariable` 终止

### Requirement: 一等函数/mixin 引用与自省

`meta.get-function/get-mixin` MUST 返回可调用一等值(跨模块、作用域正确);`meta.call/apply` MUST 以
引用调用;`meta.function-exists/mixin-exists/accepts-content` MUST 按金样。`meta.type-of` 对函数引用
MUST 返回 `function`/`mixin`。

参照:`core_functions/meta/{get_function,get_mixin,call,function_exists,mixin_exists}.hrx`。

#### Scenario: 取并用函数
- **WHEN** `@function f($x){@return $x} a { b: meta.call(meta.get-function("f"), 1) }`
- **THEN** `a { b: 1; }`
