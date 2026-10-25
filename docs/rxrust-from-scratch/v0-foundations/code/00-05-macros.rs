//! V0-05 配套代码：`macro_rules!` 的最小可用知识。
//!
//! 标本是 `observer.rs:133-167` 的 `impl_observer_for_box!`——它有三段结构：
//! 一条内部规则（`@impl`，公共骨架）+ 两条公开规则（`regular:` / `mut_ref:`），
//! 用 `$($generics:tt)*` 把"泛型列表"当令牌流搬运。这里用一对更小的 trait 复刻同一结构，
//! 于是你能逐字看清它展开成什么（答案文件里手写了一份等价实现）。

/// 协议（对照 `Observer`）：`Item` 是普通泛型参数，所以 `Sink<i32>` 与 `Sink<&mut i32>`
/// 可以由同一个类型各实现一次。
pub trait Sink<Item> {
    fn receive(&mut self, value: Item);
    fn seen(&self) -> usize;
}

/// object-safe 镜像（对照 `DynObserver`）：只有 `&mut self`，因此能装进 `Box<dyn …>`。
pub trait DynSink<Item> {
    fn box_receive(&mut self, value: Item);
    fn box_seen(&self) -> usize;
}

/// blanket impl（对照 `observer.rs:117-125`）：任何 `Sink` 都自动是 `DynSink`。
impl<Item, T: Sink<Item> + ?Sized> DynSink<Item> for T {
    fn box_receive(&mut self, value: Item) {
        self.receive(value);
    }

    fn box_seen(&self) -> usize {
        self.seen()
    }
}

/// 一个具体消费者：累加并计数。
#[derive(Default)]
pub struct Tally {
    total: i32,
    hits: usize,
}

impl Sink<i32> for Tally {
    fn receive(&mut self, value: i32) {
        self.total += value;
        self.hits += 1;
    }

    fn seen(&self) -> usize {
        self.hits
    }
}

impl Sink<&mut i32> for Tally {
    fn receive(&mut self, value: &mut i32) {
        *value += 1; // 借用式元素：改的是调用方那个变量
        self.hits += 1;
    }

    fn seen(&self) -> usize {
        self.hits
    }
}

/// 复刻 `impl_observer_for_box!` 的结构。
macro_rules! impl_sink_for_box {
    // 内部规则：唯一的骨架。`generics = [...]` 用 `tt` 流把"要生成哪些泛型参数"搬进来；
    // `item_type = $item:ty` 决定 `Sink<…>` 的角标写什么。
    (@impl $ty:ty, generics = [$($generics:tt)*], item_type = $item:ty) => {
        // 不加这行，编译器会警告：`regular`（`Sink<Item> for Box<dyn DynSink<Item>>`）与
        // `mut_ref`（`Sink<&mut T> for Box<dyn for<'m> DynSink<&'m mut T>>`）在 `Item = &mut T`
        // 时重叠——这就是 `observer.rs:137` 那句 `#[allow(coherence_leak_check)]` 的实测理由。
        #[allow(coherence_leak_check)]
        impl<$($generics)*> Sink<$item> for $ty {
            fn receive(&mut self, value: $item) {
                (**self).box_receive(value);
            }

            fn seen(&self) -> usize {
                (**self).box_seen()
            }
        }
    };

    // 公开规则一：`Item` 是类型参数（对照 `observer.rs:159-161`）。
    (regular: $ty:ty, lifetime = $lf:lifetime) => {
        impl_sink_for_box!(@impl $ty, generics = [$lf, Item], item_type = Item);
    };

    // 公开规则二：元素是 `&mut T`（对照 `observer.rs:164-166`：搬运的泛型参数换成了 `T`）。
    (mut_ref: $ty:ty, lifetime = $lf:lifetime) => {
        impl_sink_for_box!(@impl $ty, generics = [$lf, T], item_type = &mut T);
    };
}

// 三条调用（对照 `observer.rs:169-171`）：盒子本身也是**泛型**的
// （`DynSink<Item>` / `DynSink<&mut T>`），否则生成的 `impl` 会把元素类型锁死。
// 前两条只差一个 `Send`。
impl_sink_for_box!(regular: Box<dyn DynSink<Item> + 'a>, lifetime = 'a);
impl_sink_for_box!(regular: Box<dyn DynSink<Item> + Send + 'a>, lifetime = 'a);
impl_sink_for_box!(mut_ref: Box<dyn for<'m> DynSink<&'m mut T> + 'a>, lifetime = 'a);

/// 最小重复示例：`$($v:expr),*` 是"逗号分隔、零或多"，`$(,)?` 容忍尾逗号。
macro_rules! sum_all {
    ($($v:expr),* $(,)?) => {{
        let mut acc = 0;
        $(acc += $v;)*
        acc
    }};
}

/// 卫生性（hygiene）示例：宏里的 `hits` 与调用方的 `hits` 是两个东西。
///
/// 这是 `macro_rules!` 与"文本替换"的根本区别——展开不会意外捕获周围的名字。
macro_rules! counted {
    ($value:expr) => {{
        let hits = 1;
        $value + hits
    }};
}

/// 调用方自己也叫 `hits`。若宏是文本替换，这里会得到 `5 + 100`。
///
/// 返回元组而不只是宏的结果：顺手证明外层 `hits` 没被宏里的 `hits` 捕获、也没被它改写。
pub fn hygiene_probe() -> (i32, usize) {
    let hits = 100;
    let via_macro = counted!(5);
    (via_macro, hits)
}

/// 把值依次推给"盒子里的消费者"，返回它自己记的账。
///
/// 参数类型是 `Box<dyn DynSink<i32> + 'a>`，而 `receive` 来自宏生成的 `Sink<i32>` impl：
/// 没有那个宏，这里就得对每个盒子形态手写一遍。
pub fn drain_boxed<'a>(mut boxed: Box<dyn DynSink<i32> + 'a>, values: &[i32]) -> usize {
    for value in values {
        boxed.receive(*value);
    }
    boxed.seen()
}

/// 借用式版本：元素是 `&mut i32`，走的另一条宏规则。
pub fn drain_mut_ref<'a>(mut boxed: Box<dyn for<'m> DynSink<&'m mut i32> + 'a>, value: &mut i32) {
    boxed.receive(value);
}

/// 只用 `macro_rules!` 的重复符数一下有几个参数。
pub fn summed() -> i32 {
    sum_all!(1, 2, 3, 4)
}
