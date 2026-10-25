# V0-04 共享所有权 × 内部可变性 —— rxrust 的 `MutRc`/`MutArc` 为什么必须存在

> 零基础前置卷第 4 课。前三课我们把"值被搬走""类型带寿命""能力分等级"讲完了，
> 还剩最后一块拼图：**一个算子链上，多个持有者要同时改同一个状态**（订阅句柄、
> 多播总线、取消标志）。这一课用 `Rc<RefCell<_>>` 与 `Arc<Mutex<_>>` 把这条路走通，
> 然后证明一件事：**能把两者统一的抽象，只能用 GAT 写。**
>
> 对照源码：`rc.rs:28-62`（`RcDeref` / `RcDerefMut` / `SharedCell`）、
> `rc.rs:69-101`（`MutRc`）、`rc.rs:143-175`（`MutArc`）、`rc.rs:106-131`（`CellRc`）、
> `rc.rs:178-204`（`CellArc`）。
> 配套代码已真编译真跑：**5 条断言 + 1 条答案断言全过、`clippy --all-targets -D warnings` 0 警**。
> §6 的三条报错都是现场复现的原文。

---

## 1. 本课要解决的问题（不解决会怎样）

V1 会写到这样的代码（rxrust 里到处都是）：

```rust
// 简化：一个算子要能取消，就要和外部共享一个"订阅状态"
let state = MutRc::from(SubscriptionState::new());
let handle = SubscriptionHandle { state: state.clone() };
```

共享状态有两个硬约束，互相拉扯：

1. **多处持有** ⇒ 需要引用计数（`Rc`/`Arc`）。
2. **还要能改** ⇒ 但 `Rc<T>` 的 `Deref` 只给 `&T`。多个持有者同时拿 `&mut T` 是 Rust 明令禁止的
   （别名 + 可变 = 数据竞争/悬垂）。

出路只有**内部可变性**：容器给你 `&self` 就能改内部，代价是"能不能改"从编译期挪到运行期检查。
而线程模型又分两档：单线程 `Rc<RefCell<T>>`、多线程 `Arc<Mutex<T>>`。

**如果每个算子都要自己写这两种分支，代码量翻倍且必然写错。** 所以需要一个抽象：
"我有一个共享的可变句子，能借出守卫"——由调用方选容器，由算子只写一遍。这就是 `rc.rs`。

## 2. 你先试（先卡住，再看讲解）

1. 只用 `Rc<RefCell<i32>>`，写一个函数：拿句子的可变借用，加一个数，返回新值。
2. 现在把同样的函数用于 `Arc<Mutex<i32>>`。**不许改函数体**——想想签名要写成什么样子才可能成立。
3. 试着给 `struct Wrap<T>(Rc<RefCell<T>>)` 加 `#[derive(Clone)]`，然后用一个**没有 `Clone`** 的 `T` 去克隆它。
   看编译器拦不拦（§6②）。

## 3. 本课代码（请手敲；敲完再往下读）

```rust
use std::cell::{Cell, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};

/// 只读的共享解引用（对照 `rc.rs:28-36`）。
pub trait RcDeref: Clone {
    type Target;
    type Ref<'a>: Deref<Target = Self::Target>
    where
        Self: 'a;

    fn rc_deref(&self) -> Self::Ref<'_>;
}

/// 可变的共享解引用（对照 `rc.rs:38-52`）。
pub trait RcDerefMut: RcDeref {
    type MutRef<'a>: DerefMut<Target = Self::Target>
    where
        Self: 'a;

    fn rc_deref_mut(&self) -> Self::MutRef<'_>;

    fn try_rc_deref_mut(&self) -> Option<Self::MutRef<'_>>;
}

/// 单线程容器：`Rc<RefCell<T>>`（对照 `rc.rs:69`）。
pub struct MutRc<T>(Rc<RefCell<T>>);

/// 手写 `Clone`：derive 会要求 `T: Clone`，而克隆指针只需要引用计数 +1
/// （`rc.rs:71-73` 同样是手写）。
impl<T> Clone for MutRc<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> From<T> for MutRc<T> {
    fn from(value: T) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }
}

impl<T> RcDeref for MutRc<T> {
    type Target = T;
    type Ref<'a> = std::cell::Ref<'a, T>
    where
        Self: 'a;

    fn rc_deref(&self) -> Self::Ref<'_> {
        self.0.borrow()
    }
}

impl<T> RcDerefMut for MutRc<T> {
    type MutRef<'a> = RefMut<'a, T>
    where
        Self: 'a;

    fn rc_deref_mut(&self) -> Self::MutRef<'_> {
        self.0.borrow_mut()
    }

    fn try_rc_deref_mut(&self) -> Option<Self::MutRef<'_>> {
        self.0.try_borrow_mut().ok()
    }
}

/// 多线程容器：`Arc<Mutex<T>>`（对照 `rc.rs:143`）。
pub struct MutArc<T>(Arc<Mutex<T>>);

impl<T> Clone for MutArc<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> From<T> for MutArc<T> {
    fn from(value: T) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }
}

impl<T> RcDeref for MutArc<T> {
    type Target = T;
    /// 同一个方法，守卫却是另一种类型：这正是"关联类型必须带 `'a`"的用武之地。
    type Ref<'a> = MutexGuard<'a, T>
    where
        Self: 'a;

    fn rc_deref(&self) -> Self::Ref<'_> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<T> RcDerefMut for MutArc<T> {
    type MutRef<'a> = MutexGuard<'a, T>
    where
        Self: 'a;

    fn rc_deref_mut(&self) -> Self::MutRef<'_> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn try_rc_deref_mut(&self) -> Option<Self::MutRef<'_>> {
        self.0.try_lock().ok()
    }
}

/// 一个对两种容器都成立的函数——整套抽象的全部目的。
pub fn bump<D: RcDerefMut<Target = i32>>(handle: &D, by: i32) -> i32 {
    *handle.rc_deref_mut() += by;
    *handle.rc_deref_mut()
}

pub fn peek<D: RcDeref<Target = i32>>(handle: &D) -> i32 {
    *handle.rc_deref()
}

/// Copy 类型的轻量通道（对照 `rc.rs:55-62`、`CellRc`、`CellArc`）：不需要守卫。
pub trait SharedCell<T: Copy + Eq>: Clone + From<T> {
    fn get(&self) -> T;
    fn set(&self, value: T);
    fn compare_exchange(&self, current: T, fresh: T) -> Result<T, T>;
}

pub struct CellRc<T>(Rc<Cell<T>>);

impl<T: Copy> Clone for CellRc<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Copy> From<T> for CellRc<T> {
    fn from(value: T) -> Self {
        Self(Rc::new(Cell::new(value)))
    }
}

impl<T: Copy + Eq> SharedCell<T> for CellRc<T> {
    fn get(&self) -> T {
        self.0.get()
    }

    fn set(&self, value: T) {
        self.0.set(value);
    }

    /// `Cell` 没有原子 CAS，只能"读—比—写"：单线程内不会被打断，跨线程就不成立。
    fn compare_exchange(&self, current: T, fresh: T) -> Result<T, T> {
        let old = self.0.get();
        match old == current {
            true => {
                self.0.set(fresh);
                Ok(old)
            }
            false => Err(old),
        }
    }
}

/// 多线程版。`rc.rs:178-204` 用泛型的 `AtomicCell<T>`；这里为零依赖只做一个具体宽度。
#[derive(Clone)]
pub struct CellArc(Arc<std::sync::atomic::AtomicI32>);

impl From<i32> for CellArc {
    fn from(value: i32) -> Self {
        Self(Arc::new(std::sync::atomic::AtomicI32::new(value)))
    }
}

impl SharedCell<i32> for CellArc {
    fn get(&self) -> i32 {
        self.0.load(Ordering::SeqCst)
    }

    fn set(&self, value: i32) {
        self.0.store(value, Ordering::SeqCst);
    }

    fn compare_exchange(&self, current: i32, fresh: i32) -> Result<i32, i32> {
        self.0
            .compare_exchange(current, fresh, Ordering::SeqCst, Ordering::SeqCst)
    }
}

/// 跨线程累加：只有 `Arc`/`Mutex` 这条路能编译（`Rc` 会撞上 `Send`，§6①）。
pub fn parallel_bump(handles: Vec<MutArc<i32>>, each: i32) -> usize {
    let workers = handles.into_iter().map(|handle| {
        std::thread::spawn(move || {
            for _ in 0..each {
                bump(&handle, 1);
            }
            handle
        })
    });
    workers.filter_map(|joined| joined.join().ok()).count()
}
```

## 4. 逐行讲解（每一行都讲）

### 两个 trait：统一抽象的关键就在 GAT

| 行 | 讲什么 |
|---|---|
| `pub trait RcDeref: Clone {` | 继承 `Clone` 是必须的：订阅链上要能把句柄交给多个下游，不能克隆的"共享句柄"没有意义。 |
| `type Target;` | 被守卫包住的内容类型。 |
| `type Ref<'a>: Deref<Target = Self::Target> where Self: 'a;` | 本课的心脏。守卫**类型随实现者变**（`Ref<'a,T>` 或 `MutexGuard<'a,T>`），守卫**寿命随使用者变**（这次借用多久）。V0-02 说的"使用者逐次选寿命"在这里兑现：普通关联类型 `type Ref;` 写不出这件事（§6④）。 |
| `fn rc_deref(&self) -> Self::Ref<'_>;` | 接收者是 `&self`——只读借用不需要"拥有可变性"。 |
| `pub trait RcDerefMut: RcDeref {` | 可变版是读版的超集：先能借看，才谈得上借改。 |
| `type MutRef<'a>: DerefMut<Target = Self::Target> where Self: 'a;` | 同理，但要 `DerefMut`。 |
| `fn rc_deref_mut(&self) -> Self::MutRef<'_>;` | **仍是 `&self`**。这就是内部可变性的定义：句子本身不可变，里面的数据可改。 |
| `fn try_rc_deref_mut(&self) -> Option<Self::MutRef<'_>>;` | `rc.rs:45-50` 的注释讲得很直白：用途是**检测同步重入**，让调用方改走调度器，而不是当场 panic 或死锁。没有 `try_`，一个 bug 就是一次崩溃。 |

### `MutRc`：`Rc<RefCell<T>>` 的薄包装

| 行 | 讲什么 |
|---|---|
| `pub struct MutRc<T>(Rc<RefCell<T>>);` | 新类型（tuple struct，字段私有）。为什么不用类型别名：①外面塞不进"裸 `Rc<RefCell<T>>`"，类型安全；②只能给自己的类型实现 trait（孤儿规则）。 |
| `impl<T> Clone for MutRc<T> { fn clone(&self) -> Self { Self(self.0.clone()) } }` | 手写而非 derive：`derive(Clone)` 会生成 `where T: Clone` 的要求（§6②实测拒绝），而引用计数 +1 根本不需要 `T` 能被克隆。 |
| `impl<T> From<T> for MutRc<T> { fn from(value: T) -> Self { Self(Rc::new(RefCell::new(value))) } }` | 统一的"从值造容器"入口；`rc.rs:75-77` 一模一样，配合泛型代码 `T::default().into()`（`rc.rs:79-81`）。 |
| `type Ref<'a> = std::cell::Ref<'a, T> where Self: 'a;` | 守卫类型填 `Ref`。 |
| `fn rc_deref(&self) -> Self::Ref<'_> { self.0.borrow() }` | `RefCell` 的运行期读锁。 |
| `fn rc_deref_mut(&self) -> Self::MutRef<'_> { self.0.borrow_mut() }` | 写锁。**重复借用会 panic**（§6③实测：`RefCell already borrowed`）。 |
| `fn try_rc_deref_mut(&self) -> Option<Self::MutRef<'_>> { self.0.try_borrow_mut().ok() }` | 同一件事的"不 panic"版本；`.ok()` 把 `Result<RefMut, BorrowMutError>` 折成 `Option`——这里是**协议要求**的错误出口，不是我们项目里被批斗的"用 `.ok()` 吞失败"。 |

### `MutArc`：同一批方法，另一套守卫

| 行 | 讲什么 |
|---|---|
| `pub struct MutArc<T>(Arc<Mutex<T>>);` | 多线程版容器。 |
| `type Ref<'a> = MutexGuard<'a, T> where Self: 'a;` / `type MutRef<'a> = MutexGuard<'a, T> ...` | 注意：读和写**是同一个守卫类型**（`MutexGuard` 既能 `Deref` 又能 `DerefMut`）。`MutRc` 那边是 `Ref`/`RefMut` 两种。抽象必须容忍这种不对称——又一次说明为什么守卫要 GAT 而不是"一个具体类型"。 |
| `self.0.lock().unwrap_or_else(\|poisoned\| poisoned.into_inner())` | 我做了**决定**而不是沿用默认：`rc.rs:164` 与 `:173` 用 `.unwrap()`，即"有线程在持锁时 panic ⇒ 数据可能不一致 ⇒ 一起炸"。我选 `into_inner()`（不信任 panic 造成的"中毒"，继续用数据）。两种都成立，但你必须知道自己选了哪种——课文里不许含糊。 |
| `fn try_rc_deref_mut(&self) -> Option<Self::MutRef<'_>> { self.0.try_lock().ok() }` | 同线程重复 `lock()` 会死锁；`try_lock` 把它变成"拿不到"（我们的断言就钉这件事）。 |

### `bump` / `peek`：抽象的全部回报

| 行 | 讲什么 |
|---|---|
| `pub fn bump<D: RcDerefMut<Target = i32>>(handle: &D, by: i32) -> i32 {` | 签名里没有 `Rc`/`Arc`/`RefCell`/`Mutex`，只有能力要求。谁选容器？调用方。 |
| `*handle.rc_deref_mut() += by;` | 取出守卫（`MutexGuard`/`RefMut`），`DerefMut` 让它表现得像 `&mut i32`；守卫在这一句末被临时值析构时释放。 |
| `*handle.rc_deref_mut()` | 第二次借：`RefCell` 允许，因为前一个守卫已经放掉。若把两句合成"持着写再要写"就是 §6③ 的 panic（`Mutex` 同线程则是 §6③附的"拒/死锁"）。 |
| `pub fn peek<D: RcDeref<Target = i32>>(handle: &D) -> i32 { *handle.rc_deref() }` | 只要"能借看"就够：`MutRc`/`MutArc` 都满足。 |
| `pub fn parallel_bump(handles: Vec<MutArc<i32>>, each: i32) -> usize {` | 只有 `Arc`/`Mutex` 这条路能编译（`Rc` 撞 `Send`，§6①）。 |
| `std::thread::spawn(move \|\| { for _ in 0..each { bump(&handle, 1); } handle })` | `move` 把句柄搬进线程（句柄本身是计数器的克隆，不搬数据）。 |
| `workers.filter_map(\|joined\| joined.join().ok()).count()` | `join()` 才拿到 `Result`（我第一版漏了 `join`，编译器直接指出 `no method named 'ok' found for JoinHandle`）。 |

### `SharedCell`：为什么还要第三条路

| 行 | 讲什么 |
|---|---|
| `pub trait SharedCell<T: Copy + Eq>: Clone + From<T> {` | 只给 `Copy` 的小值用：标志、计数。约束里直接要求 `Clone + From<T>`，所以实现者必须提供"从值造容器"。 |
| `fn get(&self) -> T; fn set(&self, value: T);` | 没有守卫 ⇒ 不存在"重复借用"这回事 ⇒ 不可能 panic、不可能死锁。 |
| `fn compare_exchange(&self, current: T, fresh: T) -> Result<T, T>;` | 统一的"改一下，除非被别人改过"。`rc.rs:55-62` 正是这个签名。 |
| `let old = self.0.get(); match old == current { true => { self.0.set(fresh); Ok(old) } false => Err(old) }` | `CellRc` 版是"读—比—写"三步，单线程安全；跨线程不安全。 |
| `self.0.compare_exchange(current, fresh, Ordering::SeqCst, Ordering::SeqCst)` | `CellArc` 版是真原子。这就是两版**必须分开**的理由。 |
| `#[derive(Clone)] pub struct CellArc(Arc<std::sync::atomic::AtomicI32>);` | 这里可以用 derive：`Arc<AtomicI32>` 本身 `Clone`。`rc.rs:182` 用泛型 `AtomicCell<T>`，所以能写成 `CellArc<T>`；我为零依赖只做了 `i32`。 |

## 5. 对照 rxrust 真实源码

| 本课 | rc.6 原文 | 备注 |
|---|---|---|
| `trait RcDeref: Clone` + `type Ref<'a>` | `rc.rs:28-36` | 一字不差的动机。 |
| `trait RcDerefMut` + `try_rc_deref_mut` | `rc.rs:38-52` | `rc.rs:45-49` 注释点名"检测同步重入，改走调度器"。 |
| `trait SharedCell` | `rc.rs:58-62` | 注释原话：比 `RcDerefMut` 更简单，给 flag/counter 用。 |
| `MutRc<T>(Rc<RefCell<T>>)` | `rc.rs:69` | 手写 `Clone`（`:71-73`）、`From`（`:75-77`）、`Default`（`:79-81`）。 |
| `MutArc<T>(Arc<Mutex<T>>)` | `rc.rs:143` | 守卫两边都是 `MutexGuard`（`:157-175`）。 |
| `CellRc` / `CellArc` | `rc.rs:106-131` / `:178-204` | 泛型 `AtomicCell<T>`（来自 `crossbeam_utils`，`rc.rs:22`）。 |
| （差异） | `rc.rs:23` 用 `rclite::{Arc, Rc}` | 不是 `std`。原因留到 V3 读 `context.rs` 时核对，不在这里凭印象编。 |
| （差异） | 我们的 `lock()` 走 `into_inner()`，源码走 `.unwrap()` | 中毒策略的选择，写在 §4 的表里。 |

## 6. 你会撞到的编译错（三条实测原文 + 一条运行时）

### ① `E0277` —— `Rc` 不能跨线程

复现：把 `bump(&handle, 1)` 放进 `std::thread::spawn(move || …)`，`handle` 是 `MutRc<i32>`。

```
error[E0277]: `Rc<RefCell<i32>>` cannot be sent between threads safely
223 | ...from(1); std::thread::spawn(move || { bump(&h, 1); }); }
    |             ------------------ -------^^^^^^^^^^^^^^^^^
    = help: within `{closure@...}`, the trait `Send` is not implemented for `Rc<RefCell<i32>>`
note: required because it appears within the type `MutRc<i32>`
```

注意报错**指向你的包装类型**（`MutRc<i32>`）而不是内部的 `Rc`——新类型包装不会让 `Send` 变得可见，
它是 `Rc` 的转述。这正是 V3 里 `Context` 要解决的：trait 决定 `Send` 与否，而不是算子。

### ② `derive(Clone)` 的隐藏要求

复现：`#[derive(Clone)] struct Derived<T>(Rc<RefCell<T>>);`，用 `NoClone` 实例化并克隆。

```
error[E0599]: the method `clone` exists for struct `Derived<NoClone>`, but its trait bounds were not satisfied
    | method `clone` not found for this struct because it doesn't satisfy `Derived<NoClone>: Clone`
note: trait bound `NoClone: Clone` was not satisfied
```

derive 是"给每个类型参数都加 `Clone` 约束"的偷懒生成器。**共享句柄的克隆只需要计数 +1**，
所以 `rc.rs:71-73` 手写。V0-03 我们刚学到"`mut self` 什么时候多余"，这里是同一类教训：
**别把 derive 当免费午餐，它是在替你加约束。**

### ③ 运行期代价：`borrow_mut` 二次借用会 panic

复现（一次性测试）：`let _a = h.rc_deref_mut(); let _b = h.rc_deref_mut();`

```
thread 'double_borrow_panics' panicked at src/lib.rs:74:16:
RefCell already borrowed
```

`rc.rs:99` 用的就是 `borrow_mut()`，所以栈上炸的是这条文本（`BorrowMutError` 是
`try_borrow_mut()` 返回的错误值，不是 panic 文本——我一度写反了，实测才改对）。
**这就是 `try_rc_deref_mut`（`rc.rs:50`）存在的理由**：同一线程重入时拿 `None`，
把"崩"变成"能处理的分支"。`Mutex` 侧同理：`try_lock()` 返回 `Err`，裸 `lock()` 同线程重复是死锁。

### ④ 如果没有 GAT（回到 V0-02 的报错）

把 `type Ref<'a>: Deref<Target = Self::Target> where Self: 'a;` 改成 `type Ref: Deref<...>;`
并让实现者写 `Ref<'a, T>`：编译器报的就是 V0-02 §6② 那条
`missing lifetime in associated type`。也就是说：**内部可变性 + 两种线程模型 = 必须 GAT**，
这两课是同一枚硬币。

## 7. 设计取舍

| 决策 | 选了什么 | 放弃什么、代价 |
|---|---|---|
| 共享可变 | `Rc<RefCell>`/`Arc<Mutex>`（运行期检查） | 到处 `clone()` 数据（成本）或传 `&mut`（在异步/多播场景下根本写不出来） |
| 抽象粒度 | `RcDeref`/`RcDerefMut` 两个 trait + GAT 守卫 | 只支持 `Rc`（代码要写两遍）；或 `unsafe`（放弃编译器帮忙） |
| Copy 小值 | 第三路 `SharedCell`（无守卫） | 用 `MutRc<bool>`：能做，但白付守卫开销与 panic 风险 |
| 重入 | 一律提供 `try_*` | 只提供会 panic 的版本（把库的 bug 变成用户进程的崩溃） |
| `Mutex` 中毒 | 本课 `into_inner()`，源码 `.unwrap()` | 二者都对，但必须写清：前者信任"panic 后数据仍可用"，后者不信任 |

## 8. 反例（本项目真踩过的）

1. **`Arc<Mutex<业务状态>>` 当万能总线**：我们项目里被禁的就是这个。共享可变只该用于
   框架机制（订阅句柄、守卫、计数）；业务状态该是**沿流被 move 的 owned 值**（V6 的
   `State` + `scan` 就是这个做法）。
2. **给 `Rc<RefCell<T>>` 写 `fn f(&mut self)` 版接口**：内部可变性的全部意义是"只有 `&self`"，
   要 `&mut` 等于白装这个容器。
3. **靠"我知道不会重入"来省掉 `try_*`**：`rc.rs:45-50` 的存在说明作者也被咬过；
   取消/重入这类路径必须有非 panic 的探测口。
4. **以为 `.clone()` 会复制内容**：`MutRc` 的 clone 是计数 +1，两个名字共享一份数据
   （本课有断言钉住这点）。

## 9. 练习（先做，再看答案）

1. 用 `#[derive(Clone)]` 重做 `MutRc`，亲手撞一次 §6②，然后改回手写 `Clone`。
   问：为什么 `Clone` 的 derive 要求是"每个类型参数都要 `Clone`"这么粗？
2. 用 `SharedCell` 写 `fn bump_shared<C: SharedCell<i32>>(counter: &C) -> i32`，
   CAS 失败时返回重读到的值，不 panic 不死锁。
3. 给 `Mutex` 也写一个"借用被拒时返回默认值"的 `bump_or_default`：只用
   `Option`/`match`，不许 `unwrap`。
4. 把 `bump` 分别用于 `MutRc<i32>` 与 `MutArc<i32>`，写一条断言证明两者结果相同；
   再把 `Rc` 版塞进线程，抄下 §6① 的报错。

### 参考答案

见 `code/00-04-shared-mutable.answers.rs`（已在验证 crate 编译并跑过）：

- 练习 2/3/4 全在该文件；`bump_or_default` 用 `match handle.try_rc_deref_mut()`
  两条分支收口（`Some(mut guard)` 改并回读，`None` 交 `0`）。
- 练习 1 的答案：derive 按"类型参数逐个加约束"生成，它不知道我们只包了指针；
  手写 `impl<T> Clone for MutRc<T>` 把约束缩到最小。
- 该文件顶部还写了一条纪律：**故意触发的编译错不能留在测试 crate 里**——它会让整个
  crate 编译失败，连带其他断言全跑不起来。那三条是在验证 crate 里另起一次性文件复现的。

## 10. 现在你应该能做到

- 看懂 `rc.rs` 每一行在防什么，并且知道"`Clone` 手写"和"`try_*` 存在"都不是风格偏好，是坑的记录；
- 说清为什么统一 `Rc<RefCell>` 与 `Arc<Mutex>` **只能**靠 GAT（V0-02 + 本课）；
- 到了 V3 读 `context.rs` 时，`Local`/`Shared` 的 `type RcMut<...>` 对你不会是黑话。

**下一课 V0-05**：宏的最小知识 —— 把 `observer.rs:133-169` 的 `impl_observer_for_box!`
手工展开成它真实生成的四种 `impl`，你会明白"为什么用宏而不是写四遍"，
以及为什么 `rust.md` 说宏不能拿来当第二套控制流。
