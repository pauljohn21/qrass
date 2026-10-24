# scss-modules Specification

## Purpose
定义模块加载与模块图行为：文件解析规则与现代 `@use`/`@forward` 语义（含 `with` 配置）。模块单例≡`subject` 多播，独立分支≡并发。目标负载 element-plus theme-chalk 不含任何 SCSS `@import`，故 legacy `@import` 不在本能力范围（CSS `@import` 透传除外）。

## Requirements

### Requirement: 文件规范化管理
加载器 MUST 将 SCSS 引用路径解析为磁盘文件，按 sass 模块规范定义的优先级依次尝试：原样路径、`<path>.scss`、`<path>.css`、`_<path>.scss`、`_<path>.css`、目录内 `index`/`_index` 同名展开；引用 `.css` 文件时 MUST 原样透传为 CSS `@import`（不编译内容）。

#### Scenario: partial 前缀推断
- **WHEN** 某模块 `@use "config/vars"` 且磁盘存在 `config/_vars.scss`
- **THEN** 解析成功并加载该 partial

#### Scenario: CSS 透传
- **WHEN** 模块 `@use "legacy/reset.css"`
- **THEN** 输出 CSS 中出现 `@import "legacy/reset.css";`，其内容未被编译

### Requirement: @use 模块实例化与命名空间
`@use` MUST 构建独立模块作用域并以命名空间暴露公有成员：默认取路径末段（含 `-`→`_` 不变，去掉前导 `_`/扩展名），支持 `as <name>` 与 `as *`。私有成员（`$-x`、`@mixin -x`、`@function -x`）MUST NOT 跨命名空间可见；同一文件被多处 `@use` 时 MUST 只实例化一次（同一模块身份）。

#### Scenario: 命名空间访问
- **WHEN** `@use "sass:math";` 后调用 `math.div(1, 2)`
- **THEN** 求值成功；直接写裸名 `div(1, 2)` MUST 报未知函数

#### Scenario: 私有变量隔离
- **WHEN** 模块 A 定义 `$-secret: 1`，模块 B `@use "A" as *` 后引用 `$secret`
- **THEN** 报 undefined variable

### Requirement: @use with 配置
`@use "m" with ($v: 2)` MUST 在模块 m 实例化前注入配置值；m 中对应变量 MUST 以 `!default` 声明，否则 MUST 报"配置了非 @default 变量"错误；未被消费的配置项 MUST 报错并列出模块名与变量名。

#### Scenario: 入口配置链路
- **WHEN** `@use "vars" with ($primary: #409eff)` 且 vars 中 `$primary: #000 !default;`
- **THEN** 消费方读取 `$primary` 得到 `#409eff`，其余 `!default` 变量不受影响

### Requirement: @forward 再导出
`@forward "m"` MUST 使 m 的公有成员经转发模块的命名空间可见；MUST 支持 `show`/`hide` 过滤与 `as <prefix>*` 前缀改名；`@forward ... with` 配置 MUST 传递到被转发模块。

#### Scenario: forward show 过滤
- **WHEN** `@forward "vars" show $color, $font;` 后消费方引用 `$other`
- **THEN** 报 undefined variable，而 `$color` 可用

### Requirement: 模块图循环与缺失检测
`@use` 图 MUST 检测循环依赖并报错（列出环上文件）；无法解析的引用路径 MUST 报"文件不存在"错误并列出尝试过的候选路径。

#### Scenario: 循环引用
- **WHEN** a.scss `@use "b"` 且 b.scss `@use "a"`
- **THEN** 错误信息包含 a→b→a 环
