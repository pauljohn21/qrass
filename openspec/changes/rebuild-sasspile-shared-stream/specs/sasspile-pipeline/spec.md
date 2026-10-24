# sasspile-pipeline Specification

## ADDED Requirements

### Requirement: 结构化词法（一次成型）
`lexer` MUST 一次产出结构化 token：`Word`/`Punct{; : , { } }`/`Var`/`Interp(内层 token)`；`#{…}` 用累加子数据栈识别（含括号内）、`/* */` 剥离。后续 MUST 只消费 token，不再对拼回的字符串二次扫描。

#### Scenario: 插值不成块括号
- **WHEN** `a { color: #{$x}; }`
- **THEN** `#{$x}` 成为一个 `Interp`，其 `}` 不被当块闭合

### Requirement: 值/选择器折叠成惰性 Expr
值与选择器 token 段 MUST 一次折叠成 owned `Expr`（`Lit/Var/Interp/Concat/Bin/Func/And/Or/If`），`eval(Expr, &Env)` 惰性求 `Value`；`Value::to_css` 规范化（逗号 `, `、空白折叠、插值紧邻）。算术 MUST 走单位规则；函数 MUST 走**白名单** `call_builtin`，未知名→`Err`→原样兜底（`url/calc/min/max` 等 CSS 原生函数不当 Sass 内置求值）；含 `/ * %` 的值跳过算术（护 CSS 斜杠分隔符）。

#### Scenario: 单位相加
- **WHEN** 求值 `10px + 5px`
- **THEN** 得 `15px`

#### Scenario: 未定义插值丢声明
- **WHEN** 值含 `#{$undeclared}`
- **THEN** `eval` 返回 `Err`，该声明被丢弃

### Requirement: 块级结构折叠为 owned Stmt（数据，非遍历对象）
`parse` MUST 以一次栈折叠把 token 流产成 `Vec<Stmt>`（`Stmt{ head, decls, body }`，owned 数据），仅供流式核心按 `flat_map` 消费；MUST NOT 有"AST→递归下降遍历器/解释器"。

#### Scenario: 折叠产物是数据
- **WHEN** 解析 `a { b { c: d } }`
- **THEN** 得到嵌套 `Stmt`，由流核心用 `flat_map` 处理，而非被一个 `fn` 自递归遍历

### Requirement: 流式渲染核心
`compile` MUST 是：token 流 →（fold）`Vec<Stmt>` → `stmt_stream(stmts, prefix, env)` 用 `flat_map` 把每兄弟/子块展开成子 observable、`@each/@for` 用 `from_iter(items).flat_map` 扇出、`@if/@else` 用条件选子流或空流、独立子图用 `merge_observables`、模块单例用 `Shared::subject` 多播 → 叶子 `map` 成 CSS 块 → `reduce` 合并 → **一次 `subscribe`**。变量文档序 env 以"对兄弟做一次 fold 产出 (envSnapshot, stmt) 序列"传递，不用可变累加子驱动。

#### Scenario: 端到端
- **WHEN** `compile_string("a { b { color: red; } }")`
- **THEN** 得 `a b {\n  color: red;\n}\n`，且 `compile_string` 内恰一次 `subscribe`

#### Scenario: 控制流
- **WHEN** `@if false {x{y:1}} @else {z{w:2}}` 与 `@each $i in 1 2 { a#{$i}{p:$i} }`
- **THEN** 分别得 `z {…}` 与 `a1{…} a2{…}`，均由算子展开，无手写循环/分支状态机

### Requirement: 模块系统
`@use/@forward` MUST 以"编译一次 + 多播共享"实现，引用方经 `Shared::subject`/`Arc<不可变模块>` 读取；`@mixin/@include` 参数按 owned 值传入 body 子流；`@extend` 作为终点合并阶段的一次流上处理，不外部遍历全局 AST。

#### Scenario: 一次编译多引用
- **WHEN** 两个 `@use` 指向同一模块
- **THEN** 该模块仅编译一次并经多播供两消费者
