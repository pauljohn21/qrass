//! V0-04 配套代码：共享所有权（`Rc`/`Arc`）× 内部可变性（`RefCell`/`Mutex`/`Cell`），
//! 以及 rxrust 为什么要用 **GAT** 把两者统一成一个 `RcDeref`/`RcDerefMut` 抽象
//! （对照 `rc.rs:29-50`、`rc.rs:66-101`、`rc.rs:141-172`）。
//!
//! 本课只用 std：`rc.rs:21-22` 实际用的是 `rclite::{Arc, Rc}` 与
//! `crossbeam_utils::atomic::AtomicCell`，差异在 V3 读 `context.rs` 时再核对，
//! 不在这里凭印象解释。

use std::cell::{Cell, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};

/// 只读的共享解引用。`Ref<'a>` 是 GAT：守卫类型由实现者决定，寿命由使用者决定
/// （`rc.rs:30-37` 就是这个形状）。
pub trait RcDeref: Clone {
    type Target;
    type Ref<'a>: Deref<Target = Self::Target>
    where
        Self: 'a;

    fn rc_deref(&self) -> Self::Ref<'_>;
}

/// 可变的共享解引用。注意接收者是 `&self`——这就是"内部可变性"的定义：
/// 句柄本身不用可变，里面的数据能变。
pub trait RcDerefMut: RcDeref {
    type MutRef<'a>: DerefMut<Target = Self::Target>
    where
        Self: 'a;

    fn rc_deref_mut(&self) -> Self::MutRef<'_>;

    /// 拿不到就返回 `None` 而不是 panic/卡死（`rc.rs:49-50` 明说用途：
    /// 检测同步重入，从而改走调度器而不是当场炸）。
    fn try_rc_deref_mut(&self) -> Option<Self::MutRef<'_>>;
}

/// 单线程容器：`Rc<RefCell<T>>`（对照 `rc.rs:68`）。
pub struct MutRc<T>(Rc<RefCell<T>>);

/// 手写 `Clone` 而不是 `#[derive(Clone)]`：derive 会要求 `T: Clone`，
/// 而克隆指针只需要引用计数 +1（`rc.rs:70-72` 同样是手写的）。
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
    /// 同一个方法，守卫却是另一种类型：这正是"关联类型必须带 `'a`"的场景。
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

/// 一个对两种容器**都成立**的函数：这就是整套抽象的全部目的。
///
/// 签名里没有 `Rc`、没有 `Arc`、没有 `RefCell`、没有 `Mutex`：
/// 只有"能力要求"（`RcDerefMut<Target = i32>`），所以调用方选容器。
pub fn bump<D: RcDerefMut<Target = i32>>(handle: &D, by: i32) -> i32 {
    *handle.rc_deref_mut() += by;
    *handle.rc_deref_mut()
}

/// 只读版：任何共享句柄的内容都能被"看一眼"。
pub fn peek<D: RcDeref<Target = i32>>(handle: &D) -> i32 {
    *handle.rc_deref()
}

/// Copy 类型用的轻量通道（对照 `rc.rs:46-56` 的 `SharedCell` 与 `CellRc`/`CellArc`）：
/// 不需要守卫，因此没有"重复借用"这个问题。
pub trait SharedCell<T: Copy + Eq>: Clone + From<T> {
    fn get(&self) -> T;
    fn set(&self, value: T);
    fn compare_exchange(&self, current: T, fresh: T) -> Result<T, T>;
}

/// 单线程版：`Rc<Cell<T>>`。
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

    /// `Cell` 没有原子 CAS，只能"读—比—写"；单线程内这一步不会被打断，
    /// 跨线程就不成立了——那是要用原子版的真正理由。
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

/// 多线程版。`rc.rs:176-204` 用的是泛型的 `AtomicCell<T>`；
/// 这里为了零依赖，只做一个具体宽度（i32）的原子版。
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

/// 跨线程累加：只有 `Arc`/`Mutex` 这条路能编译（`Rc` 会撞上 `Send`）。
pub fn parallel_bump(handles: Vec<MutArc<i32>>, each: i32) -> usize {
    let workers = handles.into_iter().map(|handle| {
        std::thread::spawn(move || {
            for _ in 0..each {
                bump(&handle, 1);
            }
            handle
        })
    });
    // `join()` 把每个工作线程的 `Result` 取回来；`ok()` 丢掉“线程 panic”这种失败（本课不造 panic）。
    workers.filter_map(|joined| joined.join().ok()).count()
}
