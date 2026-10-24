# scss-parsing Specification

## ADDED Requirements

### Requirement: Ruleset 与声明解析
解析器 MUST 把一个 ruleset 接受为"选择器 + `{ … }` 块内的 `prop: value;` 声明"，并 MUST 产出
向下作为流值 move 的 owned AST 节点（无借用游标）。声明值跑到第一个 `;` 或闭合的 `}` 为止（最后
一条声明 MAY 省略分号）。

#### Scenario: 单个 ruleset
- **WHEN** 输入为 `a { b: c; }`
- **THEN** AST 为一条选择器 `a`、声明 `b: c` 的规则

#### Scenario: 末尾声明无分号
- **WHEN** 输入为 `a { b: c }`
- **THEN** 该规则解析出声明 `b: c`（不报错）

### Requirement: 嵌套与选择器组合
解析器 MUST 接受任意深度下规则与声明交错，并在渲染时 MUST 把嵌套规则的选择器与其祖先组合：后代用
空格、`&` 被父选择器替换、嵌套的 `&` 前/后缀形式无空格拼接。

#### Scenario: 后代嵌套
- **WHEN** 输入为 `a { b { c: d; } }`
- **THEN** 发射的规则选择器为 `a b`，含 `c: d`

#### Scenario: 父引用
- **WHEN** 输入为 `.a { &:hover { color: red; } }`
- **THEN** 发射的规则选择器为 `.a:hover`

### Requirement: 选择器、注释与插值的 token 形态
词法/语法 MUST 对 class/id/`%占位符`/`&`/伪类（`:`/`::`）、属性 `[ … ]` 与组合符选择器分词；MUST
在语法树里忽略 `//` 行注释与 `/* … */` loud 注释；并 MUST 把 `#{ … }` 插值记录为 AST 表达式，
而非在解析期摊平。

#### Scenario: 带引号值的属性选择器
- **WHEN** 输入为 `[class="--foo"] { x: y; }`
- **THEN** 该选择器分词为属性选择器并逐字保留

#### Scenario: 行注释被忽略
- **WHEN** 一条 `// note` 行出现在声明前
- **THEN** 该注释不产生 AST 节点，也不影响声明
