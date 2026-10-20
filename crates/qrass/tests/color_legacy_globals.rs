//! Global hyphenated legacy color functions (round 69): the behavior table is
//! read entirely off the `core_functions/color/{lighten,darken,saturate,
//! desaturate,fade_in,fade_out,adjust_hue}.hrx` goldens — HSL arithmetic, the
//! percent/keyword/hex serialization forms, `$amount` bounds, the
//! CSS-filter `saturate` overload, and the Color-4 rejection spelling.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn out(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n")
}

/// The declared value of `a { p: <expr> }`.
fn value(expr: &str) -> String {
    let out = out(&format!("a {{ p: {expr}; }}"));
    let rest = out
        .split_once("p: ")
        .unwrap_or_else(|| panic!("no declaration emitted for {expr:?}: {out}"))
        .1;
    rest.split_once(";\n")
        .map(|x| x.0)
        .unwrap_or(rest)
        .trim()
        .to_string()
}

fn err(expr: &str) -> String {
    let src = format!("a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src, program)
        .err()
        .unwrap_or_else(|| panic!("{expr} must fail"))
        .to_string()
}

#[test]
fn lightness_and_saturation_adjust_hsl_in_percentage_points() {
    // golden: lighten.hrx middle / fraction / max / min / keyword forms
    assert_eq!(value("lighten(red, 14%)"), "rgb(100%, 28%, 28%)");
    assert_eq!(value("lighten(red, 0.5%)"), "rgb(100%, 1%, 1%)");
    assert_eq!(value("lighten(red, 50%)"), "white");
    assert_eq!(value("lighten(red, 0%)"), "red");
    assert_eq!(value("darken(red, 50%)"), "black");
    assert_eq!(value("darken(red, 14%)"), "rgb(72%, 0%, 0%)");
    // alpha is preserved through the HSL rebuild
    assert_eq!(value("lighten(rgba(red, 0.4), 100%)"), "rgba(255, 255, 255, 0.4)");
    assert_eq!(value("darken(rgba(red, 0.2), 100%)"), "rgba(0, 0, 0, 0.2)");
    // named arguments (golden `named` sections)
    assert_eq!(value("lighten($color: red, $amount: 14%)"), "rgb(100%, 28%, 28%)");
    // exact-integer results restore keyword / hex spellings
    assert_eq!(value("saturate(plum, 100%)"), "#ff7eff");
    assert_eq!(value("desaturate(plum, 0%)"), "plum");
}

#[test]
fn amount_bounds_and_type_wording() {
    assert!(err("lighten(red)").contains("Missing argument $amount."));
    assert!(
        err("lighten(red, 1%, 2)")
            .contains("Only 2 arguments allowed, but 3 were passed.")
    );
    assert!(err("lighten(1, 2)").contains("$color: 1 is not a color."));
    assert!(err("lighten(red, blue)").contains("$amount: blue is not a number."));
    assert!(
        err("lighten(red, -0.001)").contains("$amount: Expected -0.001 to be within 0 and 100.")
    );
    assert!(
        err("lighten(red, 100.001)").contains("Expected 100.001 to be within 0 and 100.")
    );
    // the alpha family is 0..1 and rejects percentages outright (#1745 golden)
    assert!(err("fade-in(red, -0.001)").contains("Expected -0.001 to be within 0 and 1."));
    assert!(err("fade-in(red, 50%)").contains("Expected 50% to be within 0 and 1."));
}

#[test]
fn color_four_inputs_are_rejected_by_name() {
    let msg = err("lighten(color(srgb 0 0 0), 10%)");
    assert!(
        msg.contains("lighten() is only supported for legacy colors. Please use color.adjust() instead with an explicit $space argument."),
        "{msg}"
    );
    assert!(err("desaturate(color(srgb 1 1 1), 10%)").contains("desaturate() is only supported"));
}

#[test]
fn fade_family_moves_alpha_and_serializes_integral() {
    assert_eq!(value("fade-in(rgba(red, 0.5), 0.5)"), "red");
    assert_eq!(value("fade-in(rgba(red, 0.5), 0.14)"), "rgba(255, 0, 0, 0.64)");
    assert_eq!(value("fade-out(rgba(red, 0.5), 0.5)"), "rgba(255, 0, 0, 0)");
    // opacify/transparentize are direct aliases
    assert_eq!(
        value("opacify($color: rgba(red, 0.5), $amount: 0.14)"),
        "rgba(255, 0, 0, 0.64)"
    );
    assert_eq!(value("transparentize(rgba(red, 0.5), 0.5)"), "rgba(255, 0, 0, 0)");
}

#[test]
fn saturate_css_filter_overload() {
    // lone non-color arguments re-emit the plain spelling (saturate.hrx
    // css_overload sections)…
    assert_eq!(value("saturate(50%)"), "saturate(50%)");
    assert_eq!(value("saturate(1)"), "saturate(1)");
    assert_eq!(value("saturate(var(--c))"), "saturate(var(--c))");
    assert_eq!(value("saturate($amount: 50%)"), "saturate(50%)");
    // …a lone color is the Sass spelling's amount type error, and two
    // arguments adjust saturation for real.
    assert!(err("saturate(red)").contains("$amount: red is not a number."));
    assert!(err("saturate()").contains("Missing argument $amount."));
    assert_eq!(
        value("saturate(plum, 14%)"),
        "rgb(90.2078431373%, 59.2039215686%, 90.2078431373%)"
    );
}

#[test]
fn adjust_hue_rotates_with_angle_units() {
    assert_eq!(value("adjust-hue(red, 60)"), "yellow");
    assert_eq!(value("adjust-hue(red, 60deg)"), "yellow");
    // `units` goldens: an unknown unit behaves like the number, radians convert
    assert_eq!(value("adjust-hue(red, 60in)"), "yellow");
    assert_eq!(
        value("adjust-hue(red, 60rad)"),
        "rgb(0%, 70.4220486918%, 100%)"
    );
    assert!(err("adjust-hue(red)").contains("Missing argument $degrees."));
    assert!(err("adjust-hue(red, blue)").contains("$degrees: blue is not a number."));
}

/// Round-75 (`color/complement.hrx`): the `$space` working-space grammar and
/// the modern same-space rotation.
#[test]
fn complement_space_grammar() {
    // legacy colors rotate in hsl-equivalent targets unchanged
    assert_eq!(value("color.complement(red, hwb)"), "aqua");
    assert_eq!(value("color.complement($color: red, $space: hwb)"), "aqua");
    // a non-polar or unknown `$space` fails before the color's kind matters
    let msg = err("color.complement(red, xyz)");
    assert!(
        msg.contains("$space: Color space xyz doesn't have a hue channel."),
        "{msg}"
    );
    // c4 rotated in its own space re-emits that space (missing non-hue
    // channels survive: `hsl(0deg 50% none)` → `hsl(180deg 50% none)`)
    assert_eq!(
        value("color.complement(hsl(0deg 50% none))"),
        "hsl(180deg 50% none)"
    );
    // a missing hue in the working space is dart's guard, spelling the color
    let msg = err("color.complement(hsl(none 30% 40%))");
    assert!(
        msg.contains("$hue: Because the CSS working group is still deciding")
            && msg.contains("(color: hsl(none 30% 40%))"),
        "{msg}"
    );
    assert!(
        err("color.complement(red, $space: null)").contains("$space: null is not a string.")
    );
    // global legacy spelling (no CSS-filter twin)
    assert_eq!(value("complement(turquoise)"), "#e04050");
}
