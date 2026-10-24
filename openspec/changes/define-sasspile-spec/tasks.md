# Tasks

## 0. 撰写本规格（规划——由本 change 完成）

- [x] 0.1 把 `openspec/config.yaml` 的 context 改为 sasspile rxrust 核心信条（去除 qrass 引用）+ 规定工件用中文
- [x] 0.2 proposal.md + design.md（架构不变量 D1–D7）
- [x] 0.3 12 个能力 spec：parsing / values / variables / modules / callables / control-flow /
      extend / color / builtins / css-output / diagnostics / conformance-harness
- [x] 0.4 `openspec validate define-sasspile-spec --strict` 通过

## 1. 嵌套 + 选择器（spec：scss-parsing、css-output）

- [x] 1.1 扩展 parse 折叠以持有嵌套规则；组合嵌套选择器（后代 + `&`）——owned `Frame` 栈建树，按分隔符缓冲区分伪类 `:` 与声明 `:`
- [x] 1.2 多级 expanded 输出顺序；补测试（descendant/`&`/深嵌套/父声明先于子，共 7 测）
- [x] 1.3 `SASSPILE_HRX_RUN` 快照 + `ledger diff sasspile-base sasspile-nest` → `regress=0 net=396`（pass 240→636）

## 2. 值与变量（spec：scss-values、scss-variables）

- [ ] 2.1 值类型（Number+单位、String、List、Map、Bool、Null）作为 owned 流值
- [ ] 2.2 运算符 + 单位代数 + 强制转换，错误走 in-stream Result
- [ ] 2.3 `$var` 作用域、`!default`/`!global`
- [ ] 2.4 每个子步都过 hrx A/B 闸

## 3. 模块（spec：scss-modules）

- [ ] 3.1 文件/`--load-path` 解析 + partial 候选
- [ ] 3.2 `@use`（namespace、`as *`、一次性实例化）、`@forward`（前缀、show/hide、with）
- [ ] 3.3 `@import` legacy 级联；纯 `.css` 直通校验
- [ ] 3.4 hrx A/B 闸

## 4. 可调用体与控制流（spec：scss-callables、scss-control-flow）

- [ ] 4.1 `@mixin`/`@include`/`@content` 实参绑定（命名/rest/默认值）
- [ ] 4.2 `@function`/`@return`；一等 `meta.get-*`/`meta.call`
- [ ] 4.3 `@if/@else`、`@each`、`@for`、`@while`、`@at-root`
- [ ] 4.4 hrx A/B 闸

## 5. 颜色、内置、extend（spec：scss-color、scss-builtins、scss-extend）

- [ ] 5.1 Color 4 各空间 + 转换/色域/通道，数值对齐金样
- [ ] 5.2 命名空间核心模块 + 全局别名；arity 与精确错误文案
- [ ] 5.3 `@extend` + 占位符 + 归并
- [ ] 5.4 hrx A/B 闸

## 6. 诊断与工装收尾（spec：scss-diagnostics、conformance-harness）

- [ ] 6.1 带 `Located` 的 SassError，错误文案匹配金样
- [ ] 6.2 工装：段前缀 key、错误用例比较、options.yml
- [ ] 6.3 接好 element-plus e2e 金样作为第二道闸
- [ ] 6.4 收尾：hrx 通过率爬坡报告；每个 spec 能力的用例全绿或立阻塞项

## 7. 全程遵守的不变量（spec：design.md）

- [ ] 7.1 每个 stage 都是流上的 rxrust 算子；状态=owned 累加子；单一 `subscribe` 边缘；无 std
      数据流替身；无 `drain_one`/封装框架
- [ ] 7.2 每步 `cargo clippy -D warnings` + `cargo test -p sasspile` 全绿；wasm32 目标校验
