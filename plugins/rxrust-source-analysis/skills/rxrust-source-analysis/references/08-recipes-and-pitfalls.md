# 08 · 落地配方与常见坑（可复用清单）

本篇是"任何 rxrust 项目直接照抄"的部分：把反复出问题的四个点（**循环 / 嵌套 / 跨元素状态 /
一份产出喂多下游**）逐一对应到 rxrust 原生算子，并列出踩过的坑与判据。
配 `crates/sasslint` 的机器规则一起看（它已把反模式编码成硬闸）。

## 1. 编译器 / 多阶段管线配方（直接组合算子 + 终点收口）

跨阶段串联用 `flat_map`（**没有 `and_then`**）；错误以 in-stream `Result` + `Err = Infallible` 走。
**不要**为此造 `type Stage<T>` / `fn fail<T>` / `fn drain` 之类框架层——那是把 rxrust 压回"你自
己的控制流抽象"（`rust.md` 禁止；见 §4）。算子直接链在源头上；仅当一条链要跨函数边界返回时，
用 `box_it()` 擦除类型（这是原语用法，不是框架层）。

**收口看执行模型（`01 §5`）：**

纯同步 `Local` 链（非时间算子全程内联）→ 终点**唯一一次** `subscribe` + `mpsc` 取出，无需 runtime：

```rust
use rxrust::prelude::*;
use std::sync::mpsc;

// 源头 → 各阶段都是算子；错误当数据；一次 subscribe 收口。不物化"任务表"、不先建 AST 再遍历。
fn compile(source: String) -> String {
  let (tx, rx) = mpsc::channel::<String>();
  token_stream(source)
    .map(|toks| parse(&toks))                     // owned 值直接进下一算子
    .flat_map(|ast| eval(ast))                    // 循环 / 一对多 = flat_map
    .map(to_css)
    .reduce_initial(String::new(), |mut acc, s| { acc.push_str(&s); acc })
    .subscribe(move |css| { tx.send(css).ok(); }); // 内联同步跑完
  rx.recv().unwrap()
}
```

`Shared` + `from_stream` 源（`from_stream` 走 scheduler，`subscribe` 返回可 await 句柄）→ 终点用
rxrust 自带 **`into_future().await`**（由调用方 runtime 驱动），**不要**再套 `std::sync::mpsc`+`block_on`
同步壳。单条 `from_stream` 保序；别对每条 item 各起一个 `from_stream`（并发→乱序，见 §4）：

```rust
use rxrust::prelude::*;

pub async fn compile(source: String) -> String {
  Shared::from_stream(futures::stream::iter(items(source)))
    .scan_map((env0(), false), |(env, taken), item| {  // @if 链作为随流 move 的 owned 累加子值
      /* 返回本 item 的 Vec<Leaf>，写回 *taken */
  })
    .flat_map(Shared::from_iter)                  // 内联、顺序展平（保序）
    .map(to_css)
    .reduce_initial(String::new(), |mut a, s| { a.push_str(&s); a })
    .into_future()
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}
```

- `reduce` = `FnMut(Item,Item)->Item`（**无初值**）；要折叠成别的类型用 `reduce_initial(seed, FnMut(Acc,Item)->Acc)`。

## 2. 四个"必失守点"的原生写法

### (a) 循环 / 一项变多项 = `flat_map`（**不是**手搓队列重放）

```rust
// ✅ @each 式展开：把每条 item 驱动一条子 observable，loop 整个交给 rxrust
Local::from_iter(items)
  .flat_map(|item| body_stream(item))        // body_stream 返回一条子流
  .collect::<Vec<_>>()
  .subscribe(|leaves| { /* … */ });
```

```rust
// ❌ 反模式（sasslint: no-work-queue / no-from-fn-driver / mutable-accumulator-drives）
let mut q = VecDeque::new();                     // 手搓 work-queue
std::iter::from_fn(|| q.pop_front()).for_each(|job| { /* 手动 step/重放 */ });
```
`flat_map` 的本质就是 `MergeAll{ source: Map{source, f}, concurrent }`（`05 §F`）——
框架已经在做"扇出+合并"，别再造第二台引擎。

### (b) 跨元素状态（env / 累加子）= `scan_map` 携带 owned `acc`（**不是** `Arc<Mutex>`、**不是** `&mut self` 对象）

```rust
// ✅ acc 是被 move 进 observer 的独占数据，无需 Clone
Local::from_iter(sibs)
  .scan_map(env.clone(), |env, sib| {          // env 是 owned 累加子
    let out = resolve(sib, env);               // 读/改 env，产出本步输出
    out
  })
  .subscribe(|outputs| { /* … */ });
```
`scan_map` 的 `acc` 封在 `ScanMapObserver`（`scan_map.rs:54-75`）。折叠成单值用
`reduce_initial`；收成容器用 `collect`。**别**把 env 塞 `Arc<Mutex<..>>` 让多方共享可变——
那是 GC 逃逸，`sasslint`/`rust.md` 都禁。

### (c) 嵌套 / 树 = 流上的 owned 结构 + `flat_map` 下钻（**不是**先 parse 成 `Vec<Rule>` 再递归遍历）

判据（`rust.md` 反回归自查）：出现 `Vec<Rule>` / `.children` 递归下降 / `fn …->Vec<..>` 中转，
就是"物化结构再遍历"。正确形态：**结构作为被 move 的数据沿流走**，需要下钻处用 `flat_map`
把子节点当子流展开；深度用累加子里的 owned 栈表达（`reduce_initial`/`scan_map`），
不是调用栈递归、也不是 `from_fn` 手动驱动。

### (d) 一份产出喂多下游 = 多播（**不是** `collect` 进缓存再 `for_each` 挨个喂）

```rust
// 首订阅自动连、末退订自动断：
let shared = source.publish().ref_count();     // ConnectableObservable → 引用计数
let a = shared.clone().map(fa);
let b = shared.clone().filter(fb);
// 或热总线：Local::subject()/behavior_subject(init) + peek()（见 06）
```
一个模块编译一次被 N 个 parent 引用、一段解析同时喂选择器与值渲染，都用多播
（`06`），绝不缓存再手动重放。

## 3. 常见坑与判据

| 症状 | 根因（源码） | 正解 |
|------|--------------|------|
| `subscribe(\|v\|…)` 收不到错误/完成 | `FnMutObserver` 忽略 `error/complete`、且只 `Err=Infallible`（`03 §一.3`） | 先 `.on_error(…)`；或 `subscribe_with(自定义 Observer)`；或 `.into_future()`；或 in-stream `Result` |
| 想链式但写 `and_then` | 1.0 **无 `and_then`** | 跨阶段 `flat_map` |
| 类型写不出来 / 想返回 `impl` 流 | 算子链类型不可名状 | `.box_it()` → `LocalBoxedObservable`（`07`） |
| 需要"遍历一棵结构" | 物化 AST/树再递归 = GC 直觉 | 让结构是流上 owned 数据，`flat_map` 下钻（`§2c`） |
| 需要"跨 item 记状态" | 本能造 `&mut self` 对象 / `Arc<Mutex>` | `scan_map` owned acc / `reduce_initial`（`§2b`） |
| Subject 回调里再发值 panic | 再入被禁（`subject_core.rs:419-450`） | `delay(0)` 造异步边界 |
| `VecDeque`/`from_fn` 当 loop | 算子外衣下的手搓状态机 | `flat_map`；`sasslint` `no-work-queue`/`no-from-fn-driver`/`mutable-accumulator-drives` 会红 |
| wasm 编译报 `Send`/线程 | `Shared`+多线程上 wasm | 统一 `Local`（`04 §四`）；`into_shared` 的 `Send` 会逼你塞 `Arc` |
| `cargo add rxrust` 后 API 全错 | 解析到旧 0.15 | 锁 `=1.0.0-rc.6`（`README`） |
| 时间算子不动/需要 runtime | 只有 `05 §G` 走 scheduler | 纯同步管线避开它们；要计时才引 `LocalSet`/tokio |
| `Shared::from_stream` 文档序乱 | `flat_map(\|i\| from_stream(..))` 每条 item 各起一任务并发 | 用**单条** `from_stream`(顺序 poll) + `scan_map` 携 owned 累加子 + `flat_map(Shared::from_iter)` 内联顺序展平 |
| `.reduce(seed, f)` 报 E0061 | rc.6 `reduce`=`FnMut(Item,Item)->Item` 无初值 | 带初值用 `reduce_initial(seed, FnMut(Acc,Item)->Acc)` |
| `Shared::from_stream` 收口又套 `mpsc`+`block_on` | 在异步核心外裹 std 同步壳 | 终点用 `into_future().await`（runtime 由调用方供） |

## 4. Do / Don't 总纲

**Do**
- 起手先问"这条流推什么 owned 值、经哪个算子、变成哪条流"（自底向上，`01 §2`）。
- 循环 → `flat_map`；状态 → `scan_map`/`reduce_initial` 的 owned 累加子；一到多 → 多播；
  收口 → 同步 `Local` 链终点一次 `subscribe`+`mpsc`；`Shared`/`from_stream` 异步链用 `into_future().await`（别套 std 壳）。
- 错误当数据（in-stream `Result` + `Err=Infallible`）或显式 `on_error`/`into_future`。
- 跨函数/异构存流用 `box_it()`。

**Don't**（= `rust.md` 反回归自查 + `sasslint` 规则命中项）
- 不 `Vec<Rule>`/`.children` 递归下降遍历（`no-tree`/`no-recursive-descent`）。
- 不 `fn -> Vec<..>` 命令式中转（`no-relay-vec`）。
- 不把 token/值 `collect` 成字符串再 char 状态机重扫（`no-string-machine`）。
- 不 `std::iter::from_fn` + `VecDeque`/可变现代驱动"消掉 `for/while`"（`no-from-fn-driver`/
  `no-work-queue`/`mutable-accumulator-drives`）——这是"算子外衣下的状态机"，元错误。
- 不 `Arc<Mutex<业务态>>` / `&mut self` 承载跨元素共享可变。
- 不**在 rxrust 之上再造 stream 引擎 / AST 解释器**（`stream(node)` 递归造 observable 去遍历自建的树，
  仍是改造 rxrust）；`flat_map`/多播/冷流惰性本就是这些能力的内置原语。
- 不照 `dart-sass` 的"物化 AST→遍历→回收"心智建模；它只作**行为 oracle**（`sass-spec/*.hrx`）。

## 5. 验收闭环（项目现状）

改 sasspile 值/函数/流任一层，动手前后各跑：
1. `cargo run -p sasslint -- crates/sasspile/src`（或 MCP `review{path}`）→ **0 命中**；
2. `cargo test -p sasspile` + `cargo clippy --all-targets -- -D warnings` → 绿；
3. `ledger diff <base> <run>` → **`regress = 0`** 才合入，否则回退。
新算子/函数先做成 **pub 独立件 + 金样单测、不接 `compile`**，接线时**只在无歧义处触发**
（函数白名单、`/ * %` 跳过护 CSS），逐片守门（见项目记忆"sasspile 值/函数层安全生长范式"）。

---
本套文档完。核心一句话：**rxrust 本身就是引擎**——用它的 `flat_map`/多播/owned 累加子/冷流惰性
表达控制流，而不是在它之上再造一台。
