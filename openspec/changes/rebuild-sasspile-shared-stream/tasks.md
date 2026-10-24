# Tasks: Shared 流式核心重建

## 0. 前置与基线
- [ ] 0.1 `sasslint` 增 `mutable-accumulator-drives` 检测（`Acc` 含可变 `Vec<Frame>`/`chain`/跨条目驱动字段）；跑现核，确认它**报出**当前 `parse.rs::Acc` 帧栈（确立靶子）
- [ ] 0.2 `ledger` 记录旧核绿基线快照（作为重写期 `regress=0` 的 base）
- [ ] 0.3 确认 `Send/Sync/'static` 可行性：给 `Value`/`Tok`/`Stmt` 加 `Clone+Send+Sync`（`Expr` 亦然）；跑一个 `Shared::empty()`+`create` 的最小 `subscribe` 冒烟

## 1. 值层补全（复用 expr/value，扩展）
- [ ] 1.1 `Expr` 补 `Bin/Func/And/Or/If` + `eval` 惰性短路 + 单位算术（金样单测，不接主链）
- [ ] 1.2 值级 token：让算术/函数可解析（`from_tokens` 支持 num/op/paren/`name(...)`；`call_builtin` 白名单；`/ * %` 守门；金样单测）
- [ ] 1.3 `expr` 全部 parity 旧 `value_to_css`/arith 行为后，`value`/`expr` 定稿

## 2. 流式核心（替换帧栈 fold）—— 每步 regress=0，绿才提
- [ ] 2.1 `stmt_stream(stmts, prefix, env) -> SharedBoxedObservable<Leaf>`：兄弟用一次 fold 产 `(envSnapshot, stmt)` 序列 → `from_iter(...).flat_map(stream_stmt)`；普通规则 `map` 成 Leaf
- [ ] 2.2 嵌套：子块 `flat_map`，prefix/env 作 owned 入参下传（无栈）
- [ ] 2.3 `@if/@else if/@else`：条件选子流 / `empty()`；链命中用随流 move 的不可变累积值（去 `chain_taken` 可变字段）
- [ ] 2.4 `@each/@for`：`from_iter(items).flat_map(|it| body_stream(it, prefix, env+it))`（无 body 缓存重放）
- [ ] 2.5 `@media/@supports`：Leaf 携带 wrap 上下文，终点前一次 `map` 包裹（数据，非树）
- [ ] 2.6 独立子图并发 `merge_observables`（默认顺序，仅真并发处用）；确认输出文档序稳定
- [ ] 2.7 `compile` 收口：token→(fold)`Stmt`→`stmt_stream`→`map(leaf_block)`→`reduce` 合并→**一次 `subscribe`**；`compile_string` 切过去，删旧 `parse::Acc` 帧栈核心

## 3. 模块与共享
- [ ] 3.1 `@use/@forward`：编译一次 + `Shared::subject`/`Arc<不可变模块>` 多播共享
- [ ] 3.2 `@mixin/@include`：参数按 owned 值传入 body 子流
- [ ] 3.3 `@extend`：终点合并阶段一次流上处理，不外部遍历

## 4. 收敛
- [ ] 4.1 hrx 全量 `ledger diff regress=0`，按目录簇逐片转正（values/operators/directives/core_functions/scss/…）
- [ ] 4.2 `sasslint` 对 `sasspile/src` 0 命中 → 取消 `gate_sasspile_is_clean` `#[ignore]`
- [ ] 4.3 OpenSpec 归档本变更、同步 `openspec/specs/`
