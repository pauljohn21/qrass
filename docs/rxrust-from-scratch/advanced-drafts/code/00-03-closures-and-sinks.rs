//! V0-03 配套代码：闭包的三种能力（`Fn` / `FnMut` / `FnOnce`），以及"消费者只需要实现
//! `next`"为什么等价于"错误与完成信号被静默丢掉"（对照 `observer.rs:255-277`）。
//! 断言在同名 `.tests.rs`，练习答案在 `.answers.rs`。

/// 只读捕获：闭包不改环境，所以 `impl Fn`。这类消费者可以并发地被反复调用。
pub fn read_only(f: impl Fn(i32) -> i32, values: &[i32]) -> Vec<i32> {
    values.iter().map(|v| f(*v)).collect()
}

/// 可变状态：累加要改环境，所以必须 `impl FnMut`——`Observer::next(&mut self, …)` 的同构物。
pub fn apply_each(values: &[i32], f: &mut impl FnMut(i32)) {
    for v in values {
        f(*v);
    }
}

/// 一次性：闭包可能把捕获的东西搬走（`String::into_string`、`drop`…），只能调一次。
pub fn consume_once(f: impl FnOnce(i32) -> i32, value: i32) -> i32 {
    f(value)
}

/// 有状态的闭包：`move` 把 `n` 的所有权搬进闭包，之后每次调用都改它 ⇒ 它只满足 `FnMut`。
pub fn once_counter() -> impl FnMut() -> usize {
    let mut n = 0;
    move || {
        n += 1;
        n
    }
}

/// 消费者协议：`next` 反复可用（可变借用），两个终止信号**消耗自身**
/// （对照 `observer.rs:19-43`）。
pub trait Observer<Item, Err> {
    fn next(&mut self, value: Item);
    fn error(self, err: Err);
    fn complete(self);
    fn closed(&self) -> bool;
}

/// 闭包适配器：只有 `next` 有真实实现，`error`/`complete` 是**空函数体**。
///
/// 这就是 rxrust `FnMutObserver`（`observer.rs:255-277`）的机制：`subscribe(closure)` 之后
/// 错误与完成信号到此为止，既不报错也不提示——本课用可观察断言把它钉死。
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

/// 一个"知道终止"的消费者：把错误写回调用方的 `Vec`，于是丢没丢变成可断言的事。
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

/// 只收前两个值就"关门"的消费者：把见到的写回调用方，所以"源头真的停了"可被断言。
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

    /// 收满两个就关门——源头每发一个之前都会问它。
    fn closed(&self) -> bool {
        self.seen >= 2
    }
}

/// 把每个信号都记到一个调用方持有的 `Vec` 里："谁收到了什么"从此可断言，
/// 不需要内部可变性（那是 V0-04 的主题）。
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

/// 用 `Option<O>` 当消费者：一旦被终止它就变成 `None`，后续 `next` 自动 no-op。
///
/// 这不是我发明的技巧：`observer.rs:284-335` 正是给 `Option<O>`（以及 `Rc<RefCell<Option<O>>>`
/// 一族）实现了 `Observer`，用 `take()` 表达"消费者已经不在了"。
impl<O: Observer<Item, Err>, Item, Err> Observer<Item, Err> for Option<O> {
    fn next(&mut self, value: Item) {
        if let Some(inner) = self {
            inner.next(value);
        }
    }

    /// 这里反而需要 `mut self`：`take()` 要可变借用 `self`（那个 `Option`）本身。
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

/// 推式发射：先把"安全前缀"推完，最后**一次性**交出终止信号。
///
/// 为什么不在循环里直接 `sink.error(e)`？因为 `error(self)` 消耗消费者，循环下一轮就要用
/// 已被搬走的值——编译器不允许"终止后还发"。这条约束正是本课第 3 节要说清的东西。
pub fn emit<O: Observer<i32, &'static str>>(
    items: Vec<Result<i32, &'static str>>,
    mut sink: Option<O>,
) {
    let (mut prefix, mut failure): (Vec<i32>, Option<&'static str>) = (Vec::new(), None);
    for item in items {
        match (item, failure) {
            (Ok(value), None) => prefix.push(value),
            (Err(err), None) => failure = Some(err),
            // 已经终止过：后面的元素一律不再看（协议：终止后不再发）
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
