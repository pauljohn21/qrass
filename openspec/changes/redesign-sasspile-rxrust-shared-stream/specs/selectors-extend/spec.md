# Spec Delta — selectors-extend(选择器模型、selector.*、@extend、@at-root)

## Purpose

定义 SCSS **选择器域**的对外行为:结构化 selector 模型与函数、`@extend`(placeholder/类、链式、伪类
并集、超选择器)、`@at-root`。逐条可回溯 `sass-spec/spec/{css/selector,directives/extend,
directives/at_root,core_functions/selector,libsass/selectors}/**` 与 `element-plus`(BEM `&`)。

## ADDED Requirements

### Requirement: 结构化 Selector 模型与函数

`sasspile` MUST 以结构化 selector(compound / combinator `> + ~` / 伪类 `:is/:not/:where/:hover…` /
`&`)表示并序列化选择器;MUST 支持 `selector.parse/nest/unify/append/replace/is-superselector/
simple-selectors/extend`。组合子 `a > b` 在选择器上下文 MUST NOT 被当比较求值。选择器序列化空白/顺序
MUST 与金样一致。

参照:`css/selector/**`、`core_functions/selector/**`、`libsass/selectors/**`。

#### Scenario: selector.nest
- **WHEN** `a { b: selector.nest("c, d e", "f") }`
- **THEN** `a { b: c f, d e f; }`

#### Scenario: 组合子不当比较
- **WHEN** `a > b { c: d }`
- **THEN** 输出选择器 `a > b`(不求值 `>`)

### Requirement: `@extend` 选择器合并(不动点)

`@extend <selector>` MUST 把目标(complex selector)的声明并入所有匹配该目标的规则选择器,按 Sass
expanded 语义合并(去重、specificity/组合子有序)。MUST 支持:多目标 `@extend a, b`、**链式/传递**
(`d→m→u`)、**选择器伪类内并集**(`:is(...)` 扩展)、**超选择器裁剪**。`%placeholder` MUST NOT 单独输出。
`@extend … !optional` 目标缺失 MUST NOT 报错;非 optional MUST 报错。`:todo: dart-sass` 用例不作判据。

参照:`directives/extend/**`(含 `after_target`、`pseudo`、`trims_super_selector_without_combinator`)。

#### Scenario: placeholder 扩展
- **WHEN** `%c { color: red } a { @extend %c }`
- **THEN** `a { color: red }`,无 `%c`

#### Scenario: 选择器伪类并集(链式)
- **WHEN** `:is(midstream){@extend upstream}` `downstream{@extend midstream}` `upstream{a:b}`
- **THEN** `upstream, :is(midstream), :is(midstream, downstream) { a: b; }`

#### Scenario: 多目标链式合并
- **WHEN** `.a .b{c:d} .a.mod1,.a.mod2{@extend .a,.b}`(mod3..6 同)
- **THEN** 按金样顺序把各 extender 并入 `.a .b` 选择器组

### Requirement: `@extend` 边界与诊断

bogus combinator 作 extender(如 `> d`、`d +`)MUST 仅报 `bogus-combinators` **deprecation 警告**、
仍产出扩展结果;非法 `@extend` 目标 MUST 按 error 金样。`@extend` 与 `@media`/伪元素交互 MUST 与金样一致。

参照:`directives/extend/bogus.hrx`、`extend/error.hrx`、`extend/whitespace.hrx`。

#### Scenario: bogus combinator 只警告
- **WHEN** `a {b:c} > d { @extend a }`
- **THEN** 输出 `a, > d { b: c; }` 且带 `[bogus-combinators]` 警告

### Requirement: `@at-root`

`@at-root` MUST 支持裸形式与 `(without: rule|media)`/`(with: …)` 查询:`without: media` MUST 让规则
脱离媒体上下文但保留(组合后)选择器;`@at-root <selector>` MUST 把后续块提升到根。媒体内 `@at-root`
与选择器插值组合 MUST 与金样一致。

参照:`directives/at_root/**`(property_only、keyframes、nested_import、whitespace)。

#### Scenario: 脱媒体保留选择器
- **WHEN** `@media print { a { @at-root (without: media) { b: c } } }`
- **THEN** 顶层 `a { b: c; }`
