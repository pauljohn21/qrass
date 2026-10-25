# Spec Delta — modules-core-functions(@use/@forward/@import、加载器、内置函数、meta)

## Purpose

定义 SCSS **模块系统 + 内置函数库**的对外行为:`@use`/`@forward`/`@import` 与加载器、CSS `@import` 排序、
core_functions(math/string/list/map/color/selector/global)与 `meta`(含 `load`/`load-css`)。逐条可回溯
`sass-spec/spec/directives/{use,forward,import}/**`、`core_functions/**` 与 `element-plus`
(BEM 内部 `@forward` + 各组件 `@use … as *`)。

## ADDED Requirements

### Requirement: `@use` 加载与命名空间

`@use "path" [as ns | as * | as <nothing>] [with ($v: …) | ()]` MUST 加载模块、以其命名空间访问成员
(`ns.$var`/`ns.fn()`/`@include ns.mx`),`as *` 合并到当前作用域,`as null`/`hide` 仅执行不导出。
`with` MUST 配置上游 `!default` 变量且**已配置值令 `!default` 失效**;`@use` MUST 前置(在任何规则前)。
同一路径模块 MUST 只编译一次并喂多个 `@use` 方(多播)。

参照:`directives/use/**`(member/with/css)、`core_functions/meta/load*`。

#### Scenario: 命名空间成员
- **WHEN** `_a.scss`: `$x: 1`;主:`@use "a"; b { w: a.$x }`
- **THEN** `b { w: 1; }`

#### Scenario: with 配置 !default
- **WHEN** `@use "o" with ($a: configured)`,`_o.scss`: `$a: original !default; b{c:$a}`
- **THEN** `b { c: configured; }`

#### Scenario: 模块单例
- **WHEN** 两文件都 `@use "shared"`(有副作用)
- **THEN** `shared` 只编译一次(副作用一次),输出按引用序

### Requirement: `@forward` 透传

`@forward "path" [as prefix-] [show a, b | hide c] [with ($v:…)]` MUST 转发成员的可见性/重命名;经转发
访问的命名空间、`show/hide` 过滤、`with` 配置回传上游 MUST 与金样一致。`element-plus` 的
`mixins/mixins.scss` 用 `@forward 'config'/'function'/'_var'` 汇总,`@include b()` 解析 MUST 依赖此。

参照:`directives/forward/**`(member/with/error)、`element-plus` `mixins/mixins.scss`。

#### Scenario: show 过滤
- **WHEN** `_f` 有 `$a $b`;`@use "f" as fac;` 其中 `_fac.scss`: `@forward "f" show $a;`
- **THEN** `fac.$a` 可访问,`fac.$b` 不可见(按 error 金样)

### Requirement: `@import` 与 CSS import 排序

legacy `@import "partial"` MUST 内联其内容(带 `[import]` deprecation);原生 CSS `@import url(...)`/
`@import "x.css"` MUST **原样透传并上提到输出顶部、排在样式规则前**(即使源里在后面);`@import` 与
`@use`/`@forward` 优先级、二次导入去重按金样。加载器 MUST 解析 partial `_x.scss`/`x.scss`/目录 index/
显式扩展名与相对路径。

参照:`directives/import/**`(css/configuration/error)、`libsass/css-import.hrx`。

#### Scenario: CSS import 上提
- **WHEN** `@import "rule";` 后 `@import "import";`(内含 `@import url(http://x)` 与样式)
- **THEN** `@import url(http://x)` 出现在其样式规则之前(参 `import/css.hrx`)

### Requirement: core_functions 命名空间与全局双绑定

内置函数 MUST 提供命名空间形态(`@use "sass:math"` → `math.*`;同用于 color/string/list/map/
selector/meta)与 legacy **全局裸名**(`red()`/`nth()`/`map-get()`…,带 `[global-builtin]` deprecation)。
未知名 MUST `Err→原样`;CSS 原生 `url/calc/min/max/rgb()` 等 MUST NOT 被当 Sass 内置求值。各域行为按
`core_functions/**` 金样。

参照:`core_functions/{math,string,list,map,color,selector,meta,global}/**`、`core_functions/modules/**`。

#### Scenario: 命名空间 math
- **WHEN** `@use "sass:math"; a { w: math.sqrt(9) }`
- **THEN** `a { w: 3; }`

#### Scenario: 原生 min 不求值
- **WHEN** `a { w: min(1px, 2px) }`(CSS 原生上下文)
- **THEN** 依金样(不强制当 `math.min` 折叠)

### Requirement: `meta.load` / `load-css` / `get-module`(一等模块)

`meta.load("path")`/`meta.get-module("name")` MUST 产出**模块一等值**;`@include meta.css(<module|string>)`/
`meta.load-css(...)` MUST 把该模块 CSS 在**当前选择器上下文**重放(嵌套处组合父选择器)。`meta.module-
variables/functions/mixins`、`feature-exists` MUST 按金样。

参照:`core_functions/meta/{css,load,load_css,get_module}.hrx`。

#### Scenario: load-css 套当前选择器
- **WHEN** `a { @include meta.load-css("other") }`,`_other.scss`: `b { c: d }`
- **THEN** `a b { c: d; }`(参 `meta/css.hrx` nested)
