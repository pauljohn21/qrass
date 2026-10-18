//! `color.is-in-gamut` contract (W1 of the gamut/output round), read off the
//! `core_functions/color/is_in_gamut.hrx` goldens: legacy colors test against
//! sRGB on the 0..255 scale, Color-4 colors against their own (or `$space`)
//! channel ranges, with `none` channels never out of gamut.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn css(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n")
}

fn value(expr: &str) -> String {
    let out = css(&format!(".a {{ p: {expr}; }}"));
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

#[test]
fn is_in_gamut_legacy_and_color4() {
    // In-gamut basics.
    assert_eq!(value("color.is-in-gamut(rgb(0, 255, 0))"), "true");
    assert_eq!(value("color.is-in-gamut(#00ff00)"), "true");
    // change() stores out-of-range channels verbatim (no clamp), so 300 / -1
    // read as out of the sRGB gamut.
    assert_eq!(value("color.is-in-gamut(color.change(black, $green: 300))"), "false");
    assert_eq!(
        value("color.is-in-gamut(color.change(rgba(0 122 180 / 0.4), $red: -1))"),
        "false"
    );
    // A Color-4 hwb literal tests against the hwb channel ranges (0..100).
    assert_eq!(value("color.is-in-gamut(hwb(0 300% -1%))"), "false");
    assert_eq!(value("color.is-in-gamut(hwb(0 30% 10%))"), "true");
}
