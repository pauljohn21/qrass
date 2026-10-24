# sasspile-core Specification

## ADDED Requirements

### Requirement: 结构化词法（一次性、无二次扫描）
词法器 MUST 一次把源码切成结构化 token 流：`$name`→`Var`、`#{ … }`→`Interp(内层 token)`（括号内也识别）、`; : , { }`→`Punct`、其余原样→`Word`。后续 MUST 直接消费 token，MUST NOT 把 token 拼回字符串再用 char 状态机重扫（无 `stmt_text`/`subst`）。`#{}` 嵌套是累加子里的数据栈，非递归。

#### Scenario: 插值不被当块括号
- **WHEN** 源码含 `a { color: #{$x}; }`
- **THEN** `#{ … }` 被词法识别为一个 `Interp`，其 `}` 不被当作块闭合，块结构不受破坏

### Requirement: 值为惰性 Expr / 强类型 Value
声明值与选择器 MUST 折叠成 `Expr`（`Lit(Value)`/`Var`/`Interp`/`Concat`/`And`/`Or`/`If`/`Bin`/`Func`），`eval(Expr, owned Env)` 惰性求为 `Value`；`Value::to_css` MUST 规范化（逗号 `, `、空白折叠、插值紧邻拼接）。值类型 MUST 建模 Number(带单位)/String(引号态)/Color/List/Map/Bool/Null。`and`/`or`/`if` MUST 惰性短路。

#### Scenario: 惰性 if 只求所选支
- **WHEN** 求值 `if(false, <会报错的表达式>, fallback)`
- **THEN** 只求值 `fallback`，不触发未选中支的错误

#### Scenario: 未定义插值丢声明
- **WHEN** 某声明值含 `#{$undeclared}`
- **THEN** 求值返回 `Err`，该声明从输出丢弃（不 panic）

### Requirement: 变量绑定沿流的 owned env
`$x: v` 绑定 MUST 由流上携带的 owned `Env` 累加子记录、不输出；后文引用解析；文档序可见性，块作用域以 move 进 `flat_map`/子流的 env 副本实现（循环/块变量在作用域结束还原）。env 键约定 MUST 统一（一律去 `$`）。

#### Scenario: 循环变量作用域
- **WHEN** `@each $i in 1 2 { a#{$i}{} }` 展开后
- **THEN** `$i` 在循环外不残留，块级可见性正确

### Requirement: 控制流以算子表达
`@if`/`@else if`/`@else` MUST 以 `filter`/选择（依条件保留一支子流）；`@each`/`@for` MUST 以 `from_iter(items).flat_map(...)`；条件真值 MUST 遵循 Sass（仅 `false`/`null` 为假），比较/布尔运算 MUST 建模为 `Expr` 求值、MUST NOT 在值路径急切字符串判断，且 MUST NOT 把选择器 `a > b` 当比较。at-rule 块（`@media` 等）以数据表达，非递归遍历。

#### Scenario: 选择器组合子不当比较
- **WHEN** 处理选择器 `a > b`
- **THEN** 保持为后代/子组合选择器原样，绝不被求值为布尔

### Requirement: CSS 输出在终点合并生成
渲染 MUST 是把合并后的 leaf 流经 `map` 成块、终点一次 `subscribe` 汇成文本；MUST NOT 递归遍历规则树写 `String`。展开式（expanded）输出、文档序、祖先选择器已组合。

#### Scenario: 一次订阅成文
- **WHEN** `compile_string("a{b{c:d}}")` 触发终点订阅
- **THEN** 合并求值得 `a b {\n  c: d;\n}`；`subscribe` 恰一次

### Requirement: 模块系统用多播共享一次编译
`@use`/`@forward` 的一个模块被多个引用方消费时 MUST 用多播（一次实例化 fan-out），`@mixin`/`@include` 参数按 owned 值传入子流；`@extend` 作为合并阶段对 leaf 流的一次处理，不物化全局 AST 集合去外部遍历。

#### Scenario: 模块单例一次编译多次消费
- **WHEN** 两个 `@use` 引用同一模块
- **THEN** 该模块编译一次，结果经多播喂两个消费者

### Requirement: 内置函数白名单求值
`call_builtin` MUST 只对明确 Sass 内置白名单求值（含算术/字符串/`if` 惰性/`math.*` 等）；未知名 MUST 返回 `Err`、由上层原样兜底，使 `url/calc/min/max` 等 CSS 原生函数永不被当 Sass 内置求值。含 `/ * %` 的值 MUST 跳过算术（护 CSS 斜杠分隔符）。

#### Scenario: CSS 函数原样透传
- **WHEN** 值含 `min(1px, 2px)` 且不在白名单求值范围
- **THEN** 原样保留，不当作 Sass `min` 求值
