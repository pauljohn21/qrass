# Spec Delta

## Purpose

定义一致性验证体系：以 **sass 语言规范**为行为定义唯一来源（通过 `.hrx` 用例具体化），以 element-plus golden 基线为端到端回归约束，全部通过 `cargo test` 驱动。dart-sass 仅作为 golden 生成器（oracle），不是测试依赖。

## ADDED Requirements

### Requirement: hrx 用例运行器
测试设施 MUST 用 Rust 实现 `.hrx` 解析与运行：识别 `input.scss`/`options.scss`/`output.css` 段及 `error:`/`warning:` 预期，遍历 `sass-spec/spec/{core_functions,expressions,directives,callable,css}` 子集中不依赖 legacy 引擎特性的用例执行；失败清单 MUST 由可版本化的 allowlist 文件管理；汇总通过率 MUST 以 tracing 日志输出而不是丢弃。

#### Scenario: 通过单条 spec
- **WHEN** 运行包含 `a { b: c }`→`a { b: c; }` 的 hrx 用例目录
- **THEN** 该用例计为 pass 且不出现在失败清单

#### Scenario: 预期错误用例
- **WHEN** hrx 声明本用例必须以特定错误信息失败
- **THEN** qrass 返回匹配错误（子串/正则语义）时计为 pass

### Requirement: element-plus golden 基线严格对比
测试 MUST 编译 `element-plus/packages/theme-chalk/src/index.scss` 并与固化在 `crates/qrass/tests/golden/element-plus/index.css` 的 golden 基线对比：首次固化前 MUST 通过组件覆盖与 CSS 结构校验（见下条）；此后每次回归 MUST 在归一化空白差异后与 golden 逐行完全一致；不一致时失败信息 MUST 报告首个偏差行号及双方上下文。更新 golden MUST 是显式动作（重新生成命令），不允许测试自动改写基线。

#### Scenario: 回归对齐
- **WHEN** 编译器输出与 golden 归一化后逐行相同
- **THEN** element-plus e2e 测试通过

#### Scenario: 数值偏差定位
- **WHEN** 某处输出与 golden 在同位置出现不同数值文本
- **THEN** 失败信息给出首个偏差行与两侧文本

### Requirement: element-plus 组件覆盖与结构校验
element-plus e2e MUST 断言：编译成功且输出 CSS 语法合法（括号配对、声明以分号结束）；每个组件源文件（theme-chalk/src/*.scss，如 button、table、dialog）对应的顶层选择器（`.el-button` 等）全部出现，覆盖率 100% 组件文件。

#### Scenario: 组件缺失检测
- **WHEN** 输出 CSS 未包含某组件源文件应产生的任一 `.el-<name>` 选择器
- **THEN** 覆盖断言失败并列出缺失组件

### Requirement: 回归门禁仅依赖 Rust 工具链
一致性验证 MUST 仅依赖 `cargo test`（Rust 工具链），MUST NOT 引入 Python/Node 脚本、dart-sass 二进制或网络访问。

#### Scenario: 离线全量测试
- **WHEN** 在无网络环境下运行 `cargo test -p qrass`
- **THEN** 全部 harness 测试可执行（基线文件均来自仓库内只读路径）
