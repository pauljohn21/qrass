# scss-variables Specification

## ADDED Requirements

### Requirement: 声明与查找
`$name: value` MUST 声明一个变量、求值为 owned 值并绑定到当前作用域；引用未声明的 `$name` MUST
报出金样的 "not declared" 错误。

#### Scenario: 读取已声明变量
- **WHEN** `$x: 3px; a { w: $x; }`
- **THEN** 输出为 `a {\n  w: 3px;\n}`

### Requirement: 块与词法作用域
在顶层赋值的变量 MUST 为全局；在块内赋值 MUST 局部于该块，除非加 `!global`。查找 MUST 沿词法向外
解析到全局作用域。

#### Scenario: 局部赋值不外泄
- **WHEN** `a { $x: 1; b { c: $x; } }`
- **THEN** 按金样作用域，`$x` 在 `a` 之外不可用

### Requirement: !default 与 !global 标志
`$x: v !default` MUST 仅在 `$x` 未定义时绑定；`$x: v !global` MUST 无视嵌套深度写入全局作用域。

#### Scenario: 已设值时 !default 被跳过
- **WHEN** `$x: 1; $x: 2 !default;`
- **THEN** `$x` 仍为 `1`

### Requirement: 命名空间成员访问
`ns.$var` MUST 读/写模块 `ns` 导出的变量（见 `scss-modules`），经模块的共享成员存储路由，使被
再导出的成员解析到其起源模块。

#### Scenario: 读取所用模块的变量
- **WHEN** `@use "vars"; a { b: vars.$c; }` 且 `_vars.scss` 定义 `$c: 1;`
- **THEN** `b: 1`
