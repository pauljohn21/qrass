# Spec Delta

## Purpose

定义内置函数库的可观察行为：命名空间模块函数、legacy 全局别名双分发，以及数值/颜色/集合函数的 sass 规范语义。函数体为纯闭包 `(Vec<Value>) -> Result<Value, SassError>`，无环境捕获。

## ADDED Requirements

### Requirement: 命名空间内置模块
编译器 MUST 提供 `@use "sass:math|color|map|string|list|selector|meta"` 命名空间下的函数集合，覆盖 element-plus theme-chalk 实际调用面：math（div/percentage/round/ceil/floor/min/max/abs/sqrt/log/pow/hypot 及常量 `$pi`/`$e`）、color（rgb/hsl/hwb/channel/adjust/scale/change/mix/complement/invert/grayscale/ie-hex-str 等）、map（get/set/merge/has-key/keys/values/remove）、string（unquote/quote/slice/index/length/insert/to-upper-case/to-lower-case/unique-id）、list（append/join/length/nth/index/separator/set-remove）、selector（nest/parse/extend/replace/unify/is-superselector/simple-selectors）、meta（type-of/keyword-list-exists/call/get-function/variable-exists/mixin-exists/global-variable-exists 等）。

#### Scenario: color.channel 双形态
- **WHEN** 调用 `color.channel(#0d6efd, "red")` 与 `color.channel($color: ..., $channel: ...)`
- **THEN** 两种传参形态返回同一数值 13

#### Scenario: math.div 除零
- **WHEN** 调用 `math.div(1, 0)`
- **THEN** 返回无穷大数值语义（sass 规范定义）而非报错

### Requirement: legacy 别名双分发
未 `@use` 命名空间时，legacy 全局函数名（`map-get`、`nth`、`lighten`、`percentage`、`div` 旧式 `/` 等）MUST 仍可在任何模块中直接调用并分发到与命名空间版相同的实现；命名空间版与 legacy 版行为差异 MUST 与 sass 规范的 legacy 兼容规则一致。

#### Scenario: 同名等价
- **WHEN** 分别用 `map.get($m, "k")` 与 `map-get($m, "k")` 查询同一 map
- **THEN** 结果一致

### Requirement: 数值序列化精度
内置函数返回的数字 MUST 符合 sass 规范定义的精度：浮点有效数字上限 10 位、整数除法产生分数值（`math.div(1,3)` 输出 `0.3333333333`）、颜色通道取整规则（`color.adjust` 后 hsl 通道四舍五入方式）由 spec 约束并经 golden 验证。

#### Scenario: 三分之一
- **WHEN** 输出 `math.div(1, 3)`
- **THEN** CSS 文本为 `0.3333333333`

### Requirement: 未知函数与 arity 错误
调用不存在的函数名 MUST 报 undefined function 并附文件行列；实参数量/命名不匹配 MUST 报参数错误并指出函数签名。

#### Scenario: 拼写错误的命名空间
- **WHEN** `@use "sass:map"` 后调用 `map.gt($m, "k")`
- **THEN** 报 undefined function `gt`（模块 map），且不 panic

### Requirement: map/list 字面量语义
`(a: 1, b: 2)` MUST 产出 map 值；`(1 2) (3 4)` MUST 产出空格分隔的嵌套 list；`length`/`nth`/`index` 对 map MUST 按 pair-list 视图工作，转换规则由 sass 规范定义。

#### Scenario: map 的 list 视图
- **WHEN** `nth(("a": 1, "b": 2), 1)`
- **THEN** 返回 pair list `("a", 1)`
