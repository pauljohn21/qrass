//! Golden-pinned contract for the `color.mix()` / `mix()` `$method`
//! validation matrix (round 91, `color/mix/error.hrx`): the rules were read
//! off the golden input→error pairs only — non-legacy operands without a
//! method, unknown spaces, quoted/number values, the trailing-`hue` demand,
//! unknown interpolation words, and the rectangular-space rejection.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn outcome(src: &str) -> String {
    let full = format!("@use \"sass:color\"; .a {{ p: {src}; }}");
    let tokens = lex(0, "t.scss".into(), full.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), full.clone(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), full, program) {
        Ok(out) => out.join("\n"),
        Err(e) => e.message,
    }
}

#[test]
fn arity_and_method_matrix() {
    // too_few_args via the global legacy name (parameter-name driven wording)
    assert!(outcome("mix(red)").contains("Missing argument $color2."));
    // null_method/non_legacy per operand
    assert!(
        outcome("mix(lch(20% -20 0), red)")
            .contains("you must provide a $method."),
        "non-legacy color1"
    );
    // unknown one-word method space
    assert!(outcome("mix(red, blue, $method: brighter)")
        .contains("$method: Unknown color space \"brighter\"."));
    // NOTE: the `hsl/longer/hue` golden (`mix/error` `interpolation_list/
    // slash`) needs the parser to treat a slash run of idents as a list, not
    // division — it currently raises the arithmetic error before the method
    // check, tracked as residual mix/error debt.
    // quoted value rejection
    assert!(
        outcome("mix(red, blue, $method: \"hsl longer hue\")")
            .contains("$method: Expected \"hsl longer hue\" to be an unquoted string."),
        "quoted"
    );
    // trailing third word must be the literal `hue`
    assert!(
        outcome("mix(red, blue, $method: hsl longer shade)")
            .contains("$method: Expected unquoted string \"hue\" at the end of (hsl longer shade), was shade."),
        "bad tail"
    );
    // unknown interpolation word
    assert!(outcome("mix(red, blue, $method: lch longerhue)")
        .contains("$method: Unknown hue interpolation method longerhue."));
    // dropped-draft method name shares the same wording
    assert!(outcome("mix(red, blue, $method: hsl specified hue)")
        .contains("$method: Unknown hue interpolation method specified."));
    // rectangular space + hue method rejection (dart enum-name spelling)
    assert!(
        outcome("mix(red, blue, $method: srgb longer hue)").contains(
            "$method: Hue interpolation method \"HueInterpolationMethod.longer hue\" may not be set for rectangular color space srgb."
        ),
        "rectangular"
    );
}

#[test]
fn legal_methods_stay_silent() {
    // a known space alone, and polar space + method combos, must evaluate
    assert!(!outcome("mix(red, blue, $method: hsl)").contains("Unknown"));
    assert!(!outcome("mix(red, blue, $method: lch longer hue)").contains("may not be set"));
    assert!(!outcome("mix(red, blue)").contains("must provide"));
}

/// Round-92 Color4 *text* channel slots: degenerate `calc(±infinity)` literals
/// and `change` edits re-emit verbatim; `calc(NaN)` edits stay a no-op and
/// composite calcs untouched (`change/unusual_numbers` golden ladder).
#[test]
fn degenerate_channel_text_slots() {
    assert!(outcome("color.change(lab(50% 0 0), $a: calc(infinity))")
        .contains("lab(50% calc(infinity) 0)"));
    assert!(outcome("color.change(lab(50% 0 0), $a: calc(-infinity))")
        .contains("lab(50% calc(-infinity) 0)"));
    assert!(outcome("color.change(lab(50% 0 0), $a: calc(NaN))")
        .contains("lab(50% 0 0)"));
    assert!(outcome("color.change(lab(50% 20 0), $a: calc(1 + 2))")
        .contains("lab(50% 20 0)"));
    assert!(outcome("lab(50% calc(infinity) 0)").contains("calc(infinity)"));
}

/// Round-95 (OpenSpec task 2.5): the lossy `color-mix(…, black)` form of
/// `to-space` — destination lightness past its declared bounds, or an
/// `xyz`(D65)-sourced missing `y` aimed at a lightness space. L-value
/// excursions that keep lightness inside bounds print plainly.
#[test]
fn lossy_mix_form() {
    assert!(outcome("color.to-space(color(xyz 1 1 1), lab)").contains(
        "color-mix(in lab, color(xyz 1 1 1) 100%, black)"
    ));
    assert!(outcome("color.to-space(color(prophoto-rgb -1 0.4 2), lab)").contains(
        "color-mix(in lab, color(xyz"
    ));
    // lightness stays in range → plain output despite wild other channels
    assert!(!outcome("color.to-space(color(xyz -1 0.4 2), lab)")
        .contains("color-mix"));
    assert!(!outcome("color.to-space(color(srgb -1 0.4 2), oklab)").contains("color-mix"));
    // xyz D65 source with missing y heading for lab mixes; the D50 twin recovers
    // y through the adaptation matrix and prints computed.
    assert!(outcome("color.to-space(color(xyz 0.1 none 0.3), lab)").contains("color-mix"));
    assert!(!outcome("color.to-space(color(xyz-d50 0.1 none 0.3), lab)").contains("color-mix"));
    // all-missing keeps the none form, never the mix
    assert!(!outcome("color.to-space(color(xyz none none none), lab)").contains("color-mix"));
}
