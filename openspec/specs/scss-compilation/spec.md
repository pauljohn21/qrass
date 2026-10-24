# scss-compilation Specification

## Purpose
定义 qrass 编译器对外的编译契约：CLI 与库 API 的输入输出、错误呈现（流内 `Result` 协议，各阶段失败即融合停发）、编译过程的可观测行为（tracing span 阶段边界）。

## Requirements

### Requirement: CLI 编译入口
`qrass` 可执行文件接受一个 SCSS 入口文件路径作为输入，MUST 支持 `-o <file>` 指定输出文件；未指定 `-o` 时 MUST 将 CSS 写到标准输出。成功的进程退出码 MUST 为 0。

#### Scenario: 编译单个文件到标准输出
- **WHEN** 运行 `qrass <style.scss>`
- **THEN** 标准输出为展开（expanded）风格的完整 CSS，退出码 0

#### Scenario: 编译输出到文件
- **WHEN** 运行 `qrass <element-plus/packages/theme-chalk/src/index.scss> -o /tmp/out.css`
- **THEN** `/tmp/out.css` 存在且内容为编译产物，标准输出无 CSS 内容

### Requirement: 库 API 编译入口
qrass MUST 提供库函数形式的编译接口：给定入口文件路径与选项，返回拥有所有权的 CSS 字符串或类型化错误，供测试与外部 crate 直接调用。

#### Scenario: 通过库 API 编译字符串内容
- **WHEN** 测试代码以临时目录中的 `_entry.scss` 调用编译接口
- **THEN** 返回 `Ok(String)` 且包含预期 CSS；输入文件句柄在返回后已全部释放

### Requirement: 编译错误报告
任何阶段（加载、词法、文法、求值、内置函数）的错误 MUST 收敛为单一错误类型，携带出错文件路径、行列号、出错语句或表达式的源码片段，以及人类可读的成因说明；CLI MUST 以非 0 退出码结束。

#### Scenario: 未定义变量引用
- **WHEN** 编译 `a { color: $undefined-var; }` 且该变量无任何定义
- **THEN** 报错信息包含文件、行、列与 "undefined variable" 语义，退出码非 0

#### Scenario: 语法错误定位
- **WHEN** 编译 `a { color: ; }`
- **THEN** 报错定位到缺失表达式的行列位置而非文件末尾

### Requirement: 输出风格范围
首版 MUST 仅支持 expanded 输出风格；请求 nested/compact/compressed 时 MUST 明确报"不支持"错误而不是静默降级。

#### Scenario: 请求 compressed 风格
- **WHEN** 以 `--style compressed` 调用 CLI
- **THEN** 得到明确的 unsupported style 错误，退出码非 0

### Requirement: 编译阶段可观测性
编译器运行时 MUST 通过 tracing 输出各阶段（load/lex/parse/eval/render）的 span 与耗时字段（stage、module、elapsed_ms）；标准输出 MUST 只包含 CSS 产物，MUST NOT 出现任何日志文本。

#### Scenario: 以 RUST_LOG=info 运行 CLI
- **WHEN** 环境变量 `RUST_LOG=info` 下编译一个 SCSS 文件
- **THEN** stderr/日志层出现各 stage 的 span 记录，stdout 仍为纯 CSS
