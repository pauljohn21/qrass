# Proposal

## Why

`crates/qrass` 已删除；`crates/sasspile` 是一套 clean-room 的 SCSS→CSS 编译器，**从 rxrust
出发**书写——程序本身即一条 owned-value 流，而不是把命令式代码套一层 Observable 的皮。在继续
生长各 stage 之前，需要一份权威的、行为优先的规格，定义 sasspile 必须产出什么，让每个 stage
都对着固定契约构建，并以 `sass-spec` hrx + element-plus 金样为闸（绝不参照 dart-sass 源码）。

## What Changes

- 把 sasspile 的完整可观测契约定义为一组能力规格，每条都是依据本地 `sass-spec/*.hrx` 金样与
  `element-plus` 语料得出的黑盒行为描述。
- 把架构不变量固化为需求（rxrust 为核心、`Local`/wasm、in-stream `Result`、单一 `subscribe`
  终端、无共享可变业务状态、无 std 数据流替身），供后续 change 继承。
- 确立一致性工装（经 `ledger` 的 hrx A/B、`regress=0` 闸门）作为每个能力的验收机制。
- 范围：**仅行为规格**——本 change 只写规格；实现在后续 change 里逐 stage 生长，每步 hrx 为闸。
  只支持 expanded 输出（无 `:compressed`）。

## Capabilities

### New Capabilities
- `scss-parsing`：词法+语法契约——ruleset、嵌套、选择器、声明、注释、插值，以及 at-rule 语法，
  产出向下流动的 owned AST。
- `scss-values`：值类型（带单位的 number、string、color、list、map、bool、null、argument-list）、
  运算符（算术/比较/逻辑/拼接/斜杠）、强制转换与相等。
- `scss-variables`：`$var` 声明、词法/全局/块作用域、`!default`/`!global`、命名空间 `ns.$var`。
- `scss-modules`：`@use`/`@forward`（命名空间、`as *`/`as 前缀`、`with` 配置、show/hide 可见性）、
  `@import`、文件/`--load-path` 解析。
- `scss-callables`：`@mixin`/`@include`/`@content`、`@function`/`@return`、实参绑定
  （位置/命名/可变参数/默认值）、经 `meta.*` 的一等函数/混入。
- `scss-control-flow`：`@if`/`@else if`/`@else`、`@each`（list/map/解构）、`@for`
  （from…through/to）、`@while`、`@at-root`。
- `scss-extend`：`@extend`、占位符 `%` 选择器、`!optional`、extend pass 的选择器归并语义。
- `scss-color`：CSS Color 4——各空间构造（rgb/hsl/hwb/lab/lch/oklab/oklch/color()）、转换、
  色域映射、通道访问、hex/命名色，及其序列化形态。
- `scss-builtins`：命名空间核心模块（`math`/`string`/`list`/`map`/`color`/`selector`/`meta`）
  加全局 legacy 别名，含每函数的 arity、参数名与从金样取出的精确错误文案。
- `css-output`：expanded CSS 序列化——规则/声明顺序、选择器展开、at-rule 分组、注释处理。
- `scss-diagnostics`：`SassError` 模型——源位置（`Located`）、与金样 `Error:` 行逐字匹配的消息格式、
  首错短路。
- `conformance-harness`：hrx A/B runner + `ledger` SQLite 存储（regression = 集合差）、
  element-plus 金样基线，以及门控每个 stage 的"仅金样"权威规则。

### Modified Capabilities
- （无——这是初始规格；`openspec/specs/` 已 purge 掉被删 qrass 的能力。）

## Impact

- 在 `openspec/changes/define-sasspile-spec/specs/` 下新增一组能力规格。
- 指导所有后续 `crates/sasspile` 实现 change；本 change 不改代码。
- 依赖既有 `crates/ledger` A/B 存储 + `sass-spec`/`element-plus` 子模块。
