//! V0-03 的断言：三种闭包能力的分工，以及"闭包消费者丢掉终止信号"的可观察证明。
//!
//! 这里的每个断言都必须能"看见"某个信号到没到达消费者——靠 `Echo`（把信号写回调用方的
//! `Vec`）而不是靠"我以为它没收到"。

use minirx_check::{
    Echo, FnObserver, Limited, Observer, Recorder, apply_each, consume_once, emit, once_counter,
    read_only,
};

#[test]
fn the_three_closure_powers_do_different_jobs() {
    assert_eq!(read_only(|v| v * v, &[1, 2, 3]), vec![1, 4, 9]);

    let mut total = 0;
    apply_each(&[1, 2, 3], &mut |v| total += v);
    assert_eq!(total, 6);

    assert_eq!(consume_once(|v| v + 1, 41), 42);

    let mut counter = once_counter();
    assert_eq!((counter(), counter(), counter()), (1, 2, 3));
}

#[test]
fn a_closure_sink_never_hears_the_error() {
    let mut seen: Vec<String> = Vec::new();
    emit(
        vec![Ok(1), Ok(2), Err("boom")],
        Some(FnObserver(|v| seen.push(format!("next {v}")))),
    );

    // 闭包只会见到 `next`：`FnObserver::error` 是空实现（对照 observer.rs:266-273）。
    assert_eq!(seen, vec!["next 1", "next 2"]);
    assert!(
        !seen.iter().any(|s| s.contains("boom")),
        "错误文本从未到达闭包消费者——这就是 subscribe(closure) 吞错"
    );
}

#[test]
fn a_sink_that_implements_error_receives_it() {
    let mut got: Vec<String> = Vec::new();
    emit(vec![Ok(1), Err("boom")], Some(Echo(&mut got)));
    assert_eq!(got, vec!["next 1", "error boom"]);

    let mut failures: Vec<&'static str> = Vec::new();
    emit(vec![Err("only")], Some(Recorder(&mut failures)));
    assert_eq!(failures, vec!["only"]);
}

#[test]
fn a_closed_sink_makes_the_source_stop_early() {
    let mut log: Vec<String> = Vec::new();
    emit(vec![Ok(1), Ok(2), Ok(3)], Some(Limited::new(&mut log)));
    assert_eq!(log, vec!["next 1", "next 2", "complete"]);
    // 上面就是实测结果：`closed()` 只能拦住**后续发射**，已经拿到的那个终止信号仍会送达；
    // 若不希望 "complete" 送达，源头得自己根据关闭状态跳过终止信号（见 answers）。
}

#[test]
fn completion_arrives_exactly_once_and_nothing_arrives_after() {
    let mut got: Vec<String> = Vec::new();
    emit(vec![Ok(7), Ok(8)], Some(Echo(&mut got)));
    assert_eq!(got, vec!["next 7", "next 8", "complete"]);

    // 终止之后不再发：`take()` 把消费者从 `Option` 里取出并终止它，
    // 剩下的是 `None` —— 后续 `next` 自动 no-op（对照 observer.rs:322-332）。
    let mut after: Vec<String> = Vec::new();
    let mut sink: Option<Echo<'_>> = Some(Echo(&mut after));
    sink.next(1);
    if let Some(inner) = sink.take() {
        inner.error("boom");
    }
    sink.next(9);
    assert_eq!(after, vec!["next 1", "error boom"]);
}
