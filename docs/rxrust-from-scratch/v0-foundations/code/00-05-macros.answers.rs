//! V0-05 练习答案（可编译；当作验证 crate 的 `tests/answers.rs` 跑）。
//!
//! 练习 1 要求"把宏展开成你认识的代码"。不能直接对同一对 trait 再写一遍——那会与宏
//! 生成的 impl 撞 `E0119`。所以这里另起一对 `SinkH`/`DynSinkH`，把三条调用**逐字手写**出来，
//! 再与库里的宏路径对比行为。

/// 协议（手写版专用，与库里的 `Sink` 同形）。
pub trait SinkH<Item> {
    fn receive(&mut self, value: Item);
    fn seen(&self) -> usize;
}

/// object-safe 镜像（手写版专用）。
pub trait DynSinkH<Item> {
    fn box_receive(&mut self, value: Item);
    fn box_seen(&self) -> usize;
}

impl<Item, T: SinkH<Item> + ?Sized> DynSinkH<Item> for T {
    fn box_receive(&mut self, value: Item) {
        self.receive(value);
    }

    fn box_seen(&self) -> usize {
        self.seen()
    }
}

#[derive(Default)]
pub struct TallyH {
    total: i32,
    hits: usize,
}

impl SinkH<i32> for TallyH {
    fn receive(&mut self, value: i32) {
        self.total += value;
        self.hits += 1;
    }

    fn seen(&self) -> usize {
        self.hits
    }
}

impl SinkH<&mut i32> for TallyH {
    fn receive(&mut self, value: &mut i32) {
        *value += 1;
        self.hits += 1;
    }

    fn seen(&self) -> usize {
        self.hits
    }
}

// ---- 练习 1：三条调用的手写展开（对照库里 `impl_sink_for_box!` 的三次调用）----

#[allow(coherence_leak_check)]
impl<'a, Item> SinkH<Item> for Box<dyn DynSinkH<Item> + 'a> {
    fn receive(&mut self, value: Item) {
        (**self).box_receive(value);
    }

    fn seen(&self) -> usize {
        (**self).box_seen()
    }
}

#[allow(coherence_leak_check)]
impl<'a, Item> SinkH<Item> for Box<dyn DynSinkH<Item> + Send + 'a> {
    fn receive(&mut self, value: Item) {
        (**self).box_receive(value);
    }

    fn seen(&self) -> usize {
        (**self).box_seen()
    }
}

#[allow(coherence_leak_check)]
impl<'a, T> SinkH<&mut T> for Box<dyn for<'m> DynSinkH<&'m mut T> + 'a> {
    fn receive(&mut self, value: &mut T) {
        (**self).box_receive(value);
    }

    fn seen(&self) -> usize {
        (**self).box_seen()
    }
}

// ---- 练习 2：加一种类型组合 = 加一条公开规则 + 一次调用，骨架不动 ----

/// 新组合：多一个 `Sync` 约束。骨架 `@impl` 一个字都没改。
///
/// 为什么不能换成 `Arc<dyn …>`：`Arc` 只能给出 `&T`，给不了 `&mut`，而镜像 trait 的
/// `box_receive(&mut self, …)` 要 `&mut`——实测报 `E0596: cannot borrow data in an 'Arc'
/// as mutable`。要共享就得回到 V0-04：`Arc<Mutex<_>>`。
macro_rules! impl_sink_for_box_sync {
    (@impl $ty:ty, generics = [$($generics:tt)*], item_type = $item:ty) => {
        #[allow(coherence_leak_check)]
        impl<$($generics)*> SinkH<$item> for $ty {
            fn receive(&mut self, value: $item) {
                (**self).box_receive(value);
            }

            fn seen(&self) -> usize {
                (**self).box_seen()
            }
        }
    };

    (sync: $ty:ty, lifetime = $lf:lifetime) => {
        impl_sink_for_box_sync!(@impl $ty, generics = [$lf, Item], item_type = Item);
    };
}

impl_sink_for_box_sync!(sync: Box<dyn DynSinkH<Item> + Sync + 'a>, lifetime = 'a);

#[test]
fn handwritten_expansion_behaves_like_the_macro() {
    let mut boxed: Box<dyn DynSinkH<i32> + '_> = Box::new(TallyH::default());
    boxed.receive(5);
    boxed.receive(5);
    assert_eq!(boxed.seen(), 2);

    let mut shared: Box<dyn DynSinkH<i32> + Sync + '_> = Box::new(TallyH::default());
    shared.receive(1);
    assert_eq!(shared.seen(), 1);

    let mut number = 41;
    let mut borrowed: Box<dyn for<'m> DynSinkH<&'m mut i32> + '_> = Box::new(TallyH::default());
    borrowed.receive(&mut number);
    assert_eq!((number, borrowed.seen()), (42, 1));
}
