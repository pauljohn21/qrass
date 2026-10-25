# V0-02 trait、关联类型，一直到 GAT —— `type Item<'a>` 到底非用不可吗

> 零基础前置卷第 2 课。上一课我们让值"被搬走"；这一课要解决的是：**流上的元素类型，
> 怎样由实现者决定、又允许它自己带生命周期**。这三步（泛型参数 → 关联类型 → GAT）
> 在课文里是**依次被编译器逼出来**的，不是我先给结论再举例。
>
> 配套代码已在 workspace 外真编译 + 真跑：`code/00-02-traits-to-gats.rs`（4 条断言全过、
> `clippy --all-targets -D warnings` 干净）。§6 的每条报错都是现场复现的原文。

---

## 1. 本课要解决的问题（不解决会怎样）

我们要给"能产出元素的类型"起名。三个候选写法，差别巨大：

- **A 泛型参数**：`trait SourceG<Item>`
- **B 关联类型**：`trait Source { type Item; }`
- **C 带生命周期的关联类型（GAT）**：`trait Windowed { type Row<'a>; }`

rxrust 用的是 C：

```rust
// observable.rs:103-108
pub trait ObservableType {
  type Item<'a>
  where
    Self: 'a;
  type Err;
}
```

如果不理解"为什么非要 C"，读者看到 `Self::Item<'a>` 只会当成语法噪音，写算子时就会退回
"我把元素先复制成 owned 再用"——而按值复制恰恰是我们在 V0-01 里花一整课论证要尽量避免的东西。
本课就把这三级台阶踩一遍。

## 2. 你先试（先卡住，再看讲解）

1. 用 **A** 写一个"每次产出一个 `i32`，产出 3 次后结束"的来源，再写一个泛型函数把它抽空成 `Vec<i32>`。
2. 现在改成 **B**（`type Item`），让**同一个**泛型函数既能抽空 `i32` 来源、又能抽空 `String` 来源。
3. 再难一点：做一个"从 `Vec<i32>` 里借一段切片给你看"的来源——它的元素类型是 `&i32`。
   **这个 `&` 的生命周期写在哪？** 先自己撞 10 分钟，撞出来的就是本课。

## 3. 本课代码（请手敲；敲完再往下读）

```rust
/// 方案 A：元素类型做成 trait 的泛型参数。
pub trait SourceG<Item> {
    fn take(&mut self) -> Option<Item>;
}

/// 方案 B：关联类型。
pub trait Source {
    type Item;
    fn take(&mut self) -> Option<Self::Item>;
}

/// 按值产出的来源：有限计数器。
pub struct Ints {
    next: i32,
    limit: i32,
}

impl Ints {
    pub fn new(limit: i32) -> Ints {
        Ints { next: 0, limit }
    }
}

impl Source for Ints {
    type Item = i32;

    fn take(&mut self) -> Option<i32> {
        match self.next < self.limit {
            true => {
                self.next += 1;
                Some(self.next)
            }
            false => None, // 收尾：告诉消费者“没有更多了”
        }
    }
}

/// 另一个按值来源：字符串队列（`pop`，所以顺序是反的）。
pub struct Words {
    queue: Vec<String>,
}

impl Words {
    pub fn new(queue: Vec<String>) -> Words {
        Words { queue }
    }
}

impl Source for Words {
    type Item = String;

    fn take(&mut self) -> Option<String> {
        self.queue.pop()
    }
}

/// 对所有 `Source` 都成立的消费者：一直取到 `None`。
pub fn drain<S: Source>(src: &mut S, sink: &mut Vec<S::Item>) {
    while let Some(value) = src.take() {
        sink.push(value);
    }
}

/// 方案 C：GAT——元素类型自己也带生命周期。
pub trait Windowed {
    type Row<'a>: Iterator<Item = &'a i32> where Self: 'a;
    fn window(&self, at: usize, len: usize) -> Self::Row<'_>;
}

pub struct Series {
    values: Vec<i32>,
}

impl Series {
    pub fn new(values: Vec<i32>) -> Series {
        Series { values }
    }
}

impl Windowed for Series {
    type Row<'a> = std::slice::Iter<'a, i32> where Self: 'a;

    fn window(&self, at: usize, len: usize) -> Self::Row<'_> {
        self.values[at..at + len].iter()
    }
}

/// 泛型函数直接用 GAT：调用点不写任何生命周期。
pub fn window_total<S: Windowed>(s: &S, at: usize, len: usize) -> i32 {
    s.window(at, len).sum()
}

/// rxrust 算子的最小骨架：trait + 泛型闭包。
pub fn map_window<S: Windowed>(s: &S, at: usize, len: usize, f: impl FnMut(&i32)) {
    s.window(at, len).for_each(f);
}
```

## 4. 逐行讲解（每一行都讲）

### 方案 A：`trait SourceG<Item>`

| 行 | 讲什么 |
|---|---|
| `pub trait SourceG<Item> {` | `Item` 是**输入**给 trait 的参数：同一个类型可以为不同的 `Item` 各实现一次。听着灵活，代价在下面三行。 |
| `fn take(&mut self) -> Option<Item>;` | 每次要么给一个元素、要么给 `None`。`Option` 就是"结束"的契约——别小看它，第三节的 `drain` 全靠它停机。 |

代价一：使用点要到处带 `Item`。测试里那个 `fn first_of_three<S: SourceG<i32>>(src: &mut S) -> Vec<i32>` 就把 `i32` 焊死在签名里；换元素类型就得再写一个函数。
代价二：`impl SourceG<String> for X` 与 `impl SourceG<i32> for X` 虽然合法，但**函数签名必须替调用方选定一个**——"泛型函数对任何来源都成立"这个手感就没了。
代价三：算子嵌套时类型会写成 `Map<Filter<SourceG<i32>...>>` 这种链，每一层都要重述元素类型。

### 方案 B：关联类型

| 行 | 讲什么 |
|---|---|
| `pub trait Source {` | 没有尖括号。 |
| `type Item;` | 元素类型由**实现者**声明，是 trait 的成员。这是"一个实现只配一种元素"的约定，也正是我们想要的。 |
| `fn take(&mut self) -> Option<Self::Item>;` | 在 trait 内引用自身关联类型要写 `Self::Item`。 |
| `impl Source for Ints { type Item = i32;` | 实现处把抽象的 `Item` 定成具体的 `i32`。 |
| `match self.next < self.limit { true => {...} false => None }` | 用 `match` 在两个分支里分别处理"还有"和"结束"，避免 `if`+提前 `return` 链。注意 `false` 分支返回 `None` 是让 `drain` 能停下来的关键。 |
| `pub fn drain<S: Source>(src: &mut S, sink: &mut Vec<S::Item>) {` | 一个函数适配所有来源：`S::Item` 跟着实现者走，签名里不用出现任何具体元素类型。这就是 B 优于 A 的地方。 |
| `while let Some(value) = src.take() { sink.push(value); }` | "取到 `None` 为止"。`value` 是被搬进来的 owned 值（V0-01 的结论在这里兑现）。 |

### 方案 C：GAT

| 行 | 讲什么 |
|---|---|
| `pub trait Windowed {` | 新的抽象：来源**不交出拥有权**，而是每次"借一段自己内部的数据"。 |
| `type Row<'a>: Iterator<Item = &'a i32> where Self: 'a;` | 三个信息挤在一行：① `Row` 是关联类型但**自己带参数** `'a`；② 它必须实现 `Iterator` 且元素是 `&'a i32`（借用）；③ `where Self: 'a` 声明“只有当实现者至少活到 `'a` 这么久，`Row<'a>` 才存在”。删掉**声明侧**的 ③，编译器直接拒收（§6③）；`impl` 里那份重复的则是可省（§6③ 注）。 |
| `fn window(&self, at: usize, len: usize) -> Self::Row<'_>;` | `'_` 让编译器把返回元素的寿命绑到这次 `&self` 借用上：借来的窗口不可能比被看的对象活得久。调用点完全不用写生命周期。 |
| `impl Windowed for Series { type Row<'a> = std::slice::Iter<'a, i32> where Self: 'a;` | 实现者选定具体类型：借用切片迭代器。它的元素正是 `&'a i32`，与 trait 的约束对齐。 |
| `fn window(&self, at: usize, len: usize) -> Self::Row<'_> { self.values[at..at + len].iter() }` | 返回借用：`iter()` 借 `self.values`，长度由参数决定。没有任何克隆，也没有把整段数据复制出去。 |
| `pub fn window_total<S: Windowed>(s: &S, at: usize, len: usize) -> i32 { s.window(at, len).sum() }` | 泛型函数直接用 GAT：`sum()` 能从 `&i32` 元素累加出 `i32`（标准库有 `Sum<&i32>` 实现）。 |
| `pub fn map_window<S: Windowed>(s: &S, at: usize, len: usize, f: impl FnMut(&i32)) { s.window(at, len).for_each(f); }` | 这一行就是 rxrust 算子的形状：拿到一个"借来的元素流"，交给调用方给的 `FnMut`。两个小坑：`for_each(|v| f(v))` 会被 clippy 判**冗余闭包**（§6④）；`mut f` 也被判多余——`for_each` 接管闭包后自己让它可变（§6⑤）。 |

## 5. 对照 rxrust 真实源码

| 本课 | rxrust rc.6 | 为什么要这样 |
|---|---|---|
| `type Row<'a> ... where Self: 'a` | `type Item<'a> where Self: 'a`（`observable.rs:104-106`） | 元素可能是 owned（`from_iter` 给 `Iter::Item`），也可能是借用（`Observer<&'m mut T>` 一族）。同一个 trait 必须能表达"元素类型随使用者给的生命周期而变"。 |
| `type Err;`（我们的 trait 不需要） | `type Err;`（`observable.rs:107`，不带 `'a`） | 错误类型永远是 owned 值，不引用自他，所以**普通关联类型**就够——同一个 trait 里两种成员并存，正是"按需要选最简表达"。 |
| `map_window(.., f: impl FnMut(&i32))` | `fn map<F, Out>(self, f: F) where F: for<'a> FnMut(Self::Item<'a>) -> Out`（`observable.rs:170-175`） | `for<'a>` 是 HRTB："对**任意**生命周期都成立"。我们的版本因为元素类型已知（`&i32`）不需要写它；rxrust 因为 `Item<'a>` 随 `'a` 变，必须量化。 |
| 未做（V3 见） | `Observer<&'m mut T, Err>` 一族（`observer.rs:175-179, 212-227`） | 这就是 `Item<'a> = &'a mut T` 的真实用户：把可变借用当元素交出，避免逐元素克隆。 |
| `drain` 的 `while let Some` | `from_iter.rs:64-74` 的 `for item in self.iter { if observer.is_closed() { break; } … }` | 停机条件一个来自"没数据了"（`None`），一个来自"对方不要了"（`is_closed`）。两条都有才完整：前者防死循环，后者防白做。 |

顺带一提：`observable.rs:177-180` 有一条注释解释他们为什么保留 nightly 的 `FnMut<(...)>` 路径而不是自造 callable trait——因为自造抽象会在 `subscribe` 更下游破坏闭包类型推断。这类"为什么不更优雅"的取舍，V4 会正面讲。

## 6. 你会撞到的编译错（每条都是现场复现的原文）

### ① `E0119: conflicting implementations` —— 一个类型对同一 trait 只能实现一次

复现：给 `Ints` 再写一遍 `impl Source for Ints { type Item = String; … }`。

```
error[E0119]: conflicting implementations of trait `Source` for type `Ints`
   |
 34 | impl Source for Ints {
    | -------------------- first implementation here
123 | impl Source for Ints { type Item = String; fn take(&mut self) -> Option<String> { None } }
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ conflicting implementation for `Ints`
```

这正是 **A 与 B 的分水岭**：泛型参数版能实现多次（`SourceG<i32>` 与 `SourceG<String>` 并存），
关联类型版不能。对"一个来源就一种元素"的现实建模，B 更强——也更省事。

### ② `missing lifetime in associated type` —— B 走不通的那一步

复现：`impl Source for Lines { type Item = &str; ... }`（想产出借用）。

```
error: missing lifetime in associated type
124 | impl Source for Lines { type Item = &str; fn take(&mut self) -> Option<&str> { None } }
    |     -                               ^ this lifetime must come from the implemented type
    = note: in the trait the associated type is declared without lifetime parameters, so using a borrowed type for them requires that lifetime to come from the implemented type
```

注意报错怎么说的：**"这个生命周期必须来自被实现的类型"**。也就是说除非把 `Lines<'a>` 整个类型都带上寿命，否则 `Item` 借不到东西；而我们真正想要的是"**每次使用**自己挑寿命"（同一个 `Series` 现在借给这个调用、等一下借给那个）。于是只有两条路：把 `'a` 焊进 `Self`（灵活性大减），或者让**关联类型自己带参数**——GAT。这一条错误就是 C 存在的理由。

### ③ `missing required bound on Row` —— GAT 的 `where Self: 'a` 不是装饰

复现：把 `type Row<'a>: Iterator<Item = &'a i32> where Self: 'a;` 里的 `where Self: 'a` 删掉。

```
error: missing required bound on `Row`
  85 |     type Row<'a>: Iterator<Item = &'a i32>;
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^-
     |                                           |
     |                                           help: add the required where clause: `where Self: 'a`
     = note: this bound is currently required to ensure that impls have maximum flexibility
```

含义：`Row<'a>` 里出现了借用，而借用指向 `self` 的数据；编译器要求显式声明"`Self` 至少和 `'a` 一样长寿"。少写它，实现方就可能在 `Self` 短命时造出悬垂借用。`observable.rs:104-106` 那个看似累赘的 `where Self: 'a`，作用一模一样。

> 实测细节（同样重要）：必需的约束在 **trait 声明侧**。把 `impl` 里那份重复的 `where Self: 'a`
> 删掉仍编译通过（exit=0）；删掉声明侧那份则立刻是上面这个错。impl 处重写一次的意义是
> "实现者自证满足约束"，不是语法必需 —— 别把它当噪声随手删，也别以为删它一定报错。

### ④ `redundant closure`（clippy）

`for_each(|v| f(v))` → clippy：`help: use a reference to the function directly`。改成 `for_each(f)`。

### ⑤ `variable does not need to be mutable`（rustc）

`f: impl FnMut(&i32)` 上的 `mut` 是多余的：`for_each` 拿走所有权后自己把它当可变用。删掉 `mut`。

> ④⑤ 是我写这课时的**第一版真实状态**：`clippy --all-targets -D warnings` 当场拦下。教训是
> "写完立刻编译"，别把"我以为需要 mut"留在代码里。

## 7. 设计取舍

| 决策 | 选了什么 | 放弃什么、为什么 |
|---|---|---|
| A/B/C 三选一 | C | A 要在每个签名重述元素类型，且实现多份时函数无法泛化；B 表达不了"借用式元素"（②）。 |
| `Err` 不带 `'a` | `type Err;` | 错误值是 owned，不需要随使用者变寿命；带上参数只会增加使用噪音。 |
| 用 HRTB `for<'a> FnMut(Self::Item<'a>)` 而不是自造 callable trait | HRTB | 源码注释（`observable.rs:177-180`）说自造抽象会在 `subscribe` 下游破坏闭包推断，代价在整个库上叠加。 |
| 窗口返回借用 | `Self::Row<'_>` | 返回 owned `Vec<i32>` 会每次复制整段；这正是 rxrust 拒绝"物化再遍历"的同一条理由（V1 起会反复看到）。 |

## 8. 反例（别这么绕）

1. **把 `'a` 焊进 Self 绕开 GAT**：`struct Lines<'a> { text: &'a str } impl<'a> Source for Lines<'a> { type Item = &'a str; }`。能编译，但元素寿命被"实例创建那一刻"锁死，`drain` 只能服务一种寿命——把使用者该做的决定提前替它做了。
2. **`type Item = &'static str`**：把生命周期写成 `'static` 来"满足"编译器，等于用不安全假设换掉真正的表达。
3. **给每个生命周期写一份 trait**（`SourceOwned` / `SourceBorrowed` 两套）：trait 爆炸，算子要写两遍——rxrust 用一层 GAT 换掉了这整片重复。

## 9. 练习（先做，再看答案）

1. 自己定一个实现 `Source` 的结构（例如从 `left: 5` 倒数），再给它加一个**消耗自身**的
   `fn count(mut self) -> usize`，验证 `== 5`。为什么 `take()` 必须能为 `None`？
2. 再写一个 `Windowed` 实现：`Matrix { rows: Vec<Vec<i32>> }`，`window` 返回某一行的切片迭代器。`Row<'a>` 该定成什么？
3. 把 `map_window` 改成不接收闭包，而是返回一个迭代器：`fn window_of<S: Windowed>(s: &S, at: usize, len: usize) -> S::Row<'_>`。签名里那个 `'_` 绑到谁？
4. 分别删掉 `type Row<'a>` 上 **trait 声明侧**和 **impl 侧**的 `where Self: 'a`，看编译器各说什么。

### 参考答案

```rust
// 1) 消耗自身的 count()：`take` 返回 None 之前一直数。
//    （若 trait 还要在泛型里被按值调用，才需要 `where Self: Sized`；
//     这里 `fn count(mut self)` 已要求大小已知，编译器会替你把关。）
pub struct Ticker { left: i32 }

impl Source for Ticker {
    type Item = i32;
    fn take(&mut self) -> Option<i32> {
        match self.left > 0 {
            true => { self.left -= 1; Some(self.left) }
            false => None,
        }
    }
}

impl Ticker {
    fn count(mut self) -> usize {
        let mut n = 0;
        while self.take().is_some() {
            n += 1;
        }
        n
    }
}
// Ticker { left: 5 }.count() == 5（依次产出 4,3,2,1,0 后返回 None）

// 2) 截断后的迭代器是另一种类型，`Row` 必须写成 `Take<slice::Iter<..>>`
//    （我第一次写的是 `std::slice::Iter<'a, i32>` 配 `.take(len)`，类型不匹配）：
impl Windowed for Matrix {
    type Row<'a> = std::iter::Take<std::slice::Iter<'a, i32>> where Self: 'a;
    fn window(&self, at: usize, len: usize) -> Self::Row<'_> {
        self.rows[at].iter().take(len)
    }
}

// 3) `'_` 绑到 `&'s self`：返回的迭代器寿命由调用方持有的那个借用决定，
//    所以 `s` 必须在结果被使用期间保持存活。
fn window_of<S: Windowed>(s: &S, at: usize, len: usize) -> S::Row<'_> {
    s.window(at, len)
}

// 4) 实测：删 trait 侧 → `missing required bound on Row`（并提示补回 `where Self: 'a`）；
//    删 impl 侧 → 仍编译通过。必需约束只在声明处，见 §6③ 的注。
```

## 10. 现在你应该能做到

- 看到 `type Item<'a> where Self: 'a;` 能立刻答出："元素可以是借用，寿命由使用者挑，`where` 保证 `Self` 撑得住那个借用。"
- 说清 A/B/C 各自的代价，并能用 ①②③ 三条报错证明它们。
- 读懂 V1 里 `ObservableType` / `CoreObservable` / `Observable` 三层的所有签名。

**下一课 V0-03**：`Fn / FnMut / FnOnce` —— 为什么 `Observer::next` 只能是 `FnMut` 语义，
以及 `subscribe(closure)` 为什么会把 `error`/`complete` 悄悄丢掉（`observer.rs:255-277` 的三行空实现）。
