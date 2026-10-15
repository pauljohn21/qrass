# Proposal

## Why

SCSS→CSS 编译的本质是一串**数据形态转换**（字符流→token 流→owned AST→CSS 事件流→文本），每一跳都是 `(输入流, owned 状态, 迁移闭包, 输出流)` 的实例——这与 rxrust 算子（`scan_map`/`flat_map`/`reduce_initial`/`collect→pass→flat_map`/`subject` 多播）构成**同构**关系。qrass 的目标是以 rxrust 为唯一执行骨架、按**流形状 + Rust 所有权**直接推导每个阶段的算法与数据结构。

行为正确性由 **sass 语言规范**定义、由 **golden 对比与 sass-spec `.hrx` 用例**约束验证。dart-sass 仅作为 golden 离线生成器（oracle），不提供任何结构论据或性能模型。

验证负载收敛为两个：
- **element-plus theme-chalk**（企业级真实负载，全面使用现代模块制 `@use`，`@import` 为 0，1520 处 `@include`，BEM mixin 体系）
- **sass-spec**（官方行为规则 `.hrx` 用例，仅取现代语法子集）

## What Changes

- 新建 `crates/qrass` crate（lib + `qrass` CLI 二进制），依赖锁定 `rxrust = "=1.0.0-rc.5"`、`tracing`、`tracing-subscriber`，edition 2024 / toolchain 1.99
- **流原生编译管线**——每阶段由 rxrust 算子承载业务逻辑本体（非过程式内核包流壳）：
  - lexer：逐字符 `scan_map(Lexer, feed)` 状态迁移 → 0..N token → `flat_map` 摊平
  - parser：`scan_map(栈式累加器)` 折叠 owned AST 树移入流
  - eval：`scan_map(Env)` 按值携带定义；`@include` 展开 ≡ `flat_map` inner stream；`@content` ≡ 数据对物化
  - extend：`collect → 纯函数 pass → flat_map` 重发射（全局约束收齐后解算）
  - render：`reduce_initial` 折叠 + `finalize` 输出文本
  - 模块编排（L2）：`Shared::subject` 多播表达单例 + `Shared::flat_map` + `merge_observables` 并发独立分支
- 全面现代模块制：`@use`（含 `with` 配置、`as *`、命名空间）、`@forward`（含 show/hide/prefix）、私有 `$-` 成员；**不支持 legacy `@import` SCSS 导入**（CSS `@import` 透传除外）
- 内置函数双命名分发：命名空间式（`map.get`/`math.div` 等）与 legacy 全局名（`map-get`/`nth` 等）映射到同一实现；函数体为纯闭包 `(Vec<Value>) -> Result<Value, SassError>`
- 全链路 tracing span（stage/module/id/elapsed_ms），禁止 println!/eprintln!；测试全部位于 `crates/qrass/tests/`；单文件 ≤500 行
- 验收体系（行为正确性信号来源 = sass 规范 + golden）：
  - element-plus：`cargo test` 编译 `index.scss`，与 `tests/golden/element-plus/index.css` 严格对比（归一化空白后逐行一致）+ 组件选择器 100% 覆盖断言
  - sass-spec：Rust `.hrx` runner 驱动现代子集，allowlist 管理已知失败，汇总通过率以 tracing 输出

## Capabilities

### New Capabilities

- `scss-compilation`: 编译契约——CLI 与库 API 输入输出、错误呈现（流内 `Result` 协议）、阶段可观测性（tracing span 边界）
- `scss-parsing`: 词法/文法流迁移器——DFA `scan_map` token 流生长、`scan_map(栈)` owned AST 折叠、插值再词法 inner stream 复用
- `scss-modules`: 模块 DAG——canonicalize/load 纯函数、单例 ≡ `subject` 多播、`@use`/`@forward` 现代语义、循环检测在实例化流父路径栈
- `scss-evaluation`: 求值折叠——`scan_map(Env)` 按值作用域栈、mixin `flat_map` 展开、`@content` 数据对物化、控制流 ≡ 条件/迭代 inner stream、嵌套选择器合成
- `scss-builtins`: 函数注册表——命名空间 + legacy 别名双分发 → 纯闭包实现；行为由 sass 规范约束，golden 验证
- `css-output`: 序列化 reduce——`reduce_initial` 折叠 CSS 事件流为 expanded 文本；数字/颜色序列化规则由规范定义、golden 约束
- `conformance-harness`: 一致性验证——hrx runner（Rust 实现）、element-plus golden 回归、组件覆盖结构校验、通过率报告

### Modified Capabilities

（无——全新项目，`openspec/specs/` 目前为空）

## Impact

- 新增代码：`crates/qrass/`（src 模块化 ≤500 行/文件 + tests 全套）；根 `Cargo.toml` workspace member 已预先声明 `crates/qrass`，`Cargo.lock` 已含 rxrust 1.0.0-rc.5 依赖闭包
- 只读参照物（git submodule，不修改）：`sass-spec/spec/**`（行为用例）、`element-plus/packages/theme-chalk/src/**`（负载源）
- golden 基线：`crates/qrass/tests/golden/element-plus/`（静态数据，由官方构建链路离线生成一次并提交）
- bootstrap submodule 不再是验收目标
- 不引入新外部工具链（无 Python/Node）；验证全部通过 `cargo test` 与 `cargo run --bin qrass`
