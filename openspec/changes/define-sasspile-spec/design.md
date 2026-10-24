# Design

## D1 — 程序就是那条 owned-value 流

每个编译 stage 是一条冷 `Local` observable 链；stage 间用 `flat_map` 衔接；整条编译是一个表达式，
公开的 `compile_*` 就是唯一的 `subscribe`+mpsc 边缘。管道形状：

```
from_iter(chars)/create(source)
  → scan_map(owned 缓冲累加子)            // 词法：每字符产出 0..N 个 token，token 即发射
  → flat_map |tok| parse-fold             // 逐 ruleset；嵌套 = 递归子流
  → reduce_initial(owned AST)             // 语法折叠成一个向下 move 的 owned program
  → flat_map |node| eval-stream           // 求值：env 是被 scan_map/reduce 携带的 owned 累加子
  → map/scan_map 逐发射                    // 渲染：规则 → CSS 字符串发射
  → reduce_initial/collect                // 把发射折叠成最终 owned String
  → subscribe(move |r| tx.send(r))         // 唯一终端；错误走 in-stream Result
```

**规则：** 状态是被流携带的 owned 累加子（`scan_map` 的 `&mut acc`，或 `reduce_initial` 被 move 的
acc）——绝不是 `&mut self` 游标、绝不是 `Arc<Mutex<_>>` 业务数据、绝不是替数据流站台的 std
`.iter().map/fold/collect()` 链。

## D2 — `'static` / box / GAT 是所有权指令，不是豁免

闭包必须 `'static`；元素是 GAT（`Item<'a>`）；跨函数签名传流用 rxrust 自带的 `box_it()` 擦除。
这靠**把 owned 值 move 进流**满足（流携带什么就自己拥有什么——例如喂 `from_iter` 一个 owned
`Vec`/`chars.collect`，而非借用的切片），而不是造封装。当某构造确实无法等价表达时，作为**阻塞项**
上报，绝不退回 std 链、绝不登记豁免。

## D3 — 无框架层；单一终端

**没有**自造的 `Stage<T>`/`fail()`/`drain()`/`drain_one()` 迷你框架，`drain` 式塌缩也**绝不**逐
叶层使用。只存在一个同步边界（管道的 `subscribe`+mpsc）；叶子函数返回普通值，仅限它们是真正在算子
闭包**内部**使用的纯 helper，而不是用来绕过流回头走命令式。

## D4 — 错误协议

编译错误作为 in-stream `Result<T, SassError>` 在值通道流动（`Err = Infallible`，故 `subscribe`
吞错误通道也无所谓）；失败一路短路到终端。`SassError` 携带源位置（`Located`）与消息，其文本必须
逐字匹配金样 `Error:` 行（见 `scss-diagnostics`）。

## D5 — 上下文：全链 `Local`、wasm 目标

整个 crate 单线程 `Local`（无 `Shared`/`Send`/`Sync`、无 tokio 运行时；非定时算子在 subscribe
期间内联跑完），故 sasspile 可编译到 `wasm32-unknown-unknown`。完成的数据靠 move 共享；基础规格里
不含共享可变的模块总线。

## D6 — 行为权威与验收

每条需求都从本地 `sass-spec/*.hrx` 金样（input→output/error）与 `element-plus` 语料推导。**dart-sass
源码/文档绝不是结构或算法参照**——只是产出金样的黑盒 oracle。任一已实现能力的验收为：新跑一次 hrx +
`ledger diff <base> <run>` 显示 `regress=0` 且该能力瞄准的用例翻 PASS；接好后的 element-plus e2e 同理。

## D7 — 一致性工装本身也被规格化

`crates/sasspile/tests/hrx.rs` 把每个 hrx 用例编译后把 PASS/FAIL 记入 SQLite `ledger`
（`reports/hrx.db`）；regress/net 是失败集合的差。工装契约（hrx 段解析、key 格式、比较归一、权威规则）
即 `conformance-harness` 能力。
