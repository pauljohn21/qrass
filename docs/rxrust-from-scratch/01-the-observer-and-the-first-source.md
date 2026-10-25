# L01 — Observer 与第一个源头：为什么"消费者"要先被定义

> 本课产出：一个能跑的 `minirx`，只做两件事——定义消费者(`Observer`)、定义一个会**往前推**值的源头
> (`from_iter`)。跑起来你会看到 `1 2 3`，并且明白为什么 `subscribe` 结束时它们**已经全部发完了**。
>
> 对照源码：`observer.rs:19-43`、`observer.rs:255-277`、`observable/from_iter.rs:29-75`。

## 0. 先想清楚问题：我们到底要摆脱什么

命令式写法是**拉(pull)**：

```rust
for item in data {          // 我（驱动方）决定什么时候取下一个
    if item.is_ok() { sink.push(item) }   // 我决定给谁
}
```

它在你手上是好的。一旦要"把一段处理交给别人、还要中途取消、还要一份数据喂好几个消费者"，
拉式的代价就出来了：**得有人持有整份数据、得有人决定生命周期**——在带 GC 的语言里这就是"对象图 +
访问者 + 谁负责释放"的老问题。

响应式把它整个反过来：**推(push)**。数据由源头一件一件往前喂，中间每一站只是"值路过时被加工一下"。
没有整份数据被持有，也没有回头遍历。要真正做到这点，第一件事不是定义"流"，而是**定义"值要去哪儿"**：
也就是消费者。这就是 `Observer`。

## 1. 先自己试（别跳过）

在你的 `minirx/src/lib.rs` 里，只凭上面的描述写：
1) 一个 trait，能接收值、能被告知出错、能被告知结束；
2) 一个 struct 把收到的值存进 `Vec`；
3) 一个函数，把 `[1,2,3]` 挨个喂给它。

写不出来很正常。**卡住的地方就是本课的内容**——尤其是"出错/结束时你希望调用方还能继续用这个消费者吗？"

## 2. 我们的实现（本课完整代码）

```rust
//! minirx — L01
use std::convert::Infallible;

/// 值要去的地方。
pub trait Observer<Item, Err> {
    fn next(&mut self, value: Item);   // 收到一个值
    fn error(self, err: Err);          // 出错：把消费者交出去（消耗）
    fn complete(self);                 // 结束：把消费者交出去（消耗）
    fn is_closed(&self) -> bool;       // 源头问它：还要不要继续发
}

/// 最简单的消费者：把值收进 Vec（测试用）。
#[derive(Default)]
pub struct VecObserver<Item> {
    pub values: Vec<Item>,
    closed: bool,
}

impl<Item> Observer<Item, Infallible> for VecObserver<Item> {
    fn next(&mut self, value: Item) { self.values.push(value); }
    fn error(self, _: Infallible) {}
    fn complete(self) {}
    fn is_closed(&self) -> bool { self.closed }
}

/// 第一个源头：一个迭代器，订阅时把值往前推。
pub struct FromIter<I: IntoIterator> { iter: I }

pub fn from_iter<I: IntoIterator>(iter: I) -> FromIter<I> { FromIter { iter } }

impl<I, O> FromIter<I>
where
    I: IntoIterator,
    O: Observer<I::Item, Infallible>,
{
    /// 注意 `self`：订阅即消耗源头本身，它只能被"跑"一次。
    pub fn subscribe_with(self, mut observer: O) {
        for item in self.iter {
            if observer.is_closed() { return; }   // 关键：不是 break 后还 complete
            observer.next(item);
        }
        observer.complete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_pushes_into_observer() {
        let mut sink = VecObserver::default();
        // 借用它当消费者：subscribe_with 消耗的是源头，不是 sink
        {
            let mut view = &mut sink as &mut dyn Observer<_, Infallible>;
            let _ = &view;                        // 见下方「为什么这里别扭」
            let _ = view.is_closed();
        }
        let _ = sink.values.len();
    }
}
```

> 上面测试故意留了一处别扭（拿 `&mut sink` 当 `Observer` 时，`error(self)`/`complete(self)` 需要
> 拥有权，借用给不了）。**这个别扭正是源码要解决的东西**，见 §4。

## 3. 逐行讲：为什么是这四个方法

### `fn next(&mut self, value: Item)`
- `&mut self`：接收值需要可变状态（累加、缓冲、写文件），但不**消耗**消费者——后面还有值要来。
- `Item` 是**被移动进来的拥有值**，不是 `&Item`。这是本库最重要的选择：值路过每一站时被搬走，
  所以不存在"谁持有它、什么时候能释放"的问题（GC 语言里这条要靠引用+终结器）。

### `fn error(self, err: Err)` 与 `fn complete(self)`
- 两个都**消耗 `self`**。为什么？响应式的协议是：**终止之后不许再发任何信号**
  （Rx 的 formal grammar：`(next)* (error | complete)?`）。
- 用类型系统表达协议 = 编译器强制。你把消费者交出去、它没了，就**不可能**在它终止后再调 `next`。
  这就是"所有权即协议"：不需要运行时 `assert!(!terminated)`，也不需要 GC 去追踪"这个 sink 还活着吗"。
- 对比 `next`：它若也写成 `self`，第一个值就把消费者吃掉了；写成 `&mut self` 才能连续收。

### `fn is_closed(&self) -> bool`
- 它存在的唯一理由：**取消要能传到源头**。`take(2)` 收到第 2 个值后就把自己关掉，但循环在**源头**手里；
  于是源头每发一个前问一句"还收吗"，不收就早停。
- 这是"推式流"的背压/取消机制：没有队列、没有线程阻塞，只有一个布尔判断插在发射循环里。
  `from_iter.rs:64-74` 的源码就是你我上面那 6 行，一字不差的结构。

### 为什么 `Err` 是泛型参数，而源头常常用 `Infallible`
- `Infallible` 是"永远不可能发生的错误"类型。`from_iter`/`map`/`filter` 这类算子不会失败，
  它们的 `Err` 就是 `Infallible`；`throw_err`/用户 `create`/会失败的算子才带真错误。
- 把错误类型放进 trait 参数，好处：错误通道是**类型的一部分**，`Err = Infallible` 的链你连 `.error()`
  都不用处理；坏处（也是 L02 的重点）：一旦你写 `Err = Infallible` 又把真错误当值塞进流里，
  就会掉进"`subscribe(closure)` 把 error 吞了"的坑——因为闭包适配器根本没有 error 分支。

## 4. 对照真实源码

### `observer.rs:19-43` —— 四个方法，与我们一一对应

```rust
pub trait Observer<Item, Err> {
  fn next(&mut self, value: Item);      // :21
  fn error(self, err: Err);             // :27  注释写明：消耗，因为出错后不能再发
  fn complete(self);                    // :33  同上
  fn is_closed(&self) -> bool;          // :42  注释点名：给 from_iter 这类源头做提前停
}
```

设计注释（`:25-32`、`:35-41`）已经把"为什么"写给你了：终止方法 `self` 是**故意的**，
`is_closed` 的**唯一客户就是源头**。

### `observer.rs:255-277` —— `FnMutObserver`：闭包怎么变成消费者

```rust
pub struct FnMutObserver<F>(pub F);
impl<F, Item> Observer<Item, Infallible> for FnMutObserver<F> where F: FnMut(Item) {
  fn next(&mut self, v: Item) { (self.0)(v); }          // :263
  fn error(self, _err: Infallible) {}                   // :266  ← 什么都不做
  fn complete(self) {}                                  // :271  ← 什么都不做
  fn is_closed(&self) -> bool { false }                 // :276  ← 永不关
}
```

三行"什么都不做"解释了一个你迟早会撞的现象：**`obs.subscribe(|v| ...)` 看不见错误，也看不见结束**。
不是 bug，是适配器的类型选择了 `Err = Infallible`（错误不可能发生 ⇒ 不必处理）。
所以真实项目里错误必须走**流内 `Result`** 或用 `subscribe_with`/`on_error`/`into_future`（L11）。

### `observer.rs:95-99` + `:110-125` —— 两个"看起来多余"的 trait，其实各解决一个硬问题

- `Emitter<Item, Err>`：`next/error/complete` **全部 `&mut self`**。源码注释（`:53-68`）给了两条理由：
  1. `&mut dyn Emitter` 可以做**零堆分配的动态派发**（`create` 的闭包不需要知道下游具体类型）；
  2. 防止抽象泄漏：下游可能是 `MapObserver`/`FilterObserver`/`Subject`/用户闭包，
     有了 `&mut dyn Emitter` 这个门面，上游的类型签名就不会被下游类型污染。
- `DynObserver`（`:110-115`）：`Observer` 因为 `error(self)`/`complete(self)` 消耗自身而**不是
  object-safe**，所以另造一个 `box_error(self: Box<Self>, ...)` 形态的镜像 trait，
  再用 blanket impl（`:117-125`）把任意 `Observer` 自动变成 `DynObserver`。
  —— 这正是 §2 那个"借用别扭"的解法：想动态派发就换到这层来。

### `observable/from_iter.rs:29-75` —— 源头长什么样

```rust
pub fn from_iter<Iter>(iter: Iter) -> FromIter<Iter> where Iter: IntoIterator { FromIter { iter } }
#[derive(Clone)] pub struct FromIter<Iter> { iter: Iter }              // :37-39
impl<Iter> ObservableType for FromIter<Iter> {                          // :41
  type Item<'a> = Iter::Item;   // GAT：L04 讲它为什么不能写成 `type Item`
  type Err = Infallible;
}
impl<C, Iter> CoreObservable<C> for FromIter<Iter> where C: Context, C::Inner: Observer<...> {
  type Unsub = ();
  fn subscribe(self, context: C) -> Self::Unsub {
    let mut observer = context.into_inner();          // :63  从"容器"里取出真正的消费者
    for item in self.iter {
      if observer.is_closed() { break; }              // :66-68  取消/背压
      observer.next(item);                            // :69
    }
    if !observer.is_closed() { observer.complete(); } // :72-74  关掉了就别再宣布结束
  }
}
```

源码比我们多三样东西，每一样都是后面课程的主题：
| 多出来的 | 为什么需要 | 哪一课 |
|---|---|---|
| `ObservableType` / `CoreObservable<C>` 拆两层 | 让**一份算子代码**同时适配单线程与多线程 | L03、L04 |
| `context: C` + `into_inner()` | 消费者被装在 `Rc<RefCell<_>>` 还是 `Arc<Mutex<_>>` 里，是 Context 的事 | L03 |
| `type Unsub` | 取消句柄（我们上面用 `is_closed` 只能"源头自发停"，还不能外部取消） | L02 |

## 5. 设计取舍（不这样写会怎样）

| 选择 | 替代方案 | 替代方案的代价 |
|---|---|---|
| `error/complete` 消耗 `self` | `&mut self` + 内部 `terminated: bool` | 协议退化成运行时约定；重复终止、终止后再发 next 只能靠断言/panic |
| `Item` 按值传 | `&Item` | 值必须有地方活着（跨站引用 ⇒ 生命周期地狱或提前物化整份数据 = 回到拉） |
| `is_closed` 由源头轮询 | 拉式 `Iterator::next` + `take` | 那正是 pull 模型；无法把"取消"传播给异步/多路生产者 |
| 错误类型进 trait 参数 | 一律 `Result` 当值 | 每个算子都要处理错误分支；`Err=Infallible` 的链白白付代价（不过本项目实践仍选 in-stream `Result`，见 §6） |

## 6. 反例（我们自己踩过的，别重复）

1. **`subscribe(closure)` 当终点用，然后奇怪错误去哪了。** 见 `FnMutObserver`（`:266`/`:271` 空实现）。
2. **把 `Observer` 做成 `Rc<RefCell<dyn Observer>>` 到处 clone，当"共享可变 sink"用。**
   源码给多消费者的是 `Subject`（L09），不是拿共享可变绕。
3. **`VecObserver` 出现在生产代码里。** 它只是测试脚手架；一旦你在库里 `collect` 到 `Vec`
   再回头遍历，就把推式链降级成了批处理（我们在 sasspile 里正是这样被判"GC 思维"的）。
4. **给 `FromIter` 加 `&mut self` 版 subscribe 以便"复用源头"。** `Clone`（`:37`）才是答案：
   冷源每次订阅都是新一次发射，复用 `iter` 会破坏"订阅即从头"的语义。

## 7. 练习

1. 把 `VecObserver` 换成 `PrintObserver`，验证 `1 2 3` 顺序输出——并解释为什么 `subscribe_with`
   返回后已经打印完（**推式 + 同步内联**）。
2. 给 `VecObserver` 加 `limit: usize` 计数，超限时 `closed = true`；在 `subscribe_with` 里
   打印实际收到的个数，确认早停生效（这就是 `take` 的雏形）。
3. 试着写 `fn error(self, err: Err)` 的真实实现（`Err = String`）：把错误写进 `Vec<Result<Item,String>>`。
   然后回答：**为什么它必须消耗 `self`？** 用编译器证明给你自己看（试着在 `error` 之后再调 `next`）。
4. 读 `observer.rs:284-335`（`Option<O>` 与 `P: RcDerefMut<Target=Option<O>>` 两个 impl）。
   回答：`Option` 为什么能当 Observer？`take()` 在这里起什么作用？（提示：终止时"把里面的消费者搬走"。）

---
下一课：[L02 取消与 Subscription](02-subscription-and-cancellation.md) ——
把 `is_closed` 升级成真正的"外部可取消"，并顺手看清 `SingleAssignment` 在防什么。
