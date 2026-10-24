# scss-parsing Specification

## Purpose
定义 SCSS 源文本到 AST 的流迁移器行为：词法 DFA（`scan_map` 逐字符生长 token 流）、语法栈式折叠（`scan_map(ParseStack)` 边界事件发 owned AST 树）、插值再词法 inner stream 复用；可解析的语法面、插值语义与语法错误的可定位性。

## Requirements

### Requirement: 语句与表达式全覆盖
解析器 MUST 接受 SCSS 全部语句形式：变量声明、规则集（含嵌套与父选择器 `&`）、属性声明、`@mixin`/`@include`（含带参 `@content` 块与关键字实参）、`@function`/`@return`、`@if`/`@else if`/`@else`、`@each`（含多变量解构）、`@for`（from..to/through）、`@while`、`@media`、`@supports`、`@at-root`、`@use`/`@forward`/`@import`、`@extend` 语法、`@error`/`@warn`/`@debug`、带命名前缀的 `@-<vendor>-keyframes` 等 at-rules。表达式 MUST 覆盖：字面量（数字带单位、颜色、字符串带/不带引号、null/true/false）、列表与 map 字面量、二元/一元运算（含 `and`/`or`/`==`/`!=`/`<=` 等）、函数调用、变量引用、`#{}` 插值、带括号的分组。

#### Scenario: 嵌套规则与父选择器
- **WHEN** 解析 `.a { .b& { color: red; } }`
- **THEN** 得到嵌套 statements，其中内层选择器保留 `&` 占位结构供求值期展开

#### Scenario: @each 多变量解构
- **WHEN** 解析 `@each $k, $v in (a: 1, b: 2) { x: $k; }`
- **THEN** AST 中该语句记录两个迭代变量名与 map 表达式

### Requirement: 插值的语法保留
选择器、属性名、at-rule 参数中的 `#{...}` 插值 MUST 被解析为"静态段 + 表达式"交替序列，MUST NOT 在解析期求值。

#### Scenario: 动态属性名
- **WHEN** 解析 `a { #{$prop}-style: 1px; }`
- **THEN** 属性名为插值序列（表达式 `$prop` 与静态后缀 `-style`）

### Requirement: 注释的语法区分
`//` 注释 MUST 被识别为静默注释并从 AST 中排除；`/* */` 注释 MUST 作为可保留节点进入 AST（是否输出由序列化阶段决定）。

#### Scenario: 两种注释并存
- **WHEN** 解析同时含 `// hidden` 与 `/* kept */` 的文件
- **THEN** AST 仅含 `/* kept */` 注释节点

### Requirement: 语法错误可定位
文法错误 MUST 产生携带文件、行列号与出错 token 片段的错误，且 MUST 在不消费后续文件内容的前提下于首个错误处中止。

#### Scenario: 缺失右花括号
- **WHEN** 解析 `a { color: red;`
- **THEN** 错误信息指出未闭合规则块的起始位置与期望的 `}`
