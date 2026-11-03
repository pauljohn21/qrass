//! The positive control for `ledger shift`: the classifier has to tell "closer" from "farther" from
//! "changed kind", and a reading that cannot be shown to move in both directions is not a reading.
//!
//! Without this, a batch could describe its own `changed` rows as progress on the strength of a tally that
//! silently counts a row going the wrong way — the mistake `tasks.md` 9.11 records (`changed=816` **不得说成
//! 进步**), and the reason the split exists as a command instead of as a habit of squinting at two listings.

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
    // The row going the wrong way is named, not just counted.
    assert_eq!(tally.farther_keys, ["to_space/srgb/lab::gray"]);
}

#[test]
fn a_row_that_changed_kind_is_never_a_distance() {
    // An honest verbatim that became a computed value: same key, different *kind* of answer.
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
    // A row whose expectation is an error message has no distance to travel, and the reason is its own
    // bucket — an answer that *became* an error must never be counted as a row that got closer.
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
    // An exponent stays inside one number, so `1e-5` and `2e-5` mask alike and align positionally.
    assert_eq!(skeleton("calc(1e-5px) calc(2e-5px)"), "calc(#px) calc(#px)");
    // A sign is part of the number, so a value that flipped its sign is a **big distance**, not a change
    // of kind — the rules call a wrong sign a wrong value, and a wrong value is something to measure.
    assert_eq!(numbers("0.1 -0.2 1e-3"), vec![0.1, -0.2, 0.001]);
    assert_eq!(skeleton("-0.2"), skeleton("0.2"));
    // `none` in the expectation and a digit in the answer is a kind difference, and masks apart.
    assert_ne!(
        skeleton("color(xyz none 0.2 0.3)"),
        skeleton("color(xyz 0.1 0.2 0.3)")
    );
}
