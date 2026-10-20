//! Legacy channel-editing contract tests for `color.change`/`adjust`/`scale`
//! (round 78, `core_functions/color/change/rgb.hrx` + `scale/rgb`/`adjust/rgb`
//! low sections). Rules read off the golden tables: the named channel setters
//! ($red/$green/$blue/$alpha kwargs) actually edit, out-of-range values fall
//! back to the `hsl(…)` expression, `none` makes the color serialize as its
//! Color-4 `rgb(…)` spelling, and fractional channels keep the `rgb(%…)`
//! form rather than snapping to a keyword.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn value(expr: &str) -> String {
    let src = format!("@use \"sass:color\"; .a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src, program)
        .expect("eval ok")
        .join("\n");
    out.split_once("p: ")
        .map(|x| x.1.split_once(";\n").map(|y| y.0).unwrap_or(x.1))
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[test]
fn change_rgb_channel_edits() {
    // integer channels re-spell as keyword/hex (`change/rgb` red/min…)
    assert_eq!(value("color.change(red, $red: 0)"), "black");
    assert_eq!(value("color.change(red, $red: 100)"), "#640000");
    assert_eq!(value("color.change(black, $green: 255)"), "lime");
    assert_eq!(value("color.change(black, $red: 12, $green: 24, $blue: 48)"), "#0c1830");
    assert_eq!(
        value("color.change(rgba(black, 0.3), $red: 12, $green: 24, $blue: 48)"),
        "rgba(12, 24, 48, 0.3)"
    );
}

#[test]
fn change_rgb_out_of_range_and_none() {
    // a channel beyond the 0..255 byte domain falls back to the HSL expression
    assert_eq!(
        value("color.change(black, $red: 500)"),
        "hsl(0, 5000%, 98.0392156863%)"
    );
    assert_eq!(
        value("color.change(black, $green: -50)"),
        "hsl(120, 100%, -9.8039215686%)"
    );
    // `none` switches to the Color-4 space-separated spelling
    assert_eq!(value("color.change(black, $red: none)"), "rgb(none 0 0)");
    assert_eq!(value("color.change(black, $green: none)"), "rgb(0 none 0)");
}

#[test]
fn adjust_and_scale_rgb_edits_serialize_by_shape() {
    // integer edits re-spell as hex (`adjust/rgb` `red/low`)
    assert_eq!(value("color.adjust(red, $red: -100)"), "#9b0000");
    // fractional channels keep the `rgb(%…)` form, not a keyword snap
    assert_eq!(
        value("color.scale(lightcoral, $red: -33%)"),
        "rgb(63.0588235294%, 50.1960784314%, 50.1960784314%)"
    );
}
