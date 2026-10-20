//! Channel-domain validation for the legacy `adjust`/`scale`/`change` trio
//! (round 84, `scale/error/*` + `adjust/error/*` + `change/error/*` goldens).
//! The error spellings and the ONE-space domain rule (mixed-format edits
//! report the space the first-family channel binds) are pinned here, together
//! with the golden-punned exemptions: unknown units pass through taking the
//! magnitude, and `none` edits belong to the missing-slot path.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn err(expr: &str) -> String {
    let src = format!("@use \"sass:color\"; .a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src, program)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

#[test]
fn domain_binding_and_channel_errors() {
    assert!(err("color.scale(red, $green: 1%, $saturation: 1%)")
        .contains("$saturation: Color space rgb doesn't have a channel with this name."));
    assert!(err("color.scale(red, $saturation: 1%, $blackness: 1%)")
        .contains("$blackness: Color space hsl doesn't have a channel with this name."));
    assert!(err("color.adjust(red, $ambience: 10%)")
        .contains("$ambience: Color space rgb doesn't have a channel with this name."));
    assert!(
        err("color.scale(red, $red: c)").contains("$red: c is not a number."),
        "type error"
    );
    assert!(err("color.scale(red, $hue: 10%)").contains("$hue: Channel isn't scalable."));
    assert!(err("color.scale(red, $whiteness: 100.001%)")
        .contains("$whiteness: Expected 100.001% to be within -100% and 100%."));
}

#[test]
fn golden_exemptions_stay_valid_edits() {
    // `adjust/units` unknown/*: magnitude passes, unit ignored (round-69
    // adjust-hue semantics) — these must NOT become validation errors.
    assert!(err("color.adjust(red, $hue: 60in)").is_empty());
    assert!(err("color.adjust(red, $lightness: 10in)").is_empty());
    // `none` edits belong to the missing-slot path, not a type error
    assert!(err("color.change(black, $red: none)").is_empty());
    // plain valid edits across all spaces stay silent
    assert!(err("color.scale(red, $red: 50%)").is_empty());
    assert!(err("color.adjust(red, $alpha: -0.3)").is_empty());
    assert!(err("color.change(red, $green: 5)").is_empty());
}
