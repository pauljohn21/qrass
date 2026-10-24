# Spec Delta

## Purpose

定义求值语义：作用域与变量规则、mixin/function 调用、控制流展开、嵌套选择器合成与 at-rule 包装行为。流视角：`scan_map(Env)` 按值携带作用域栈，`@include` 展开 ≡ `flat_map` inner stream，`@content` ≡ 数据对物化。

## ADDED Requirements

### Requirement: 作用域与变量可见性
变量查找 MUST 遵循块作用域自内向外链式查找；`!default` 仅在变量未定义或为 null 时赋值；`!global` 直接写入模块根作用域；块内首次赋值 MUST 在该块新建绑定而不影响外层同名变量。

#### Scenario: 重复 !default 声明
- **WHEN** 模块先定义 `$emit: false !default;` 之后再次 `$emit: true !default;`
- **THEN** 最终值为 `true`，第二条不覆盖第一条已生效的值

#### Scenario: 块内遮蔽
- **WHEN** 外层 `$i: 1;`，某规则块内 `$i: 2;` 后块外再读 `$i`
- **THEN** 块内为 2，块外仍为 1

### Requirement: mixin 与 @content
`@include` MUST 支持位置实参、命名实参、默认值、`$args...` 可变参数转发；`@content` 块 MUST 以调用点作用域惰性求值（lexical scope），且 `@mixin` 内 `@content` 的参数（如 `@content($x)`）MUST 绑定到内容形参。

#### Scenario: content 形参
- **WHEN** `@mixin m { @include content(2); }` 与 `@include m using ($n) { x: $n; }`
- **THEN** 输出 `x: 2`

### Requirement: 用户自定义函数
`@function` MUST 支持位置/命名/可变参数与 `@return`；函数体 MUST 拥有模块级词法作用域；无 `@return` 到达末尾 MUST 返回 null；函数名与内置全局函数同名时 MUST 由模块命名空间优先规则消解。

#### Scenario: 递归/链式颜色函数
- **WHEN** element-plus 式 `@function calc-main-color($color)` 这类多层调用被求值
- **THEN** 多层调用正确返回计算后的颜色值

### Requirement: 控制流展开
`@if/@else if/@else` MUST 按真值语义（null 与 false 为假，其余为真）；`@each` MUST 遍历 list/map（map 解构键值）；`@for` MUST 支持整数边界与 to/through 语义；`@while` MUST 以布尔表达式迭代直至假值。

#### Scenario: map 解构遍历
- **WHEN** `@each $k, $v in ("a": 1, "b": 2)` 生成声明
- **THEN** 输出包含 a/b 两条对应结果

### Requirement: 嵌套选择器合成
规则嵌套 MUST 展开为完整复合选择器序列：`&` 替换为父选择器全文；无 `&` 时按后代组合；支持 `@at-root`（默认脱离全部祖先，`without recursion` 与 `with/all` 媒体包含语义）、选择器插值、以及 `.a { &:hover {} }` 生成 `.a:hover`。

#### Scenario: 多层嵌套
- **WHEN** `.a { .b { .c { color: red } } }`
- **THEN** 输出仅含 `.a .b .c { color: red; }` 一条规则

### Requirement: 媒体与 supports 包装
`@media` 嵌套 MUST 合并外层条件（`and` 连接）；`@include` 进入 `@media` 内的规则 MUST 上浮至最近媒体块；声明出现在 `@media` 中 MUST 包装输出；`@supports` 行为同理。

#### Scenario: 嵌套 media 合并
- **WHEN** `@media screen { .a { @media (min-width: 500px) { color: red } } }`
- **THEN** 输出 `@media screen and (min-width: 500px) { .a { color: red; } }`

### Requirement: 数值与单位算术
二元运算 MUST 实现带单位数值算术与比较：同单位直接运算；可换算单位（px/in、s/ms、deg/rad 等 sass 规范换算表）自动换算；不可换算单位运算报错；`/` 在算术上下文 MUST 产生除法（配合 math.div 语义）；结果保留左操作数单位。

#### Scenario: 单位换算比较
- **WHEN** `1in == 96px` 参与 `@if`
- **THEN** 判定为真
