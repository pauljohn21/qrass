# V0-01 所有权与借用 —— 为什么值必须被"搬走"，而不是"借来看一眼"

> 零基础前置卷第 1 课。本课**不碰 rxrust 源码**，先把后面每一课都要用的地基打牢：
> 值的所有权、借用、以及"消费者的两个形态"（还能继续收 / 已经收完）。
>
> 配套代码（已在 workspace 外真编译 + 6 条断言全过）：
> - `code/00-01-ownership.rs`（本课要手敲的代码）
> - `code/00-01-ownership.tests.rs`（断言）
> 验证方法见文末"怎么跑"。

---

## 1. 本课要解决的问题（不解决会怎样）

响应式库要做的就是一件事：**把值一串一串往前送，送到消费者手上**。于是有三个绕不开的决策：

1. 送过去的是**值本身**，还是**指向值的引用**？
2. 消费者接收时要不要**改自己的内部状态**（比如累加）？要不要**换身份**（比如关闭）？
3. 消费者"收完"之后，如果还有人给它塞值，谁负责发现这个错误？

如果这三个决策靠"约定"和"运行时判断"来回答，就会长出：引用悬挂（值已释放但还有人指着它）、
边发边被改的竞态、以及"终止后还能收"的隐式 bug——这正是带垃圾回收的语言里那些
"谁持有、还活着吗、能不能释放"的经典麻烦。

Rust 给了另一条路：**把答案写进类型**。本课就把这条路铺出来。

## 2. 你先试（先卡住，再看讲解）

在纸上（或临时 crate 里）写三个函数，先别查资料：

- **A** 写一个函数，把一串 `i32` 加起来，输入是"借来看的"数据。
- **B** 写一个函数，把一串 `i32` **逐个交给另一个函数处理**，数据归它所有。
- **C** 给 B 加一条规则：处理方说"我够了"，剩余数据就不许再给它。

写不出来很正常。A 与 B 的区别（`&[i32]` vs `Vec<i32>`）就是"拉 vs 推"的分水岭，
C 里你必然要造一个"处理方"——它就是下一课的 `Observer`。

## 3. 本课代码（请手敲；敲完再往下读）

```rust
/// 【拉 pull】调用方决定何时取：从借来的切片上算一遍。
pub fn pull_sum(values: &[i32]) -> i32 {
    let mut total = 0;
    for v in values {
        total += *v;
    }
    total
}

/// 【推 push】被调用方决定何时发：整份 `Vec` 的所有权交给它，逐个数塞进 `sink`。
pub fn push_all(values: Vec<i32>, sink: &mut Vec<i32>) {
    for v in values {
        sink.push(v);
    }
}

/// 推 + 早停：接收方不接了，源头据此停止发射。
pub fn push_until(values: Vec<i32>, limit: usize, sink: &mut Vec<i32>) -> usize {
    let mut emitted = 0;
    for v in values {
        if emitted >= limit {
            break;
        }
        sink.push(v);
        emitted += 1;
    }
    emitted
}

/// 值必须被搬走的真正理由：`String` 拥有堆上的字符缓冲。
pub fn push_names(names: Vec<String>, sink: &mut Vec<String>) {
    for name in names {
        sink.push(name);
    }
}

/// 消费者协议：`receive` 要可变借用；`finish` 消耗自身。
pub trait Sink {
    fn receive(&mut self, value: i32);
    fn finish(self) -> i32;
    fn closed(&self) -> bool;
}

#[derive(Default)]
pub struct Counter {
    total: i32,
    seen: usize,
    limit: usize,
}

impl Counter {
    pub fn new(limit: usize) -> Counter {
        Counter { limit, ..Counter::default() }
    }
}

impl Sink for Counter {
    fn receive(&mut self, value: i32) {
        self.total += value;
        self.seen += 1;
    }
    fn finish(self) -> i32 {
        self.total
    }
    fn closed(&self) -> bool {
        self.seen >= self.limit
    }
}

/// 推式管道 + 提前取消。
pub fn run(values: Vec<i32>, limit: usize) -> i32 {
    let mut sink = Counter::new(limit);
    for v in values {
        if sink.closed() {
            break;
        }
        sink.receive(v);
    }
    sink.finish()
}
```

## 4. 逐行讲解（每一行都讲）

### `pull_sum`：借来看一眼

| 行 | 讲什么 |
|---|---|
| `pub fn pull_sum(values: &[i32]) -> i32 {` | 参数类型 `&[i32]` 读作"借一段连续的 `i32`，我不拥有它"。数据仍归调用方那个 `Vec` 所有。函数返回 `i32`（一个自己算出来的新值），没有把任何借来的东西带出去。 |
| `let mut total = 0;` | 累加器。`mut` 是因为下面要改它；不写 `mut` 编译器立刻报错（这是 Rust 的优点：可变性写在类型里）。 |
| `for v in values {` | 遍历"借来的视图"，因此 `v` 的类型是 `&i32`——借来的元素引用，不是元素本体。 |
| `total += *v;` | `*v` 是"解引用"：从引用拿到那个 `i32`。因为 `i32` 是 `Copy`，这一步相当于复制一个 4 字节整数，没有所有权问题。 |
| `total` | 函数体最后一个表达式即返回值（没有分号的那个表达式）。 |
| `}` | `values` 的借用在这里结束，调用方的 `Vec` 一直被它自己拥有——这就是"拉"：调用方拿着数据，随时可以自己再算一遍。 |

**为什么不处处用 `&`？** 因为"借来看"要求被借的东西活得比这次使用更久，而且同一时间对同一数据的可变访问受严格限制。一旦我们要做"值路过一站又一站、每站都可能把它改一改或存下来"的事，引用就会把一堆生命周期的负担甩给调用方。下面看推的做法。

### `push_all`：整份数据归它

| 行 | 讲什么 |
|---|---|
| `pub fn push_all(values: Vec<i32>, sink: &mut Vec<i32>) {` | `values: Vec<i32>`——注意没有 `&`。调用时 `Vec` 的**所有权被搬进**这个函数（move）。`sink: &mut Vec<i32>` 是"可变借用"：函数不拥有 sink，但可以往里放东西。 |
| `for v in values {` | 这里 `v` 的类型是 `i32` 本体（拥有），因为 `values` 被这个循环消费掉了。 |
| `sink.push(v);` | 把拥有权继续往 sink 里搬。全程任何一刻，每个数只有**一个**主人。 |
| `}` | `values` 的外壳 `Vec` 在这里被释放（里面的 `i32` 已经搬进 sink 了）。 |

对照 `pull_sum`：两者的区别只在参数有没有 `&`。但这一个 `&` 决定了两件事——**谁拥有数据**，以及**控制流在谁手里**（前者是调用方遍历，后者是被调用方遍历）。推式（push）才是响应式库的骨架：发不发由源头决定，消费者不能"回头再取一次"。

### `push_until`：早停是"值"，不是异常

| 行 | 讲什么 |
|---|---|
| `pub fn push_until(values: Vec<i32>, limit: usize, sink: &mut Vec<i32>) -> usize {` | 多了 `limit: usize`（"最多发几个"）并返回"实际发了几个"。 |
| `let mut emitted = 0;` | 发射计数。 |
| `if emitted >= limit { break; }` | **早停点**。每要发一个之前先问一次"还发得出去吗"。下一课你会看到 rxrust 把这一句写成 `if observer.is_closed() { break; }`。 |
| `sink.push(v); emitted += 1;` | 发出去才计数，所以返回值语义是"真正交付了几个"。 |
| `emitted` | 返回计数，方便上层断言（见测试 `early_stop_reports_exactly_how_many_were_emitted`）。 |

为什么要 `break` 而不是 `panic!`/返回 `Err`？取消是**正常控制流**，不是错误。用值传播，调用方就不必 catch 任何东西——这是响应式库和"异常式解析器"的分水岭。

### `push_names`：为什么非要有所有权

| 行 | 讲什么 |
|---|---|
| `pub fn push_names(names: Vec<String>, sink: &mut Vec<String>) {` | `String` = "堆上一段字符缓冲 + 长度 + 容量"，并且**拥有**那块缓冲。 |
| `for name in names {` | 每个 `name: String` 是被搬出来的拥有者。 |
| `sink.push(name);` | 所有权交给 sink；原来那个 `Vec<String>` 的外壳被逐个搬空。 |

如果把签名改成 `names: &[String], sink: &mut Vec<&String>`，那么 sink 里存的是一堆"指向别人家缓冲区的引用"：这些引用必须在 sink 存活期间一直有效——于是你必须给函数加生命周期参数、并保证调用方永远不早于 sink 释放那个 `&[String]`。**"谁拥有、活多久、何时释放"又变成需要人肉维护的问题。** 推式流用所有权把这类问题一次性消灭：值搬进来、搬出去、最后一站拥有它，中间没有别名。

### `trait Sink`：消费者的两种形态

| 行 | 讲什么 |
|---|---|
| `fn receive(&mut self, value: i32);` | 要改内部状态（累加、计数）→ 所以 `&mut self`；但不能消耗自己，因为后面还要继续收。 |
| `fn finish(self) -> i32;` | 接收者是 `self` 而非 `&self`：**这个方法会消耗掉消费者**，交出最终结果。消耗之后再想 `receive` 就没有对象可用了。 |
| `fn closed(&self) -> bool;` | 只读地问"收够了吗"，所以 `&self`。它让源头能在发射前做决定。 |

这三行就是本课给下一课准备的模板：`next(&mut self, Item)` / `complete(self)` / `is_closed(&self)`。注意"终止"和"接收"在类型上就是不同形状——这不是风格，是把协议写进类型。

### `Counter` 与 `run`：拼起来

| 行 | 讲什么 |
|---|---|
| `#[derive(Default)]` | 让编译器生成"全零/全空"的默认值，下面 `..Counter::default()` 才写得出。 |
| `total: i32,` | 累加 `i32` 的值，字段就必须是 `i32`。（我第一次写成 `usize`，编译器立刻拦下，见 §6 第 2 条——这就是"编译器是对话对象"的实例。） |
| `seen: usize, limit: usize,` | 分别记录"已收几个"和"最多收几个"。 |
| `Counter { limit, ..Counter::default() }` | 函数式更新语法：只指定要改的字段，其余取默认，避免手写一堆零。 |
| `fn receive(&mut self, value: i32) { self.total += value; self.seen += 1; }` | 状态推进：累加 + 计数。两个动作都要求可变借用。 |
| `fn finish(self) -> i32 { self.total }` | 交出结果并消耗自己。 |
| `fn closed(&self) -> bool { self.seen >= self.limit }` | 只读判断。 |
| `let mut sink = Counter::new(limit);` | 造消费者，`mut` 因为后面 `receive` 要改它。 |
| `for v in values {` | 源头逐个数往前推（`values` 所有权已在这个 `run` 里）。 |
| `if sink.closed() { break; }` | 发之前问一次消费者，取消是值。 |
| `sink.receive(v);` | 推给它。 |
| `sink.finish()` | 消耗 sink、拿回结果。这一行之后 `sink` 已不存在。 |

`run(vec![1,2,3,4,5], 3) == 6`（只收 3 个），`run(vec![1,2,3,4,5], 99) == 15`（全收）——两个断言都在测试里。

## 5. 对照 rxrust 真实源码（下一课会逐行讲它，这里先对齐词汇）

| 本课 | rxrust rc.6 | 说明 |
|---|---|---|
| `Sink::receive(&mut self, i32)` | `Observer::next(&mut self, value: Item)`（`observer.rs:21`） | 一模一样：可变借用 + 按值接收。 |
| `Sink::finish(self)` | `Observer::complete(self)`（`observer.rs:33`）与 `error(self, Err)`（`:27`） | 都消耗自身，因为"之后不许再发"。 |
| `Sink::closed(&self) -> bool` | `Observer::is_closed(&self) -> bool`（`observer.rs:42`） | 源码注释直接点名：给 `from_iter` 这类源头判断是否提前停。 |
| `for v in values { if closed break; … }` | `from_iter.rs:64-74` | 结构完全一致，只是它把 `Item` 泛型化、把 sink 包进 `Context`。 |
| `i32` / `String` | `Item`（泛型参数 + GAT `type Item<'a>`） | 本课只有具体类型；GAT 是 V0-02/V4 的内容。 |

一句话：本课造的 `Sink` + `run`，就是 `Observer` + `subscribe` 的**未泛型化、未多线程化**版本。下一课把"类型"补上，再下一课把"线程模型"补上。

## 6. 你会撞到的编译错（全部是真实跑出来的原文）

### ① `E0382: borrow of moved value` —— 数据已经搬走了，你还想用

复现：把 `push_all(moved, &mut sink);` 写在一行，然后再用 `moved`（测试文件里我留了注释掉的复现块）。

```
error[E0382]: borrow of moved value: `v`
  |
1 | fn take(_v: Vec<i32>) {} pub fn a(v: Vec<i32>) -> usize { take(v); v.len() }
  |                                   -                            -   ^ value borrowed here after move
  |                                   move occurs because `v` has type `Vec<i32>`, which does not implement the `Copy` trait
note: consider changing this parameter type in function `take` to borrow instead if owning the value isn't necessary
```

怎么改：要么先算完再搬，要么把参数改成 `&Vec<i32>`/`&[i32]`（但那就回到"拉"）。注意编译器给的 note——它建议"如果你不需要拥有，就改成借用"，这正好说明**推式（要所有权）是有代价的选择**，我们为了流语义的干净付这个代价。

### ② `E0308` + `E0277` —— 我自己在写这课时空想的产物

我把 `Counter.total` 写成 `usize` 却 `+= i32` 的值：

```
error[E0308]: mismatched types
1 | ... { self.total += value; } }
  |                     ^^^^^ expected `usize`, found `i32`
error[E0277]: cannot add-assign `i32` to `usize`
    = help: the trait `AddAssign<i32>` is not implemented for `usize`
```

修法：字段类型跟数据同族（这里 `total: i32`）。教训：**别凭习惯猜类型，写完就编译**。这条我也写进了 `VERIFICATION.tsv`。

### ③ `E0106: missing lifetime specifier` —— 返回值引用没说清来自哪个参数

复现：`pub fn pick(a: &str, b: &str) -> &str { if a.len() > b.len() { a } else { b } }`

```
error[E0106]: missing lifetime specifier
108 | pub fn pick(a: &str, b: &str) -> &str { ... }
    |                ----     ----     ^ expected named lifetime parameter
= help: this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `a` or `b`
help: consider introducing a named lifetime parameter
108 | pub fn pick<'a>(a: &'a str, b: &'a str) -> &'a str
```

为什么这一课要见它：`pull` 风格一旦想把"借来的值"存进别处，就会被要求写生命周期。而 `push`（按值传）大多数时候根本不需要面对这个问题——这是选所有权路线的红利。

### ④ `E0382: use of moved value` —— 终止方法真的把消费者拿走了

复现：`let _ = x.finish(); x.finish()`（`fn finish(self)`）

```
error[E0382]: use of moved value: `x`
1 | ...let x = X; let _ = x.finish(); x.finish() }
  |                     -  --------  ^ value used here after move
  |                     `x` moved due to this method call
note: `S::finish` takes ownership of the receiver `self`, which moves `x`
```

这条正是我们要的**免费协议检查**：`complete(self)`/`error(self)` 之后不可能再 `next`——`observer.rs:27-33` 的注释说的就是这件事，而 Rust 让它不需要运行时断言。

## 7. 设计取舍（不这样写会怎样）

| 决策 | 本课做法 | 另一种做法及其代价 |
|---|---|---|
| 值怎么交给下一站 | 按值 `Item`（所有权搬走） | 按引用：要生命周期、要"别释放"约定，链一长就变成人肉追踪别名 |
| 消费者状态怎么改 | `&mut self`，状态归消费者自己 | 状态放外面 + `Fn(&State)`：要么每站重建状态包，要么共享可变 |
| 终止怎么保证 | `finish(self)` 消耗自身 | `&mut self` + `finished: bool`：靠运行时判断，忘了判断就 UB 级逻辑错 |
| 取消怎么传 | `closed() -> bool` + `break` | `panic!`/`Result` 当控制流：取消变成异常，调用方被迫 catch |
| 谁决定何时发 | 被调用方（push） | 调用方（pull）：那才是 `Iterator`，无法自然表达"源头异步/多播/中途取消" |

## 8. 反例（本项目真踩过的，别学）

1. **把 sink 做成 `Rc<RefCell<Sink>>` 到处 clone**，好让"多个地方都能往里塞"。这不是响应式，是共享可变；框架给多消费者的答案是 `Subject`（V5 会自己写出来）。
2. **`collect` 成 `Vec` 再回头遍历**，把推式链降级成批处理——本项目就因此被判"GC 思维"。真需要"看到全体"时（如 `@extend`），它是**一次纯函数折叠**，不是把每条中间结果都攒起来。
3. **用 `&mut self` 的对象当"驱动者"**：让对象自己决定下一步访问谁。那是访问者模式（拉），不是流。本课 `run` 的循环在**函数**里、状态在**值**里，方向是对的。

## 9. 练习（先做，再看答案）

1. 把 `push_all` 改成 `push_all_with(values: Vec<i32>, f: impl FnMut(i32))`，用它打印每个值的平方。为什么要 `impl FnMut` 而不是 `impl Fn`？
2. 给 `Counter` 加 `fn reset(&mut self)`，然后尝试在 `finish()` 之后调用它，把编译器的原话抄下来。
3. 写 `fn push_first(values: Vec<i32>, n: usize, sink: &mut Vec<i32>)`，要求**不**使用 `break`（提示：把判断变成"要不要执行这一步"的值）。
4. 把 `run` 改造成接收 `impl Sink` 的泛型版本：`fn run<S: Sink>(values: Vec<i32>, limit: usize, sink: S) -> i32`。想一想：`limit` 现在该由谁持有？

### 参考答案

```rust
// 1) FnMut：闭包会改环境（累加/打印计数），`Fn` 只读，编译器会要求 mut
pub fn push_all_with(values: Vec<i32>, mut f: impl FnMut(i32)) {
    for v in values {
        f(v * v);
    }
}

// 2) 报错与 §6④ 同类：`cannot borrow *x as mutable, as it is moved`，
//    因为 `finish(self)` 已把 `sink` 的所有权消耗掉，之后没有对象可借用。

// 3) 用值代替 break：把"要不要发"变成一个布尔，配合迭代器链
pub fn push_first(values: Vec<i32>, n: usize, sink: &mut Vec<i32>) {
    values.into_iter().enumerate().for_each(|(i, v)| {
        if i < n {
            sink.push(v);
        }
    });
}

// 4) limit 归消费者持有更内聚：由 `Sink::new(limit)` 造出，run 只负责推
pub fn run<S: Sink>(values: Vec<i32>, mut sink: S) -> i32 {
    for v in values {
        if sink.closed() {
            break;
        }
        sink.receive(v);
    }
    sink.finish()
}
```

## 10. 怎么跑（验证工装在仓库外，别在 `target/` 里 `cargo new`）

```bash
cargo new --lib /tmp/minirx-check
D=docs/rxrust-from-scratch/v0-foundations/code
cp $D/00-01-ownership.rs /tmp/minirx-check/src/lib.rs
mkdir -p /tmp/minirx-check/tests
cp $D/00-01-ownership.tests.rs /tmp/minirx-check/tests/lesson.rs
cd /tmp/minirx-check && cargo test          # 期望：6 passed
```

> 为什么在 workspace 外：实测在仓库 `target/` 下 `cargo new` 会改写根 `Cargo.toml` 的
> `workspace.members`（本项目发生过一次并已复原）。

---

**下一课 V0-02**：trait 与泛型、关联类型，以及为什么最后需要 **GAT**（`type Item<'a>`）——
我们会先用普通关联类型写一版，让编译器的报错把我们逼到 GAT，那时你就懂了 rxrust 为什么长成那样。
