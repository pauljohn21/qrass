//! `ledger shift` 的阳性对照：分类器必须能分辨「更近」「更远」与「换了种类」，而一个无法被证明会
//! 双向移动的读数不算读数。
//!
//! 没有它，一批可以凭一份统计把自己 `changed` 里的行说成进步，而那统计悄悄数着反向行走的行——
//! 那正是 `tasks.md` 9.11 记下的错误（`changed=816` **不得说成进步**），也是这个拆分作为一条命令
//! 存在（而不是作为眯眼比对两份清单的习惯）的理由。

#[path = "../src/shift_impl.rs"]
mod shift_impl;

use crate::shift_impl::{Reason, Verdict};
use crate::shift_impl::{judge, numbers, skeleton, split_of};

fn line(key: &str, got_a: &str, got_b: &str, want: &str) -> String {
    format!("{key} | a: got \"{got_a}\" want \"{want}\" | b: got \"{got_b}\" want \"{want}\"")
}

#[test]
fn a_smaller_distance_counts_as_closer_and_a_larger_one_as_farther() {
    let closer = line(
        "to_space/srgb/lab::middle",
        "a { b: lab(44.1210% 10.95 -59.08); }",
        "a { b: lab(44.12101% 10.954 -59.085); }",
        "a { b: lab(44.1210142293% 10.9543336098 -59.0857381981); }",
    );
    let farther = line(
        "to_space/srgb/lab::gray",
        "a { b: lab(53.389% 0.0001 0); }",
        "a { b: lab(53.389% 0.0002 0); }",
        "a { b: lab(53.3880481926% 0 0); }",
    );
    assert!(
        matches!(judge(&closer), Verdict::Distance { old, new } if new < old),
        "closer row judged as {:?}",
        judge(&closer)
    );
    assert!(
        matches!(judge(&farther), Verdict::Distance { old, new } if new > old),
        "farther row judged as {:?}",
        judge(&farther)
    );

    let tally = split_of(&[closer, farther]);
    assert_eq!(
        (
            tally.closer,
            tally.farther,
            tally.tie,
            tally.error_expectation,
            tally.shape_of_text
        ),
        (1, 1, 0, 0, 0),
        "got {tally:?}"
    );
    // 反向行走的那行被点名，而不只是被计数。
    assert_eq!(tally.farther_keys, ["to_space/srgb/lab::gray"]);
}

#[test]
fn a_row_that_changed_kind_is_never_a_distance() {
    // 一个诚实的逐字行变成了算出的值：同一 key，*种类*不同的答案。
    let verbatim_to_value = line(
        "to_space/xyz_d50/srgb::missing/red",
        "a { b: color.to-space(color(xyz-d50 none 0.2 0.3), srgb); }",
        "a { b: color(srgb -0.7157 0.6601 0.6467); }",
        "a { b: color(srgb none 0.6601216969 0.6467153345); }",
    );
    assert_eq!(
        judge(&verbatim_to_value),
        Verdict::Shape(Reason::ShapeOfText),
        "a verbatim that became a value is not closer"
    );
    // 期望是错误消息的行没有距离可走，其理由有自己的桶——一个*变成*错误的答案绝不得被数成更近的一行。
    let expects_error = "to_gamut/space::error/unknown | a: expected error \"Unknown gamut method.\", \
                        got \"a { b: 1; }\" | b: expected error \"Unknown gamut method.\", got \"a { b: 2; }\"";
    assert_eq!(
        judge(expects_error),
        Verdict::Shape(Reason::ErrorExpectation)
    );
    let tally = split_of(&[verbatim_to_value, expects_error.to_string()]);
    assert_eq!(
        (
            tally.closer,
            tally.farther,
            tally.tie,
            tally.error_expectation,
            tally.shape_of_text
        ),
        (0, 0, 0, 1, 1),
        "got {tally:?}"
    );
    assert_eq!(tally.error_keys, ["to_gamut/space::error/unknown"]);
    assert_eq!(tally.shape_keys, ["to_space/xyz_d50/srgb::missing/red"]);
}

#[test]
fn the_mask_keeps_the_text_and_forgets_only_the_digits() {
    assert_eq!(
        skeleton("color(xyz-d50 0.5103421984 0.5014942183)"),
        "color(xyz-d# # #)"
    );
    // 指数留在同一个数里，所以 `1e-5` 与 `2e-5` 一样掩掉、按位置对齐。
    assert_eq!(skeleton("calc(1e-5px) calc(2e-5px)"), "calc(#px) calc(#px)");
    // 符号是数的一部分，所以一个翻符号的值是**大距离**，不是种类变化——规则把错符号叫错值，而错值就是可量的东西。
    assert_eq!(numbers("0.1 -0.2 1e-3"), vec![0.1, -0.2, 0.001]);
    assert_eq!(skeleton("-0.2"), skeleton("0.2"));
    // 期望里的 `none` 与答案里的数字是种类差异，掩码也把它们分开。
    assert_ne!(
        skeleton("color(xyz none 0.2 0.3)"),
        skeleton("color(xyz 0.1 0.2 0.3)")
    );
}
