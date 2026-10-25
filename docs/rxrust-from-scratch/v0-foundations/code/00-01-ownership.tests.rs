//! V0-01 的断言：证明"推/拉得到同样结果"、早停真的早停、被消耗的对象拿不回结果。

use minirx_check::{Counter, Sink, pull_sum, push_all, push_names, push_until, run};

#[test]
fn pull_and_push_agree_on_the_same_numbers() {
    let owned = vec![1, 2, 3, 4];

    let mut sink = Vec::new();
    push_all(owned.clone(), &mut sink);
    assert_eq!(sink, vec![1, 2, 3, 4]);

    assert_eq!(pull_sum(&owned), 10);
}

#[test]
fn push_moves_the_whole_vec_so_the_caller_no_longer_owns_it() {
    // 取消下一行的注释，编译器会告诉你 `owned` 已被搬走、不能再在这里用。
    // let moved = vec![1, 2, 3];
    // let mut into = Vec::new();
    // push_all(moved, &mut into);
    // assert_eq!(moved.len(), 3); // ← 编译错：borrow of moved value
}

#[test]
fn early_stop_reports_exactly_how_many_were_emitted() {
    let mut sink = Vec::new();
    let emitted = push_until(vec![1, 2, 3, 4, 5], 2, &mut sink);
    assert_eq!((emitted, sink.as_slice()), (2, &[1, 2][..]));
}

#[test]
fn each_string_ends_up_owned_by_the_sink() {
    let mut sink = Vec::new();
    push_names(vec!["a".to_string(), "b".to_string()], &mut sink);
    assert_eq!(sink, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn the_limit_makes_the_source_stop_early() {
    assert_eq!(run(vec![1, 2, 3, 4, 5], 3), 6);
    assert_eq!(run(vec![1, 2, 3, 4, 5], 99), 15);
}

#[test]
fn finish_consumes_the_sink_and_still_hands_back_its_result() {
    let mut sink = Counter::new(2);
    sink.receive(5);
    sink.receive(5);
    assert!(sink.closed());
    // `finish(self)` 消耗接收者：这一行之后 `sink` 不复存在，无法再 `receive`。
    assert_eq!(sink.finish(), 10);
}
