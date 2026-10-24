# scss-callables Specification

## ADDED Requirements

### Requirement: @mixin / @include 与实参绑定
`@mixin m(…) { … }` 声明一个混入；`@include m(args)` 就地展开其主体。实参绑定 MUST 接受位置与
命名（`$name:`）实参、应用声明的默认值、把多余位置实参收进 `rest` 参数，并对同一槽既位置又命名
的传参以金样消息拒绝。

#### Scenario: 命名实参
- **WHEN** `@mixin m($a, $b) { x: $a $b; } @include m($b: 2, $a: 1);`
- **THEN** 输出声明为 `x: 1 2`

### Requirement: @content 与 @yield
`@include m { … }` MUST 把块绑定到 `m` 内的 `@content`；混入 MUST 在原处发射 `@content`，未传块
时不展开任何东西。

#### Scenario: content 块
- **WHEN** `@mixin wrap { a { @content; } } @include wrap { c: d; }`
- **THEN** 输出为 `a {\n  c: d;\n}`

### Requirement: @function / @return
`@function f(…) { … @return v; }` 定义一个返回 owned 值的可调用体；`@return` 退出函数。调用未知
函数 MUST 报出金样的 `Undefined function` 错误。

#### Scenario: 用户函数返回计算值
- **WHEN** `@function double($n) { @return $n * 2; } a { w: double(3px); }`
- **THEN** `w: 6px`

### Requirement: 一等可调用值
`meta.get-function`/`meta.get-mixin` MUST 返回一个带身份语义的可调用值（内置按 `(namespace,name)`
相等、用户定义按引用相等），`meta.call`/`@include` 以可调用值 MUST 调用它，并保留词法环境捕获。

#### Scenario: get-function 后 call
- **WHEN** `$f: meta.get-function("abs"); a { b: meta.call($f, -2); }`
- **THEN** `b: 2`
