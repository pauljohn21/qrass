//! V0-03 练习答案（可编译；当作验证 crate 的 `tests/answers.rs` 跑）。
//!
//! 重点有两条：① 源头如何"关门后连终止信号也不发"；② `E0161`（不能通过 `dyn` 移动 `self`）
//! 的正解正是 rxrust 的 `DynObserver` 镜像 trait + `box_error(self: Box<Self>, …)`。

use minirx_check::{Limited, Observer, emit};

/// 练习 1：一个自己拥有日志、会记录两个信号的消费者（按值传给 `emit` 也行得通）。
#[derive(Default)]
pub struct Collecting {
    pub seen: Vec<String>,
}

impl Observer<i32, &'static str> for Collecting {
    fn next(&mut self, value: i32) {
        self.seen.push(format!("next {value}"));
    }

    /// 这里确实需要 `mut self`：`seen` 是 `self` **拥有**的 `Vec`，push 要可变借用它
    // （与 `Recorder`/`Echo` 不同：那两个只是透过 `&mut Vec` 字段写入，所以不需要 `mut`）。
    fn error(mut self, err: &'static str) {
        self.seen.push(format!("error {err}"));
    }

    fn complete(mut self) {
        self.seen.push("complete".to_string());
    }

    fn closed(&self) -> bool {
        false
    }
}

/// 练习 2：消费者关门后，连终止信号也不该再送。
///
/// 注意这里用 `match (stopped, failure)` 一次分派三种结局，而不是叠 `if`+提前 `return`：
/// "只发一个终止信号"由结构的唯一出口保证。
pub fn emit_polite<O: Observer<i32, &'static str>>(
    items: Vec<Result<i32, &'static str>>,
    mut sink: Option<O>,
) {
    let (mut prefix, mut failure): (Vec<i32>, Option<&'static str>) = (Vec::new(), None);
    for item in items {
        match (item, failure) {
            (Ok(value), None) => prefix.push(value),
            (Err(err), None) => failure = Some(err),
            _ => {}
        }
    }
    let mut stopped = false;
    for value in prefix {
        if sink.closed() {
            stopped = true;
            break;
        }
        sink.next(value);
    }
    match (stopped, failure) {
        (true, _) => {}
        (false, Some(err)) => sink.error(err),
        (false, None) => sink.complete(),
    }
}

/// `DynObserver`：`Observer` 的 object-safe 镜像（对照 `observer.rs:110-125`）。
///
/// 消耗式方法在这里换成 `self: Box<Self>`，于是能通过 `dyn` 调用——修好 §6③ 的 `E0161`。
pub trait DynObserver<Item, Err> {
    fn box_next(&mut self, value: Item);
    fn box_error(self: Box<Self>, err: Err);
    fn box_complete(self: Box<Self>);
    fn box_closed(&self) -> bool;
}

impl<T: Observer<Item, Err> + 'static, Item, Err> DynObserver<Item, Err> for T {
    fn box_next(&mut self, value: Item) {
        self.next(value);
    }

    fn box_error(self: Box<Self>, err: Err) {
        self.error(err);
    }

    fn box_complete(self: Box<Self>) {
        self.complete();
    }

    fn box_closed(&self) -> bool {
        self.closed()
    }
}

#[test]
fn owning_consumer_compiles_and_runs() {
    emit(vec![Ok(1), Err("boom")], Some(Collecting::default()));
    // “信号真的被记下来了”由课文断言集里的 `Echo`（日志归调用方）负责证明，
    // 本文件只证明：拥有式日志的消费者在同一条协议下同样可用。
}

#[test]
fn polite_version_sends_no_terminal_signal_after_close() {
    let mut log: Vec<String> = Vec::new();
    emit_polite(vec![Ok(1), Ok(2), Ok(3)], Some(Limited::new(&mut log)));
    assert_eq!(log, vec!["next 1", "next 2"], "关门后连 complete 也不该送达");

    // 对照：未改的 `emit` 会多送一个终止信号（已被 V0-03 的断言固定）。
    let mut plain: Vec<String> = Vec::new();
    emit(vec![Ok(1), Ok(2), Ok(3)], Some(Limited::new(&mut plain)));
    assert_eq!(plain, vec!["next 1", "next 2", "complete"]);
}

#[test]
fn boxed_observers_are_drivable_through_the_mirror_trait() {
    let mut boxed: Box<dyn DynObserver<i32, &'static str>> = Box::new(Collecting::default());
    boxed.box_next(1);
    boxed.box_next(2);
    assert!(!boxed.box_closed());
    boxed.box_complete(); // `self: Box<Self>`：消耗盒子，这正是 `dyn` 能做到的形式
}
