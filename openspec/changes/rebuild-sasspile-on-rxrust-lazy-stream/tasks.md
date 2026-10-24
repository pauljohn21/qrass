# Tasks: 重建 sasspile 为 rxrust 惰性流

## 1. 基线与闸口
- [x] 1.1 回退手搓 `Engine`，保留绿色基线
- [ ] 1.2 `sasslint` 规则齐（含 `no-from-fn-driver`/`no-work-queue`）+ `regress=0` 与 CI 闸脚本化
- [ ] 1.3 记录当前 `ledger` 绿基线快照作为 A/B 基准

## 2. 词法：结构化 token（无二次字符串扫描）
- [ ] 2.1 `lexer` 产 `Word/Punct/Var/Interp`，`#{}` 数据栈嵌套，块注释 `/* */` 剥离
- [ ] 2.2 隔离单测（interp 紧邻、`#{}` 内含函数/括号、注释）
- [ ] 2.3 主链接入 + `ledger diff regress=0`

## 3. 值：惰性 `Expr` / `Value`
- [ ] 3.1 `Expr` 折叠自 token：`Var`/`Interp`/`Concat`/`Bin`/`Func`/`And`/`Or`/`If`（`from_iter().fold`，无 while）
- [ ] 3.2 `Value` 强类型 + 单位代数 `arith` + `to_css` 规范化；`eval(Expr, owned Env)` 惰性短路
- [ ] 3.3 `call_builtin` 白名单 + 未知名→原样兜底 + `/ * %` 跳过守门；金样单测
- [ ] 3.4 比较/布尔 `Expr`（`== != < > <= >=`→`Value::Bool`），单独一步、护选择器 combinator
- [ ] 3.5 接线：值以惰性 `Expr` 随流下传，终点 `subscribe` 才求值输出；A/B regress=0

## 4. 结构与输出：无树的惰性流
- [ ] 4.1 parse 作为 `scan_map/reduce_initial` 的 owned 累加子：选择器即时组合进值、块以数据表达、**不物化 `Vec<Rule>`/`children`**
- [ ] 4.2 `@if/@else` = 依条件 `filter`/选择子流；块作用域 env 以 move 副本、循环变量作用域结束还原
- [ ] 4.3 `@each/@for` = `from_iter(items).flat_map(|it| body 子流(it))`（**不攒 body、不手搓重放**）
- [ ] 4.4 `@media/@supports` 包裹以数据表达、非递归；`render` = 对合并 leaf 流的 `map` + 终点一次 `subscribe` 拼接
- [ ] 4.5 全量 compile 单测 + `ledger diff regress=0`（目标净≥当前 579）

## 5. 模块与共享（多播）
- [ ] 5.1 `@use`/`@forward` 模块单例：一次编译结果用 `behavior_subject`/`share` 多播喂多引用方
- [ ] 5.2 `@mixin`/`@include`：参数按 owned 值传入 body 子流
- [ ] 5.3 `@extend`：合并阶段对 leaf 流的一次后处理（不外部遍历全局 AST）
- [ ] 5.4 每步 A/B regress=0

## 6. 收敛与收口
- [ ] 6.1 按 hrx 目录簇逐片转正（values/`operators`/`directives`/`core_functions`/`scss`/`extend`/`functions`）
- [ ] 6.2 `sasslint` 对 `crates/sasspile/src` 0 命中 → 取消 `gate_sasspile_is_clean` 的 `#[ignore]`
- [ ] 6.3 OpenSpec 归档本变更 + `openspec/specs/` 主 spec 同步
