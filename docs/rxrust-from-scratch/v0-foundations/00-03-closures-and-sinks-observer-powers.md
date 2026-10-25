# V0-03 `Fn` / `FnMut` / `FnOnce` —— 为什么 `subscribe(closure)` 会悄悄吞掉 `error` 和 `complete`

> 零基础前置卷第 3 课。上一课我们让"元素类型"能带生命周期；这一课解决**消费者的能力等级**：
> 为什么 `Observer::next` 只能是 `FnMut` 语义、为什么终止方法要消耗自身，
> 以及由此推出的一个真实陷阱：`subscribe(|v| …)` 之后，错误与完成信号去哪了。
>
> 配套代码在 workspace 外真编译真跑：`code/00-03-closures-and-sinks.rs`
> （5 条断言 + 答案文件 3 条断言、`clippy --all-targets -D warnings` 干净）。
> §6 的四条报错都是现场复现的原文，不是我背诵的。

---

## 1. 本课要解决的问题（不解决会怎样）

三个问题串在一起：

1. 用户传给 `subscribe` 的是一个闭包。闭包有三种"能力"，差别在**它怎么对待自己捕获的环境**：
   只读（`Fn`）、可改（`FnMut`）、只能一次性用掉（`FnOnce`）。
2. 消费者协议本身要求"接收可反复、终止只一次"（V0-01 已见过 `&mut self` 与消耗式 `self`）。
   这两件事必须对上：什么样的闭包能当消费者？
3. 如果对不上会发生什么？——**错误信号静默消失**。这不是假想：
   `observer.rs:255-277` 的 `FnMutObserver` 里 `error` 与 `complete` 就是**空函数体**。

第 3 点是本项目真实踩过的坑（我们一度用 `.ok()` 把求值错误折成"退回原样"，
和"闭包吞错"是同族问题：失败信号没有出口）。所以本课不只是讲语法，是把**失败的可观察性**立起来。

## 2. 你先试（先卡住，再看讲解）

1. 写一个函数，接收一个"只读"闭包（`impl Fn(i32) -> i32`），对 `&[1,2,3]` 逐个求平方。
2. 再写一个函数，接收一个"要累加"的闭包：把参数写成 `impl Fn(i32)`，调用方传 `&mut |v| total += v`。
   **看编译器说什么**（§6①）。
3. 写一个消费者 trait：`next(&mut self, Item)` + 两个消耗自身的终止方法。然后给你的
   闭包适配器只实现 `next`，另两个写成空实现，把一条 `Err("boom")` 喂进去——
   写一条断言证明"错误没到达任何地方"。

## 3. 本课代码（请手敲；敲完再往下读）

```rust
/// 只读捕获：闭包不改环境，所以 `impl Fn`。
pub fn read_only(f: impl Fn(i32) -> i32, values: &[i32]) -> Vec<i32> {
    values.iter().map(|v| f(*v)).collect()
}

/// 可变状态：累加要改环境，所以必须 `impl FnMut`。
pub fn apply_each(values: &[i32], f: &mut impl FnMut(i32)) {
    for v in values {
        f(*v);
    }
}

/// 一次性：闭包可能把捕获的东西搬走，只能调一次。
pub fn consume_once(f: impl FnOnce(i32) -> i32, value: i32) -> i32 {
    f(value)
}

/// `move` 把 `n` 搬进闭包，之后每次调用都改它 ⇒ 只满足 `FnMut`。
pub fn once_counter() -> impl FnMut() -> usize {
    let mut n = 0;
    move || {
        n += 1;
        n
    }
}

/// 消费者协议（对照 `observer.rs:19-43`）。
pub trait Observer<Item, Err> {
    fn next(&mut self, value: Item);
    fn error(self, err: Err);
    fn complete(self);
    fn closed(&self) -> bool;
}

/// 闭包适配器：只有 `next` 有实现，两个终止信号是空函数体
/// （这就是 `observer.rs:255-277` 的机制）。
pub struct FnObserver<C>(pub C);

impl<C: FnMut(i32)> Observer<i32, &'static str> for FnObserver<C> {
    fn next(&mut self, value: i32) {
        (self.0)(value);
    }
    fn error(self, _err: &'static str) {}
    fn complete(self) {}
    fn closed(&self) -> bool {
        false
    }
}

/// 日志归调用方的消费者：终止信号是否送达，从此可以断言。
pub struct Recorder<'a>(pub &'a mut Vec<&'static str>);

impl Observer<i32, &'static str> for Recorder<'_> {
    fn next(&mut self, _value: i32) {}
    fn error(self, err: &'static str) {
        self.0.push(err);
    }
    fn complete(self) {}
    fn closed(&self) -> bool {
        false
    }
}

/// 每个信号都记下来（`Echo`）；`Limited` 则收满两个就关门。
pub struct Echo<'a>(pub &'a mut Vec<String>);

impl Observer<i32, &'static str> for Echo<'_> {
    fn next(&mut self, value: i32) {
        self.0.push(format!("next {value}"));
    }
    fn error(self, err: &'static str) {
        self.0.push(format!("error {err}"));
    }
    fn complete(self) {
        self.0.push("complete".to_string());
    }
    fn closed(&self) -> bool {
        false
    }
}

pub struct Limited<'a> {
    pub log: &'a mut Vec<String>,
    seen: usize,
}

impl<'a> Limited<'a> {
    pub fn new(log: &'a mut Vec<String>) -> Limited<'a> {
        Limited { log, seen: 0 }
    }
}

impl Observer<i32, &'static str> for Limited<'_> {
    fn next(&mut self, value: i32) {
        self.seen += 1;
        self.log.push(format!("next {value}"));
    }
    fn error(self, err: &'static str) {
        self.log.push(format!("error {err}"));
    }
    fn complete(self) {
        self.log.push("complete".to_string());
    }
    fn closed(&self) -> bool {
        self.seen >= 2
    }
}

/// `Option<O>` 本身也能当消费者：被终止后变 `None`，后续 `next` 自动 no-op
/// （对照 `observer.rs:284-335`）。
impl<O: Observer<Item, Err>, Item, Err> Observer<Item, Err> for Option<O> {
    fn next(&mut self, value: Item) {
        if let Some(inner) = self {
            inner.next(value);
        }
    }
    /// 这里反而需要 `mut self`：`take()` 要可变借用那个 `Option` 本身。
    fn error(mut self, err: Err) {
        if let Some(inner) = self.take() {
            inner.error(err);
        }
    }
    fn complete(mut self) {
        if let Some(inner) = self.take() {
            inner.complete();
        }
    }
    fn closed(&self) -> bool {
        self.as_ref().is_none_or(Observer::closed)
    }
}

/// 推式发射：先推完安全前缀，最后**一次性**交出终止信号。
pub fn emit<O: Observer<i32, &'static str>>(
    items: Vec<Result<i32, &'static str>>,
    mut sink: Option<O>,
) {
    let (mut prefix, mut failure): (Vec<i32>, Option<&'static str>) = (Vec::new(), None);
    for item in items {
        match (item, failure) {
            (Ok(value), None) => prefix.push(value),
            (Err(err), None) => failure = Some(err),
            // 已经终止过：后面的元素一律不再看
            _ => {}
        }
    }
    for value in prefix {
        if sink.closed() {
            break;
        }
        sink.next(value);
    }
    match failure {
        Some(err) => sink.error(err),
        None => sink.complete(),
    }
}
```

## 4. 逐行讲解（每一行都讲）

### 三种闭包能力：差别只在"怎么处理捕获"

| 行 | 讲什么 |
|---|---|
| `pub fn read_only(f: impl Fn(i32) -> i32, values: &[i32]) -> Vec<i32> {` | `Fn` = 只读捕获：同一个闭包可以被反复调用而互不干扰。 |
| `values.iter().map(\|v\| f(*v)).collect()` | `*v` 是把 `&i32` 解成 `i32` 再交给闭包（V0-01 的 `Copy` 小知识）。 |
| `pub fn apply_each(values: &[i32], f: &mut impl FnMut(i32)) {` | 两级可变：闭包本身要可变（`FnMut`），所以参数是 `&mut impl FnMut(i32)`；只写 `mut f: impl FnMut` 也够用，这里用 `&mut` 是为了让调用方的闭包能"跨多次调用"保留状态。 |
| `for v in values { f(*v); }` | 每一轮都调同一个闭包 ⇒ 它必须是 `FnMut` 级以上。 |
| `pub fn consume_once(f: impl FnOnce(i32) -> i32, value: i32) -> i32 { f(value) }` | `FnOnce` 只保证能调一次（它可能把捕获的值搬走或 drop 掉）。在函数里 `f(value)` 之后就再无 `f`，编译器不会抱怨。 |
| `pub fn once_counter() -> impl FnMut() -> usize {` | 返回值是"能力未知的闭包"，用 `impl FnMut` 明确承诺：可反复调、会改状态。 |
| `let mut n = 0;` | 被捕获的环境。 |
| `move \|\| { n += 1; n }` | `move` 把 `n` 的所有权搬进闭包；闭包要改 `n` ⇒ 它只实现 `FnMut`，不可能是 `Fn`。 |

`Fn: FnMut: FnOnce` 的包含关系是单向的：**能 `Fn` 的一定能 `FnMut`/`FnOnce`，反之不行**。
这就是消费者该收哪一种的依据：`Observer::next` 要 `&mut self`，等价地，闭包至少要 `FnMut`。

### 闭包适配器：吞错发生在哪儿

| 行 | 讲什么 |
|---|---|
| `pub struct FnObserver<C>(pub C);` | 一个把闭包"伪装成消费者"的适配器。 |
| `impl<C: FnMut(i32)> Observer<i32, &'static str> for FnObserver<C> {` | 约束只要求 `FnMut`——正好配 `next(&mut self, …)`。 |
| `fn next(&mut self, value: i32) { (self.0)(value); }` | `next` 转发给闭包。 |
| `fn error(self, _err: &'static str) {}` | **空实现**。错误到这里为止：不记录、不 panic、不返回给调用方。 |
| `fn complete(self) {}` | 同上，完成信号也没人接。 |
| `fn closed(&self) -> bool { false }` | 永远不关门——所以它永远不会让源头早停。 |

这四行（两个空实现 + 一个 `false`）就是 `subscribe(closure)` 的全部代价。rxrust 用同样的写法
（见 §5），并把它写进文档："只对不可能失败的 observable 可用"。

### `Recorder` / `Echo`：让"丢没丢"变成可断言的事

| 行 | 讲什么 |
|---|---|
| `pub struct Recorder<'a>(pub &'a mut Vec<&'static str>);` | 日志存在**调用方手里**。`emit` 把消费者按值吃掉之后，Vec 仍在调用方手上。 |
| `fn error(self, err: &'static str) { self.0.push(err); }` | 不需要 `mut self`：写入的是 `self.0` 这个 `&mut Vec` 字段，透过共享借用就能推。这是本课的小知识点——`mut` 加在哪一层要算准（答案文件里的 `Collecting` 反而必须 `mut self`，因为它**拥有**那个 `Vec`）。 |
| `pub struct Echo<'a>(pub &'a mut Vec<String>);` | 同一种手法：所有信号统一写成 `String`，一条 `Vec` 记录"谁收到了什么、按什么顺序"。 |
| `impl Observer<i32, &'static str> for Echo<'_> { fn error(self, err: &'static str) { self.0.push(format!("error {err}")); } ... }` | 于是 `emit(..., Some(FnObserver(..)))` 与 `emit(..., Some(Echo(..)))` 的差别变成一条断言：错误文本进没进 `Vec`。 |
| `pub struct Limited<'a> { pub log: &'a mut Vec<String>, seen: usize }` / `fn closed(&self) -> bool { self.seen >= 2 }` | "收够就关门"，用来验证 `closed()` 真能拦住发射。 |

### `Option<O>` 当消费者：`take()` 表达"人不在了"

| 行 | 讲什么 |
|---|---|
| `impl<O: Observer<Item, Err>, Item, Err> Observer<Item, Err> for Option<O> {` | 给 `Option<O>` 实现同一个协议——不是我的发明，`observer.rs:284-335` 正是这么做的（并覆盖到 `Rc<RefCell<Option<O>>>` 一族）。 |
| `fn next(&mut self, value: Item) { if let Some(inner) = self { inner.next(value); } }` | 还没终止就转发；已经 `None` 就静默 no-op。 |
| `fn error(mut self, err: Err) { if let Some(inner) = self.take() { inner.error(err); } }` | 需要 `mut self`：`take()` 要可变借用那个 `Option` 本身；`take()` 之后 `self` 变 `None`。 |
| `fn closed(&self) -> bool { self.as_ref().is_none_or(Observer::closed) }` | `is_none_or`：`None` 视为已关（没人可收了）。这一句让"终止之后"自动传染给源头。 |

### `emit`：为什么不在循环里直接 `sink.error(e)`

| 行 | 讲什么 |
|---|---|
| `let (mut prefix, mut failure): (Vec<i32>, Option<&'static str>) = (Vec::new(), None);` | 先把输入拆成"安全前缀 + 第一个错误"。 |
| `match (item, failure) { (Ok(value), None) => prefix.push(value), (Err(err), None) => failure = Some(err), _ => {} }` | 一个 `match` 处理三种情形；一旦出现错误，后面的项全部落到 `_`（协议：终止后不再看数据）。 |
| `for value in prefix { if sink.closed() { break; } sink.next(value); }` | 发射循环，`closed()` 早停——`from_iter` 的同款结构。 |
| `match failure { Some(err) => sink.error(err), None => sink.complete() }` | **出口只有一个**：两条分支各自消耗 `sink`。 |

关键在最后一行：如果把它写进循环里（`sink.error(e); ... sink.next(v)`），编译器立刻拒绝——
`error(self)` 把 `sink` 搬走了（§6④）。所以"终止后不再发射"不是我的纪律，是类型逼出来的形状。

## 5. 对照 rxrust 真实源码

| 本课 | rxrust rc.6 | 为什么这样设计 |
|---|---|---|
| `FnObserver<C: FnMut(i32)>` + 两个空实现 | `FnMutObserver<F>`：`next` 转发、`error`/`complete` 空体、`is_closed` 恒 `false`（`observer.rs:255-277`） | 一行不多：闭包只关心值，所以另两个信号"必须"存在但"必然"没人接。 |
| `emit(.., sink: Option<O>)` | `Option<O>` 与 `RcDerefMut<Target=Option<O>>` 两个 `Observer` impl，终止时用 `take()`（`observer.rs:284-335`） | 循环里不能移动消费者，但终止又要移动它 → 把"有/无"做成值，`take()` 就是"取走并结束"。 |
| `impl Observer<Item, Err> for Option<O>` | 同上 | 也是 `is_closed()` 传染的机制：终止过的下游，源头会立刻看到 `true`。 |
| `pub fn subscribe(f: impl FnMut)` | `fn subscribe<F, U>(self, f: F) where F: for<'a> FnMut(Self::Item<'a>)`（`observable.rs:144-150`） | 注意 `for<'a>`：V0-02 的 HRTB 在这里兑现。文档还写明"**只对不可能失败的 observable 可用；有错误先 `on_error`**"。 |
| （未做）`Box<dyn Observer>` | `DynObserver` + `box_error(self: Box<Self>, err)` + blanket impl（`observer.rs:110-125`） | 因为 `fn error(self)` 无法通过 `dyn` 调用（§6③）。rxrust 造一个 object-safe 镜像来翻这堵墙。 |
| （未做）`on_error` | `fn on_error<F>(self, f: F)`（`observable.rs:985`） | 官方给的"闭包之外还有一条错误通道"。 |

## 6. 你会撞到的编译错（每条都是现场复现）

### ① `E0277` —— 需要 `FnMut` 的地方给了只读能力 / 或反过来

复现：把 `apply_each` 的参数声明改成 `f: impl Fn(i32)`，调用处仍传 `&mut |v| total += v`。

```
error[E0277]: expected an `Fn(i32)` closure, found `&mut {closure@src/lib.rs:189:65: 189:68}`
    |
189 | ...total = 0; apply_each(&[1, 2], &mut |v| total += v); assert_eq!(tota...
    |               ----------          ^^^^^^^^^^^^^^^^^^^ expected an `Fn(i32)` closure
    = help: the trait `Fn(i32)` is not implemented for `&mut {closure@...}`
note: required by a bound in `apply_each`
```

结论：`Observer::next` 要 `&mut self`，所以适配器约束必须是 `FnMut`；写成 `Fn` 会在**调用点**炸。

### ② `E0382` —— `FnOnce` 只能调一次

复现：`pub fn twice_bad(f: impl FnOnce(i32), v: i32) { f(v); f(v); }`

```
error[E0382]: use of moved value: `f`
note: `FnOnce` closures can only be called once
    | pub fn twice_bad(f: impl FnOnce(i32), v: i32) { f(v); f(v); }
    |                          ^^^^^^^^^^^            ---- this value implements `FnOnce`, which causes it to be moved when called
```

这解释了"消耗式方法"为什么自然：`FnOnce` 与 `fn error(self)` 是同一件事的两种写法。

### ③ `E0161` —— 不能通过 `dyn` 调用消耗 `self` 的方法

复现：`pub fn via_box(b: Box<dyn Observer<i32, &'static str>>, e: &'static str) { b.error(e); }`

```
error[E0161]: cannot move a value of type `dyn Observer<i32, &str>`
    | ... str>>, e: &'static str) { b.error(e); }
    |                               ^ the size of `dyn Observer<i32, &str>` cannot be statically determined
```

**这就是 `DynObserver` 存在的唯一理由**：把 `error`/`complete` 改写成 `self: Box<Self>` 版本
（`observer.rs:112-113`），再由 blanket impl（`:117-125`）自动接回真协议。答案文件里你自己实现了一遍。

### ④ `E0382` —— 终止之后再发（协议在类型层的兑现）

复现：`fn after_terminal(mut sink: Option<Echo<'_>>, e: &'static str) { sink.error(e); sink.next(1); }`

```
error[E0382]: borrow of moved value: `sink`
note: `Observer::error` takes ownership of the receiver `self`, which moves `sink`
   --> src/lib.rs:35:14
 35 |     fn error(self, err: Err);
    |              ^^^^
```

修法是 `take()`（见本课 `Option` 实现与断言集最后一条）：先取出再终止，剩下的 `None` 天然 no-op。

### ⑤ 一个"反过来"的警告：多余的 `mut`

`fn error(mut self, err: &str) { self.0.push(err) }`（`self.0: &'a mut Vec`）会被
`unused_mut` 提醒"不需要 `mut`"；而 `Collecting { seen: Vec<String> }` 的 `fn error(mut self, …)`
就**必须** `mut`。同一个协议里 `mut` 加不加取决于状态是"借来的 `&mut`"还是"自己拥有的"——
这类细节别靠记忆，写一次编译器就告诉你。

## 7. 设计取舍

| 决策 | 选了什么 | 放弃什么、代价 |
|---|---|---|
| 闭包消费者的能力 | `FnMut` | 更宽的 `Fn`（可并发重入）会拒掉所有带状态的消费者；更窄的 `FnOnce` 无法连续发射。 |
| 终止方法的接收者 | `self`（消耗） | `&mut self` + 内部 `terminated` 标志：能写出"终止后又发"的程序，只能靠运行时兜。 |
| `subscribe(closure)` 的语义 | 明确只服务 `Err = Infallible` | 一旦可能失败就必须改用 `subscribe_with` / `on_error` / `into_future`；代价是"看起来最顺手的写法"有边界，必须写进文档（rxrust 就是这么做的）。 |
| 循环里终止谁 | 循环外唯一出口 | 在循环里 `sink.error(..)` 会撞上 §6④，等于让编译器当审计。 |

## 8. 反例（本项目/常见误区）

1. **把错误折成"退回原样"**：`expr.eval(env).ok().unwrap_or_else(|| raw_text)` ——
   等价于给错误写了个空 `error()`。失败必须可观察：要么进 in-stream `Result`，要么进 Err 通道。
2. **用 `Rc<RefCell<Option<O>>>` 当"随便谁都能塞"的总线**：那是共享可变；
   框架给多播的答案是 `Subject`（V5-05 手写）。
3. **为了绕过 §6③ 把 `Observer` 改成全部 `&mut self`**：会丢掉"终止后不可能再发"这条免费保证，
   正解是镜像 trait（`DynObserver`）。

## 9. 练习（先做，再看答案）

1. 写一个**自己拥有日志**的消费者 `Collecting`，实现四个方法；它需要 `mut self` 吗？和 `Echo` 有何不同？
2. 写一个 `emit_polite`：消费者 `closed()` 之后连终止信号都不发。断言 `Limited` 的日志是
   `["next 1", "next 2"]`（没有 `complete`）。
3. 用 `Box<dyn Observer<i32, &'static str>>` 调 `b.error(e)`，抄下 §6③ 的原文；
   然后自己写 `DynObserver` 镜像 trait + blanket impl，让它跑通。
4. 把 `once_counter()` 的返回类型改成 `impl Fn() -> usize`，看编译器在**定义处**说什么。

### 参考答案

见 `code/00-03-closures-and-sinks.answers.rs`（同目录，已在验证 crate 里编译通过）：

- 练习 1：`Collecting`（拥有 `Vec<String>`）⇒ `fn error(mut self, …)` 必需；
  `Echo`/`Recorder` 透过 `&'a mut Vec` 字段写入 ⇒ 不需要 `mut self`。
- 练习 2：拆出"安全前缀 + 首个错误"，再用一个 `match (stopped, failure)` 三分支收尾；
  `(true, _) => {}` 这一支就是"关门后什么都不送"。
- 练习 3：`trait DynObserver { fn box_error(self: Box<Self>, err: Err); … }` +
  `impl<T: Observer<Item, Err> + 'static> DynObserver<Item, Err> for T`，四个方法逐一转发。
- 练习 4：定义处直接报"闭包 `Fn` 要求不能改捕获"，因为闭包体里 `n += 1`。

## 10. 现在你应该能做到

- 说出 `next` 为什么是 `FnMut` 级、`error`/`complete` 为什么是 `FnOnce` 级；
- 看到任何"只实现了值的接收、没有失败出口"的消费者，立刻判断错误正在被丢；
- 遇到 `dyn` 调不动消耗式方法时，知道该造镜像 trait，而不是把协议降级成 `&mut self`。

**下一课 V0-04**：`Rc`/`Arc`/`RefCell`/`Mutex` 与内部可变性 —— 为什么 rxrust 的 `Context` 要把
`MutRc`/`MutArc` 抽成一个 trait，以及 object-safety 在真实算子里怎么被绕开。
