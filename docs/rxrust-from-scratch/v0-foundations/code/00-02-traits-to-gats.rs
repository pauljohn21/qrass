//! V0-02 配套代码：从"泛型参数化元素类型"一路被编译器逼到 **GAT**
//! （`type Item<'a>`）——也就是 rxrust `ObservableType::Item<'a>` 的来处。
//! 断言在同名 `.tests.rs` 里。

/// 方案 A：把元素类型做成 **trait 的泛型参数**。
///
/// 能写，但每个使用点都要把 `Item` 带着跑：`fn f<S: SourceG<i32>>(…)`；
/// 而且一旦想同时提供"按值"和"按借用"两种产出，就得实现两次同名 trait。
pub trait SourceG<Item> {
    fn take(&mut self) -> Option<Item>;
}

/// 方案 B：**关联类型**——一个实现只配一种元素，名字也不用到处带参数。
pub trait Source {
    type Item;
    fn take(&mut self) -> Option<Self::Item>;
}

/// 按值产出的来源：有限计数器。
///
/// `limit` 不是为了测试才加的装饰品：`Option::None` 就是“结束了”的契约。若 `take`
/// 永不返回 `None`，下面的 `drain` 就是一个永不返回的死循环（我第一版就写出了这个）。
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
    /// 关联类型在这里被定为具体的 `i32`：调用方不需要再写 `SourceG<i32>`。
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

/// 另一个按值产出的来源：字符串队列。同一个 `Source` 名字、不同的 `Item`。
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

/// 一个对所有 `Source` 都成立的消费者：一直取到 `None` 为止。
///
/// 泛型参数版本必须写成 `SourceG<S::Item>`，关联类型版本只写 `Source` 就够了。
pub fn drain<S: Source>(src: &mut S, sink: &mut Vec<S::Item>) {
    while let Some(value) = src.take() {
        sink.push(value);
    }
}

/// 方案 C：**GAT（带生命周期的关联类型）**。
///
/// 有的来源不"交出拥有权"，而是"每次借一段自己内部的数据给你看"。这种元素的类型里
/// 有生命周期，而那个生命周期是**每个使用者**决定的，不是实现方一次定死的——
/// 于是关联类型必须自己带参数。
pub trait Windowed {
    /// `where Self: 'a` 读作"只有当 self 至少和 'a 一样长寿时，这个类型才存在"。
    /// 少了这个约束，编译器不知道 `Self` 能不能活得比借用期久，直接拒绝（见课文 §6③）。
    type Row<'a>: Iterator<Item = &'a i32> where Self: 'a;

    /// 返回 `Self::Row<'_>`：那个匿名生命周期由 `&self` 推断而来，
    /// 所以窗口不会比被看的 `Series` 活得更久。
    fn window(&self, at: usize, len: usize) -> Self::Row<'_>;
}

/// 一段连续的数据：可以被"切窗口借用"，也能被按值消费。
pub struct Series {
    values: Vec<i32>,
}

impl Series {
    pub fn new(values: Vec<i32>) -> Series {
        Series { values }
    }
}

impl Windowed for Series {
    /// 借用切片迭代器：它的元素正是 `&'a i32`——这就是"元素类型带生命周期"的具体样子。
    type Row<'a> = std::slice::Iter<'a, i32> where Self: 'a;

    fn window(&self, at: usize, len: usize) -> Self::Row<'_> {
        self.values[at..at + len].iter()
    }
}

/// 泛型函数直接用 GAT：调用点不需要写任何生命周期标注，`Row<'_>` 由借用的寿命推出来。
pub fn window_total<S: Windowed>(s: &S, at: usize, len: usize) -> i32 {
    s.window(at, len).sum()
}

/// 同时用 trait + 泛型闭包：这就是 rxrust 算子的最小骨架
/// （源码里是 `FnMut(Self::Item<'a>)`，`'a` 由每一次发射决定）。
pub fn map_window<S: Windowed>(s: &S, at: usize, len: usize, f: impl FnMut(&i32)) {
    // 不写 `|v| f(v)`（冗余闭包），也不给 `f` 加 `mut`：`for_each` 把闭包搬走后自己把它变可变。
    s.window(at, len).for_each(f);
}
