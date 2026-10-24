# scss-control-flow Specification

## ADDED Requirements

### Requirement: @if / @else if / @else
该指令 MUST 用 Sass 真值性求值条件（仅 `false` 与 `null` 为假），并 MUST 只发射第一个命中的分支体。

#### Scenario: else-if 链取首个命中
- **WHEN** `@if false { a { x:1 } } @else if true { b { y:2 } } @else { c { z:3 } }`
- **THEN** 只发射 `b { y: 2 }`

### Requirement: @each 遍历 list 与 map
`@each $v in <list>` MUST 逐值迭代；`@each $k, $v in <map>` MUST 逐对迭代 map；带多个循环变量的
`@each` MUST 对每个元素解构。

#### Scenario: 遍历 list
- **WHEN** `@each $c in a b c { .#{$c} { x: $c; } }`
- **THEN** 按序发射三条规则 `.a`、`.b`、`.c`

### Requirement: @for 与 @while
`@for $i from <a> through <b>` MUST 含 `b`；`@for $i from <a> to <b>` MUST 不含 `b`。
`@while <cond>` MUST 循环到条件为假，且 MUST NOT 在主体无进展时无限循环（以金样语义为界）。

#### Scenario: through 与 to 之别
- **WHEN** `@for $i from 1 through 3 { a#{$i} { x:$i } }`
- **THEN** 三条规则 `a1`、`a2`、`a3`

### Requirement: @at-root
`@at-root` MUST 把嵌套规则提升到文档根（丢弃祖先选择器），其 `without`/`with` 媒体模式 MUST 遵循
金样对媒体上下文放置的行为。

#### Scenario: at-root 丢弃祖先
- **WHEN** `a { b { @at-root { c { x: y } } } }`
- **THEN** 发射一条根规则 `c { x: y }`
