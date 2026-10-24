# Tasks

## 1. 复用件补全（value/expr 独立件，先不接主链）

- [x] 1.1 扩 `Value`：List/Map/Color/带单位 Number/引号态 String + `to_css` 规范化，并补 `sass-spec/spec/values/**` 对应金样 `#[test]`，验证 `cargo test -p sasspile` 通过且未接入 `compile_string`
- [x] 1.2 扩 `Expr`：`Bin`/`Func`/`And`/`Or`/`If` + 惰性 `eval(owned Env)`，算术走单位规则、`/ * %` 跳过、比较只在条件上下文，补 `sass-spec/spec/operators/**`、`expressions/**` 金样 `#[test]`，验证独立件测试通过
- [ ] 1.3 内置函数白名单 `call_builtin` 分域（math/string/list/map/color/selector/meta）逐域以 `sass-spec/spec/core_functions/**` 金样钉死，未知名 `Err→原样`，验证 `#[test]` 覆盖各域且 `url/calc` 保留

## 2. `Shared` 流核心骨架（与旧核并行到 parity）

- [x] 2.1 引入 `tokio`（rt-multi-thread）依赖并加一个 `Shared`+`flat_map` 扇出/合并的最小可编译冒烟，验证 `cargo build -p sasspile` 通过
- [x] 2.2 `structure`：`Shared` token 流折成 owned `Block`/`Stmt` 数据（含 `order` 文档序键），补结构断言 `#[test]`，验证不产 `Vec<Frame>`/`chain_taken`（`sasslint` 对该模块 0 命中）
- [ ] 2.3 `stmt_stream(stmt, ctx) -> SharedBoxedObservable<Leaf>`：`flat_map` 下钻嵌套、`filter`+不可变链值选支、`concat_map`/`merge_all(1)` 保序、独立子图用 `merge_observables`，验证 `tests/compile.rs` 全绿（parity 旧核）
- [ ] 2.4 D3 定序终点：`collect` 后按 `order` 稳定排序拼接，`Err=Infallible`+流内 `Result`，唯一终点 `subscribe`/`into_future` 收口（无 `drain`/`Stage`），验证 expanded CSS 与旧核逐字节一致

## 3. 切换核心并移除帧栈式旧核

- [ ] 3.1 把 `compile_string` 切到新 `Shared` 流核心并删 `parse::Acc` 帧栈/可变标志，验证 `cargo test -p sasspile` 全绿 + `sasslint` 命中归 `0` + `ledger diff` 对旧基线 `regress=0`

## 4. 控制流簇：`@each` / `@for` / `@while`

- [ ] 4.1 以 `from_iter(items).flat_map(body_stream)` 实现 `@each`（列表/map），补 `sass-spec/spec/directives/each/**` 金样 `#[test]`，验证 `ledger diff regress=0`
- [ ] 4.2 `@for ... to/through` 计数循环 = `from_iter(range).flat_map(...)`，补 `directives/*for*` 金样 `#[test]`，验证 `regress=0`
- [ ] 4.3 `@while` 条件循环以流递归/`expand` 表达（非 `loop` 关键字），补 `directives/while/**` 金样 `#[test]`，验证 `regress=0` 且 `sasslint` 无 `no-imperative-loop`

## 5. `@mixin` / `@include` / `@function`

- [ ] 5.1 `@mixin`（形参默认值、`@content`、var-args）+ `@include` 展开为子流，补 `directives/mixin/**`、`values/mixins.hrx` 金样 `#[test]`，验证 `regress=0`
- [ ] 5.2 `@function`/`@return`/调用与用户函数求值，补 `callable/**`、`directives/function/**` 金样 `#[test]`，验证 `regress=0`

## 6. `@extend`

- [ ] 6.1 `@extend` 与 `%placeholder` 哨兵合并、选择器 specificity/去重，补 `directives/extend/**` 金样 `#[test]`，验证 expanded 选择器与金样一致且 `regress=0`

## 7. `@media` / `@supports`

- [ ] 7.1 媒体/`@supports` 冒泡与查询合并、内部选择器组合，补 `directives/media/**`、`directives/supports/**` 金样 `#[test]`，验证 `regress=0`

## 8. 模块系统 `@use` / `@forward`（多播）

- [ ] 8.1 `@use`/`@forward` + 命名空间访问，模块单例一次编译经 `Shared::subject`/`publish().ref_count()` 喂多引用方、跨线程共享冻结 `Arc<CompiledModule>`，补 `values/modules.hrx` 及带命名空间 `core_functions` 金样 `#[test]`，验证 `regress=0` 且无 `Arc<Mutex<业务态>>`

## 9. 错误形态

- [ ] 9.1 语法/未定义/非法运算/参数不匹配等错误以流内 `Result` 到终点收口，补 `sass-spec` 中标 `error` 的金样 `#[test]`，验证 `ledger` 归一化错误判定 `regress=0`

## 10. 集成与端到端校验（仅跨簇系统级）

- [ ] 10.1 全量 `sass-spec`（排除 `non_conformant`/`libsass-*`/`:todo:`）跑一遍并 `ledger` 立基线，验证通过率相对上一基线 `regress=0`
- [ ] 10.2 `element-plus` theme-chalk 入口端到端编译，验证无 panic 且与参照产物在 `ledger` 归一化下一致
- [ ] 10.3 `sasslint` 全 `crates/sasspile/src` 命中数 `0` + `cargo clippy -p sasspile --all-targets -- -D warnings` 绿，作为合入闸最终校验
