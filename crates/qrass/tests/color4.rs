//! CSS Color 4 literal parsing and in-space channel editing (task 11.3,
//! round: `color.adjust`/`change`/`scale` over `color(...)`/`lab`/`lch`/
//! `oklab`/`oklch`/`hsl`/`hwb` literals). Every expectation is copied from the
//! dart-sass `sass-spec` golden output for the same input.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn out(scss: &str) -> String {
    let tokens = lex(0, "t.scss".into(), scss.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), scss.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), scss.into(), program)
        .expect("snippet must compile")
        .join("\n")
}

#[test]
fn a98_rgb_adjust_unitless() {
    let css = out("@use \"sass:color\";\na {b: color.adjust(color(a98-rgb 0.2 0.5 0.7), $red: 0.5)}");
    assert!(css.contains("color(a98-rgb 0.7 0.5 0.7)"), "{css}");
}

#[test]
fn a98_rgb_adjust_percent() {
    let css = out("@use \"sass:color\";\na {b: color.adjust(color(a98-rgb 0.2 0.5 0.7), $red: -10%)}");
    assert!(css.contains("color(a98-rgb 0.1 0.5 0.7)"), "{css}");
}

#[test]
fn adjust_is_not_clamped_out_of_gamut() {
    let hi = out("@use \"sass:color\";\na {b: color.adjust(color(a98-rgb 0.2 0.5 0.7), $red: 0.9)}");
    assert!(hi.contains("color(a98-rgb 1.1 0.5 0.7)"), "{hi}");
    let lo = out("@use \"sass:color\";\na {b: color.adjust(color(a98-rgb 0.2 0.5 0.7), $red: -0.3)}");
    assert!(lo.contains("color(a98-rgb -0.1 0.5 0.7)"), "{lo}");
}

#[test]
fn adjust_alpha_emits_inside_parens() {
    let css = out("@use \"sass:color\";\na {b: color.adjust(color(a98-rgb 0.2 0.5 0.7), $red: 0.5, $alpha: -0.1)}");
    assert!(css.contains("color(a98-rgb 0.7 0.5 0.7 / 0.9)"), "{css}");
}

#[test]
fn powerless_none_adjusts_from_channel_floor() {
    let css = out("@use \"sass:color\";\na {b: color.adjust(color(a98-rgb 0.2 none 0.7), $green: 0.3)}");
    assert!(css.contains("color(a98-rgb 0.2 0.3 0.7)"), "{css}");
}

#[test]
fn lab_change_percent_lightness_and_raw_a() {
    let css = out("@use \"sass:color\";\na {b: color.change(lab(50% 0.7 0.7), $lightness: 60%, $a: -0.5)}");
    assert!(css.contains("lab(60% -0.5 0.7)"), "{css}");
}

#[test]
fn hue_edits_wrap_mod_360() {
    let css = out("@use \"sass:color\";\na {b: color.adjust(oklch(50% 0.1 350deg), $hue: 20deg)}");
    assert!(css.contains("oklch(50% 0.1 10deg)"), "{css}");
}

#[test]
fn scale_uses_channel_range_for_relative_growth() {
    let css = out("@use \"sass:color\";\na {b: color.scale(color(display-p3 0.2 0.5 0.7), $red: 100%)}");
    assert!(css.contains("color(display-p3 1 0.5 0.7)"), "{css}");
}

#[test]
fn hsl_and_hwb_literals_parse_with_units() {
    let css = out("a {b: hsl(120deg 50% 50% / 50%); c: hwb(197deg 0% 50%)}");
    assert!(css.contains("hsl(120deg 50% 50% / 0.5)"), "{css}");
    assert!(css.contains("hwb(197deg 0% 50%)"), "{css}");
}

#[test]
fn legacy_comma_forms_keep_old_serialization() {
    let css = out("a {b: hsl(120, 50%, 50%); c: rgb(13, 110, 253)}");
    assert!(css.contains("hsl(120, 50%, 50%)"), "{css}");
    assert!(css.contains("rgb(13, 110, 253)"), "{css}");
}

#[test]
fn unknown_color_space_stays_plain_css() {
    let css = out("a {b: color(made-up-space 1 2 3)}");
    assert!(css.contains("color(made-up-space 1 2 3)"), "{css}");
}

#[test]
fn xyz_literal_parses_and_serializes() {
    let css = out("a {b: color(xyz 0.5 0.4 0.3)}");
    assert!(css.contains("color(xyz 0.5 0.4 0.3)"), "{css}");
}
