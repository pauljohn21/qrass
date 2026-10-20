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

/// Round-85 c4 branch (`scale/error/*` + `adjust/error/*` on Color 4 bases).
#[test]
fn c4_channel_validation_ladder() {
    // domain names report the color's own space (`incompatible_channel`)
    assert!(
        err("color.scale(color(srgb 0.2 0.5 0.7), $whiteness: 50%)")
            .contains("$whiteness: Color space srgb doesn't have a channel with this name."),
        "domain spelling"
    );
    // scale demands percent units (`units/a98_rgb`)
    assert!(err("color.scale(color(a98-rgb 0.2 0.5 0.7), $green: 1)")
        .contains("$green: Expected 1 to have unit \"%\"."));
    // scale bounds (`bounds` modern/*)
    assert!(err("color.scale(lab(50% -70 60), $a: -100.001%)")
        .contains("$a: Expected -100.001% to be within -100% and 100%."));
    // scaling a missing channel raises the guard; adjusting it floors
    assert!(err("color.scale(lab(none 0 0), $lightness: 10%, $space: lab)")
        .contains("$lightness: Because the CSS working group is still deciding"));
    // `$alpha: none` is a type error, not a missing write
    assert!(err("color.scale(red, $alpha: none)").contains("$alpha: none is not a number."));
    // $space quoted rejection
    assert!(err("color.scale(red, $space: \"lab\", $saturation: 10%)")
        .contains("$space: Expected \"lab\" to be an unquoted string."));
    // two positional arguments are rejected wholesale
    assert!(err("color.scale(red, 1)").contains(
        "Only one positional argument is allowed. All other arguments must be passed by name."
    ));
}

/// Round-86 unit ladder for adjust/change (`adjust/error/units/*`).
#[test]
fn unit_ladder_adjust_change() {
    // numeric channels accept `%` or unitless only
    assert!(
        err("color.adjust(color(a98-rgb 0.2 0.5 0.7), $green: 0.5px)")
            .contains("$green: Expected 0.5px to have unit \"%\" or no units.")
    );
    // percent-only channels reject foreign units (lch chroma is numeric per
    // `units/lch`; lab a/b take `%`-or-no-units per `units/lab`)
    assert!(
        err("color.adjust(lab(50% 30 -50), $a: 20px)")
            .contains("$a: Expected 20px to have unit \"%\" or no units.")
    );
    // hue demands an angle unit
    assert!(err("color.adjust(oklch(50% 0.3 50deg), $hue: 20%)")
        .contains("$hue: Expected 20% to have an angle unit (deg, grad, rad, turn)."));
    // legacy hwb-family channels are percent-only in every mode
    assert!(err("color.adjust(black, $blackness: 1)")
        .contains("$blackness: Expected 1 to have unit \"%\"."));
    // legal spellings stay silent
    assert!(err("color.adjust(color(a98-rgb 0.2 0.5 0.7), $green: 10%)").is_empty());
    assert!(err("color.adjust(oklch(50% 0.3 50deg), $hue: 20deg)").is_empty());
    assert!(err("color.adjust(oklch(50% 0.3 50deg), $hue: 20)").is_empty());
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
