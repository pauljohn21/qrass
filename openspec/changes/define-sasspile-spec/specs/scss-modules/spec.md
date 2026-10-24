# scss-modules Specification

## ADDED Requirements

### Requirement: @use、命名空间与配置
`@use "url" [as ns | as *] [with (…)]` MUST 加载并实例化一个模块一次，把其公开成员绑定到 `ns`
下（`as *` 时并入当前作用域）；`with ($p: v)` MUST 在模块求值前把配置传给它的 `!default` 变量。
对同一 canonical URL 的第二次 `@use` MUST 复用同一实例（模块按 canonical path 一次性实例化）。

#### Scenario: 命名空间函数调用
- **WHEN** `@use "math"; a { b: math.abs(-2); }`
- **THEN** `b: 2`

#### Scenario: with 配置喂给 !default
- **WHEN** `@use "m" with ($x: 5);` 且 `m` 里 `$x: 1 !default;` 被用于输出
- **THEN** 模块看到的 `$x` 为 `5`

### Requirement: @forward 与可见性
`@forward "url" [as prefix-*] [only/except show/hide …]` MUST 再导出成员，应用前缀与 show/hide
过滤；show/hide MUST 区分类型（`$name` 条目作用于变量、裸名作用于 mixin/function），且非空的
`only` 是全局 allow-list。

#### Scenario: 带前缀转发
- **WHEN** `@forward "m" as m-*;` 然后 `@use "fwd";` 并调 `fwd.m-get()`
- **THEN** 该成员在前缀名下解析成功

### Requirement: @import legacy 与解析
`@use`/`@forward`/`@import` 的 URL 解析 MUST 依次搜索 partial 候选（`_name.scss`、`name.scss`、
`name/index.scss`、`.css`），先相对导入文件、再按配置的 load paths；`@import` MUST 支持 legacy
级联（变量/mixin 提升到导入方作用域）。纯 `.css` 导入 MUST 原样直通，且 SCSS-only 构造被拒绝
（见 `css-output` / plain-CSS 校验）。

#### Scenario: partial 文件解析
- **WHEN** `@use "foo";` 且入口同目录存在 `_foo.scss`
- **THEN** 加载 `_foo.scss`
