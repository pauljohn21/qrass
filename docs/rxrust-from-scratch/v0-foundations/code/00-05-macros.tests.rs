//! V0-05 的断言：宏生成的三条 impl 各自可用，且 hygiene 真的成立。

use minirx_check::{
    DynSink, Tally, drain_boxed, drain_mut_ref, hygiene_probe, summed,
};

#[test]
fn hygienic_macro_does_not_capture_the_callers_name() {
    // 宏里也有一个 `let hits = 1;`：它既没读外层的 100，也没写它。
    assert_eq!(hygiene_probe(), (6, 100));
}

#[test]
fn repetition_fragment_expands_per_argument() {
    assert_eq!(summed(), 10);
}

#[test]
fn the_regular_rule_gives_the_plain_box_a_sink() {
    let boxed: Box<dyn DynSink<i32> + '_> = Box::new(Tally::default());
    assert_eq!(drain_boxed(boxed, &[1, 2, 3]), 3);
}

#[test]
fn the_send_rule_is_a_separate_expansion_and_also_works() {
    let boxed: Box<dyn DynSink<i32> + Send + '_> = Box::new(Tally::default());
    assert_eq!(drain_boxed(boxed, &[7, 7]), 2);
}

#[test]
fn the_mut_ref_rule_mutates_the_callers_variable() {
    let mut number = 41;
    let boxed: Box<dyn for<'m> DynSink<&'m mut i32> + '_> = Box::new(Tally::default());
    drain_mut_ref(boxed, &mut number);
    assert_eq!(number, 42, "元素是 `&mut i32` 的那条宏规则确实生效");
}
