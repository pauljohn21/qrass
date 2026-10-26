# qrass-dependencies.dot — 证据索引

- **产出方式**: `codegraph_explore`（server: codegraph，projectPath=工作区根）×2 查询 + `Cargo.toml`/`use crate::` 扫描兜底（codegraph 不索引配置文件，符合规则回退条件）。
- **视图问题（一图一问）**: crate 间与 sasspile 模块间的**编译期依赖**方向及内核流数据形态。静态 import，非运行时调用。

## 节点/边的证据

| 关系 | 类型 | 证据 | 置信度 |
| --- | --- | --- | --- |
| sasspile → rxrust `=1.0.0-rc.6` | compile-time | `crates/sasspile/Cargo.toml` [dependencies] | high |
| sasspile → ledger | **dev**-dependency | `crates/sasspile/Cargo.toml` [dev-dependencies]；使用方 `tests/hrx.rs` | high |
| ledger → rusqlite(0.32 bundled)/tracing/tracing-subscriber | compile-time | `crates/ledger/Cargo.toml` | high |
| sasslint → tracing/tracing-subscriber/serde/serde_json | compile-time | `crates/sasslint/Cargo.toml` | high |
| lib::compile → Lexer→Structurize→Exec→Render（transform×4，`Shared::from_iter` 源，`reduce_initial`+`into_future` 收口） | 订阅链 | codegraph 查询 1：`crates/sasspile/src/lib.rs:144-166` 原文 | high |
| phases → lexer（Tok/Event/LexAcc/MsgAcc）、phases → exec（State/StyleRule） | module import | codegraph 查询 1：`phases.rs` use 段 + `use crate::` 扫描 | high |
| exec → env/expr/lexer/selector/value；expr → calls/env/lexer/value；calls → builtins/lexer；builtins → value；env → value；selector → lexer | module import | `use crate::<mod>` 全量 grep（逐文件） | high |
| 数据形态 char→Tok→Event→StyleRule→String | 文档性边 | `phases.rs` 各内核 doc 注释与 `ObservableType::Item` | high |

## 过滤与已知粗化

- 未画 `crate::SassError/ErrorKind`（各模块 → lib.rs 根）这条公共边——10 个模块里 4 个引用它，画全会糊掉主线；它体现在 lib.rs 节点标签里。
- 测试文件（`tests/hrx.rs` 等 22 个 `compile` callers）不入图，仅以 sasspile→ledger 虚线代表 dev 边。
- `element-plus/`、`sass-spec/` 为外部参照目录，非 Rust 依赖，不入图。
- 本机未安装 graphviz，`dot -Tsvg` 渲染未执行；`.dot` 可直接用 Qoder 的 DOT 预览器打开，安装 graphviz 后可派生 SVG。
