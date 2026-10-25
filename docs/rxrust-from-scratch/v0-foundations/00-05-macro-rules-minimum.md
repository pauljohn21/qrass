# V0-05 `macro_rules!` 最小知识 —— 把 `impl_observer_for_box!` 展开成你认识的代码

> 零基础前置卷第 5 课（最后一课）。目标不是"会写宏"，而是**读得懂 rc.6 里那 7 处宏在干什么**，
> 并且知道宏的代价落在哪里。全程用一对自造的 `Sink`/`DynSink` 复刻
> `observer.rs:133-167` 的结构，三条调用逐条实测。
>
> 验证：`code/00-05-macros.rs` + `.tests.rs`（5 条断言）+ `.answers.rs`（手写展开，1 条断言），
> 全部在 `/tmp/minirx-check` 编译通过，`clippy --all-targets -D warnings` 0 警。
> §6 的 5 条报错都是现场复现的原文。

---

## 1. 本课要解决的问题（不解决会怎样）

V0-03 我们学到：`Box<dyn Observer<…>>` 调不动消耗式方法，得造 object-safe 镜像 `DynObserver`
（`observer.rs:110-125`）。但镜像只是第一步——还要让**任何盒子形态**都重新满足 `Observer`：

- `Box<dyn DynObserver<Item, Err> + 'a>`
- `Box<dyn DynObserver<Item, Err> + Send + 'a>`
- `Box<dyn for<'m> DynObserver<&'m mut T, Err> + 'a>`
- `Box<dyn for<'m> DynObserver<&'m mut T, Err> + Send + 'a>`

源码里正好就是这四种（`observer.rs:172-178`：两条 `regular` + 两条 `mut_ref`，各自只差一个
`Send`）。每个形态的 `impl` 骨架**一模一样**（四个方法各转发一次），只有泛型参数列表和元素类型不同。
手写四遍的后果不是啰嗦，是**改一处忘三处**：`next` 的转发逻辑变了，你漏改一个形态，
那个形态的订阅就静默不转发——正是 V0-03 讲过的"信号丢失"。

所以 rxrust 用 `macro_rules!`。全库只有 7 处（实测 `grep -rn "macro_rules!" src`）：

| 位置 | 名字 | 复制的是什么骨架 |
|---|---|---|
| `observer.rs:133` | `impl_observer_for_box!` | `Observer` 的四个方法 × 盒子形态 |
| `subject_core.rs:411` | `impl_observer_for_subject!` | `Observer` × `Subject`/`BehaviorSubject` |
| `subject_core.rs:512` | `impl_core_observable_for_subject!` | `CoreObservable` 的订阅骨架 |
| `context.rs:248` | `impl_context_for_container!` | `Context` × `Rc`/`Arc` 族容器 |
| `context.rs:407` | `impl_observer_and_subscription!` | `Observer` + `Unsubscribable` × `RefCell`/`Mutex` 族 |
| `ops/average.rs:37` | `impl_averageable_int!` | 一个 trait × 各整数类型 |
| `scheduler.rs:645` | `impl_scheduler!` | （在 `#[cfg(test)]` 内）测试调度器骨架 |

**规律**：宏只出现在"同一份 impl 骨架 × N 种类型组合"。没有一处宏用于控制流或业务分支——
这条纪律也是 `rust.md` 的意思：宏不能当第二套控制流。

## 2. 你先试（先卡住，再看讲解）

1. 写一个宏 `sum_all!`，接受**任意个** `expr`，展开成把它们相加的代码。
2. 写一个宏，接收"一个类型 + 一个生命周期"，生成 `impl<'a> Trait for 那个类型`。
3. 故意把宏定义放在调用**之后**，看编译器说什么。

第 2 题你会立刻撞上"泛型参数列表怎么当参数传"——那正是 `observer.rs` 用
`generics = [$($generics:tt)*]` 解决的事。

## 3. 本课代码（关键部分；全文见 `code/00-05-macros.rs`）

```rust
/// 协议（对照 `Observer`）。
pub trait Sink<Item> {
    fn receive(&mut self, value: Item);
    fn seen(&self) -> usize;
}

/// object-safe 镜像（对照 `DynObserver`）。
pub trait DynSink<Item> {
    fn box_receive(&mut self, value: Item);
    fn box_seen(&self) -> usize;
}

/// blanket impl（对照 `observer.rs:117-125`）。
impl<Item, T: Sink<Item> + ?Sized> DynSink<Item> for T {
    fn box_receive(&mut self, value: Item) { self.receive(value); }
    fn box_seen(&self) -> usize { self.seen() }
}

macro_rules! impl_sink_for_box {
    // 内部规则：唯一的骨架。
    (@impl $ty:ty, generics = [$($generics:tt)*], item_type = $item:ty) => {
        #[allow(coherence_leak_check)]
        impl<$($generics)*> Sink<$item> for $ty {
            fn receive(&mut self, value: $item) { (**self).box_receive(value); }
            fn seen(&self) -> usize { (**self).box_seen() }
        }
    };

    // 公开规则一：`Item` 是类型参数（对照 `observer.rs:159-161`）。
    (regular: $ty:ty, lifetime = $lf:lifetime) => {
        impl_sink_for_box!(@impl $ty, generics = [$lf, Item], item_type = Item);
    };

    // 公开规则二：元素是 `&mut T`（对照 `observer.rs:164-166`）。
    (mut_ref: $ty:ty, lifetime = $lf:lifetime) => {
        impl_sink_for_box!(@impl $ty, generics = [$lf, T], item_type = &mut T);
    };
}

// 三条调用（对照 `observer.rs:169-171`）。
impl_sink_for_box!(regular: Box<dyn DynSink<Item> + 'a>, lifetime = 'a);
impl_sink_for_box!(regular: Box<dyn DynSink<Item> + Send + 'a>, lifetime = 'a);
impl_sink_for_box!(mut_ref: Box<dyn for<'m> DynSink<&'m mut T> + 'a>, lifetime = 'a);

macro_rules! sum_all {
    ($($v:expr),* $(,)?) => {{
        let mut acc = 0;
        $(acc += $v;)*
        acc
    }};
}

/// 卫生性：宏里的 `hits` 与调用方的 `hits` 是两个名字。
macro_rules! counted {
    ($value:expr) => {{
        let hits = 1;
        $value + hits
    }};
}

pub fn hygiene_probe() -> (i32, usize) {
    let hits = 100;
    let via_macro = counted!(5);
    (via_macro, hits)
}

pub fn drain_boxed<'a>(mut boxed: Box<dyn DynSink<i32> + 'a>, values: &[i32]) -> usize {
    for value in values {
        boxed.receive(*value);
    }
    boxed.seen()
}

pub fn drain_mut_ref<'a>(mut boxed: Box<dyn for<'m> DynSink<&'m mut i32> + 'a>, value: &mut i32) {
    boxed.receive(value);
}
```

## 4. 逐行讲解（每一行都讲）

### `macro_rules!` 的形状

| 行 | 讲什么 |
|---|---|
| `macro_rules! impl_sink_for_box {` | 定义一个"模式 → 生成代码"的规则集。它是**语法级**的：匹配的是令牌（token），不是值。 |
| `(@impl $ty:ty, generics = [$($generics:tt)*], item_type = $item:ty) => {` | 一条规则 = 左侧 matcher + `=>` + 右侧模板。`@impl` 是**惯例**：以 `@` 开头的规则是内部入口，不给外面调用（编译器会参与匹配，见 §6① 的 `note: while trying to match '@'`）。 |
| `$ty:ty` | 片段分类符（fragment specifier）`ty` = 一个类型。可选的还有 `expr`/`ident`/`lifetime`/`tt`/`path`/`pat`/`block`/`stmt`。 |
| `$($generics:tt)*` | 重复：`$( ... )*` 匹配零次或多次，里面写"要重复的模式"。`tt` = 单个**令牌树**（一个标识符、一个生命周期、一个 `+`…）。把泛型列表当令牌流搬，是让同一骨架服务不同参数集的关键。 |
| `$item:ty` | 决定生成的 `impl` 里 `Sink<…>` 的角标。 |
| `impl<$($generics)*> Sink<$item> for $ty {` | 模板侧：`$($generics)*` 把令牌流原样铺开成参数列表。**注意没有逗号**——逗号是调用侧 `[$lf, Item]` 里带进来的，`tt` 连逗号一起吃掉了。这就是为什么 matcher 要用 `tt` 而不是 `ident`/`lifetime`。 |
| `fn receive(&mut self, value: $item) { (**self).box_receive(value); }` | `**self`：`Box<dyn …>` 去两层引用（盒子 → trait object → 内容），拿到 `&mut dyn DynSink<…>` 才能调 `box_receive`。 |
| `#[allow(coherence_leak_check)]` | 见 §6⑤：不加就警告。`regular`（对所有 `Item`）与 `mut_ref`（对 `&mut T`）在 `Item = &mut T` 时重叠。这一行的存在是有原因的，不是装饰。 |
| `(regular: $ty:ty, lifetime = $lf:lifetime) => { impl_sink_for_box!(@impl …); };` | 公开规则：**递归**调用同一个宏、走内部规则。所以"骨架只有一份"，公开规则只负责"怎么把参数打包给骨架"。 |
| `generics = [$lf, Item], item_type = Item` | 生成 `impl<'a, Item> Sink<Item> for …`。 |
| `generics = [$lf, T], item_type = &mut T` | 生成 `impl<'a, T> Sink<&mut T> for …`。两条公开规则的差别**只有这两处**，骨架一字未动。 |
| `impl_sink_for_box!(regular: Box<dyn DynSink<Item> + 'a>, lifetime = 'a);` | 调用点传的 `$ty` 里 `Item` 是**类型参数**而不是具体类型——这是我第一版写错的地方：我写成 `DynSink<i32>`，于是生成的 `impl` 想对所有 `Item` 实现却只支持 `i32`，`E0308` 当场指出（§6④）。 |

### `sum_all!`：重复与分隔符的最小样本

| 行 | 讲什么 |
|---|---|
| `($($v:expr),* $(,)?) => {{` | `$(...),*` 里那个逗号是**字面分隔符**（写在括号外）；`$(,)?` 单独一条重复，意思是"可选的一个尾逗号"。 |
| `let mut acc = 0;` | 模板里的名字是宏自己的，不会与调用方冲突（下一段讲）。 |
| `$(acc += $v;)*` | 重复展开：**每个**匹配到的 `$v` 生成一条 `acc += …;`。 |
| `acc }};` | 双花括号 `{{ … }}` 是模板整体（一个 block 表达式）加外层规则定界。 |

### 卫生性（hygiene）：宏不是文本替换

| 行 | 讲什么 |
|---|---|
| `macro_rules! counted { ($value:expr) => {{ let hits = 1; $value + hits }}; }` | 宏里定义了 `hits`。 |
| `pub fn hygiene_probe() -> (i32, usize) { let hits = 100; let via_macro = counted!(5); (via_macro, hits) }` | 断言是 `(6, 100)`：宏里的 `hits` 没看见外面的 100，也没写坏它。若是 C 预处理器那种文本替换，结果会是 `105`。 |

这条保证很重要：V1 起你会看到算子内部大量用宏生成方法体，如果宏会捕获名字，
我们就没法在宏附近安全地用同名局部变量。

## 5. 对照 rxrust 真实源码（`observer.rs:133-169`）

源码的骨架（节选；令牌逐字，行版式为了凑进一个屏幕有压缩）：

```rust
macro_rules! impl_observer_for_box {
  (@impl $ty:ty, generics = [$($generics:tt)*], item_type = $item:ty) => {
    #[allow(coherence_leak_check)]
    impl<$($generics)*> Observer<$item, Err> for $ty {
      #[inline] fn next(&mut self, value: $item) { (**self).box_next(value) }
      #[inline] fn error(self, err: Err) { self.box_error(err) }
      #[inline] fn complete(self) { self.box_complete() }
      #[inline] fn is_closed(&self) -> bool { (**self).box_is_closed() }
    }
  };
  (regular: $ty:ty, lifetime = $lf:lifetime) => {
    impl_observer_for_box!(@impl $ty, generics = [$lf, Item, Err], item_type = Item);
  };
  (mut_ref: $ty:ty, lifetime = $lf:lifetime) => {
    impl_observer_for_box!(@impl $ty, generics = [$lf, T, Err], item_type = &mut T);
  };
}
```

| 源码 | 本课 | 说明 |
|---|---|---|
| `Observer<$item, Err>` | `Sink<$item>` | 我只省了 `Err` 参数，结构一字不差。 |
| `generics = [$lf, Item, Err]`（`:162`） | `[$lf, Item]` | 源码多搬一个 `Err`，因为 `Observer` 有两个角标。 |
| `item_type = &mut T`（`:167`） | 同 | `mut_ref` 一族：元素是可变借用（`Observer<&'m mut T, Err>`）。 |
| `(**self).box_next(value)`（`:139`） | `(**self).box_receive(value)` | 同构的两层解引用。 |
| `#[inline]`（`:138`/`143`/`148`/`153`） | 我没写 | 发射路径每元素一次调用，内联是必要的；V1 会正面讨论。 |
| `#[allow(coherence_leak_check)]`（`:136`） | 同（§6⑤ 是它存在的实测证据） | 重叠来自 `Item` 与 `&mut T` 两条规则。 |
| 四次调用（`:172-178`） | 三次调用 | 源码把 `Send` 也配给了 `mut_ref`；加一种形态 = 再加一次调用，骨架不动。 |
| `self.box_error(err)`（`error(self, …)`） | — | `Box<dyn …>` 能调消耗式方法，因为镜像 trait 声明的是 `self: Box<Self>`（V0-03 §6③）。 |

## 6. 你会撞到的编译错（5 条，全部现场复现）

### ① `no rules expected 'weird'` —— 公开规则名写错

复现：把第二条调用改成 `impl_sink_for_box!(weird: …)`。

```
error: no rules expected `weird`
  --> src/lib.rs:96:20
62 | macro_rules! impl_sink_for_box {
   | ------------------------------ when calling this macro
96 | impl_sink_for_box!(weird: Box<dyn DynSink<Item> + Send + 'a>, lifetime = ...
   |                    ^^^^^ no rules expected this token in macro call
note: while trying to match `@`
```

两个教学点：**(a)** 报错位置在**宏定义那一行**（62）+ 调用行，而不是你以为的地方；
**(b)** `note: while trying to match '@'` 说它在按规则顺序试匹配——规则是**有序的**，
写错首令牌会一路试到底。

### ② `no rules expected ','` —— `tt` 流写成单个 `tt`

复现：把 matcher 的 `[$($generics:tt)*]` 改成 `[$generics:tt]`。

```
error: no rules expected `,`
83 |         impl_sink_for_box!(@impl $ty, generics = [$lf, Item], item_type = ...
   |                                                      ^ no rules expected this token in macro call
```

单 `tt` 只吃一个令牌（`'a`），后面那个逗号没人接。**`tt` 流的意义**就在这里：
它连逗号一起吃进来，展开时再原样吐出。

### ③ `cannot find macro 'later' in this scope` —— 定义顺序

复现：调用在前、定义在后。

```
error: cannot find macro `later` in this scope
2 |     later!(1, 2)
  |     ^^^^^ consider moving the definition of `later` before this call
note: a macro with the same name exists, but it appears later
```

`macro_rules!` 是**按文本顺序**可见的（不像函数/类型那样整个模块互通）。这就是为什么
rxrust 把宏定义放在调用**之前**（`observer.rs:133` 定义、`:169` 起调用），
也是为什么跨模块用的宏要 `#[macro_use]` 或 `pub(crate) use`。

### ④ `E0308` —— 盒子写死具体类型、`item_type` 却是泛型参数

我的第一版真实错误：调用写 `Box<dyn DynSink<i32> + 'a>` 而规则传 `item_type = Item`。

```
error[E0308]: mismatched types
68 |                 (**self).box_receive(value);
   |                          ----------- ^^^^^ expected `i32`, found type parameter `Item`
   = note: this error originates in the macro `impl_sink_for_box`
```

最后一行 `this error originates in the macro …` 是宏的第二大代价：**错误在宏体里**，
你得自己反推是哪次调用造成的。源码那四条调用的盒子都是泛型 `DynObserver<Item, Err>`
（`observer.rs:172-178`），正是为了避开这个坑。

### ⑤ `warning: conflicting implementations … Box<dyn DynSink<&mut _>>`

不加 `#[allow(coherence_leak_check)]` 时编译器给的原话：

```
warning: conflicting implementations of trait `Sink<&mut _>` for type `Box<dyn DynSink<&mut _>>`
66 |         impl<$($generics)*> Sink<$item> for $ty {
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |         first implementation here
   |         conflicting implementation for `Box<dyn DynSink<&mut _>>`
```

**这条把 `observer.rs:137` 那个属性解释清楚了**（我上一课把它推给"V1 再核对"，这课自己量出来了）：
`regular` 对所有 `Item` 实现，`mut_ref` 对 `&mut T` 实现，两者在 `Item = &mut T` 时是否重叠
取决于 Rust 的一致性检查松紧（leak check），源码选择显式 `allow`。

顺带两条同源实测：

- 手写一遍**同一个** impl（不是另一对 trait）→ `E0119: conflicting implementations of trait Sink<i32> for Box<dyn DynSink<i32>>`，
  并把冲突指回"在那个宏调用中生成"（`...in this macro invocation`）。所以答案文件必须另起一对 trait。
- 把 `Box` 换成 `Arc` → `E0596: cannot borrow data in an Arc as mutable`：
  `Arc` 给不了 `&mut`，所以这种"独占所有权 + 转发"的骨架只能在 `Box` 上做；
  要共享就回到 V0-04 的 `Arc<Mutex<_>>`（`context.rs` 的 `Shared` 侧正是这么干的）。

## 7. 设计取舍

| 决策 | 选了什么 | 代价与替代 |
|---|---|---|
| N 种类型组合 × 同一骨架 | `macro_rules!` + 内部 `@impl` 规则 | 报错定位到宏体（§6①④）；替代是手写 N 遍，代价是"改一处忘三处"。 |
| 传泛型参数列表 | `tt` 流（`$($generics:tt)*`） | 失去结构检查，写错就 §6②；替代是每种组合单开一条完整规则（骨架会重复）。 |
| 内部规则用 `@` 前缀 | 惯例（无语言强制） | 没有真正的"私有宏"；靠首令牌隔离，误调用得到 §6①。 |
| 重叠的两条规则 | `#[allow(coherence_leak_check)]` | 承认重叠并显式记录，而不是重排规则来躲避。 |
| 宏用于控制流/业务分支 | 不用（全库 7 处都不是） | 那会造出第二套不可检查的流程；`rust.md` 的禁令与此同源。 |

## 8. 反例

1. **用宏写业务分支**（比如"根据 `@each` 的类型生成不同循环"）：那是把求值逻辑塞进展开期，
   调试时 tracing span 看不见、编译器不检查语义。框架里宏只做"同一骨架的类型组合"。
2. **以为宏里的名字会捕获外部变量**：§4 的 hygiene 断言 `(6, 100)` 否证了这一点；
   反过来也别依赖"外部名字能被宏看见"——`counted!(hits)` 不会读到调用方的 `hits`。
3. **为了少写代码而写宏**：两种形态的手写其实更清楚时（比如只差一个方法体），别上宏——
   本课前两条公开规则只差 `$lf, Item`/`$lf, T`，值得；若只差一个函数体，就不值得。

## 9. 练习（先做，再看答案）

1. 把三条调用的展开**手写**出来（提示：不能对同一对 trait 写，否则 §6 的 `E0119`）。
2. 加一种新组合（如多一个 `Sync` 约束）：要求骨架 `@impl` 一字不改。
3. 试着把 `Box` 换成 `Arc`，抄下 §6 那条 `E0596`，并解释它为什么必然发生。
4. 故意犯 §6② 的错（`tt` 流 → 单 `tt`），记下报错与它指向的行。

### 参考答案

见 `code/00-05-macros.answers.rs`（已编译通过）：

- 练习 1：另起一对 `SinkH`/`DynSinkH`，把三份 `impl` 逐字写出（每份都带
  `#[allow(coherence_leak_check)]`），断言 `seen()` 与借用式元素都能改到调用方变量。
- 练习 2：新增公开规则 `(sync: …)` + 一次调用，骨架未动。
- 练习 3：`Arc` 只能给出 `&T`，而镜像方法是 `box_receive(&mut self, …)` ⇒ 必然 `E0596`；
  共享形态必须 `Arc<Mutex<_>>`（V0-04）。
- 练习 4：`no rules expected ','`，位置指向宏内部的递归调用行。

## 10. 现在你应该能做到

- 看到 `macro_rules!` 能立刻定位"哪条是骨架、哪条是类型组合入口"，并手抄出它的展开；
- 知道宏的两大代价（报错定位、按文本顺序可见），因而不会在调试时误判现场；
- 理解 `observer.rs:133` 与 `context.rs:248/407` 这些位置**为什么必须是宏**——
  因为 rxrust 的 `Local`/`Shared` 两套线程模型（V3）就是同一骨架 × N 种容器形态。

---

## V0 卷末（前置卷到此完成）

| 课 | 你拿到的地基 | 兑现处 |
|---|---|---|
| V0-01 | 值被搬走、借用受寿命约束、消耗式方法 = 免费协议检查 | `next(&mut self, Item)` / `complete(self)` |
| V0-02 | 泛型参数 → 关联类型 → GAT 的推导链 | `type Item<'a> where Self: 'a` |
| V0-03 | `Fn/FnMut/FnOnce` 与三信号协议；闭包消费者吞错 | `subscribe` 的边界、`FnMutObserver` |
| V0-04 | 共享 × 可变的统一抽象必须用 GAT；`try_*` 与中毒策略 | `rc.rs`、`Context` |
| V0-05 | 宏只做"同一骨架 × N 种组合"；代价是报错定位 | `observer.rs:133`、`context.rs:248/407` |

自检三问（每课都回答过，合起来应能一口气答出）：
① 这行代码在防什么？② 不这样写会撞哪条报错？③ rxrust 里它的原文位置在哪？

**下一卷 V1**：`observer.rs` 全文逐行 + `from_iter`/`of`/`empty`/`never`/`throw_err`——
从这一课起，我们开始讲 rxrust 本体，而你现在已经认识它每一块地基的来处。
