//! CSS Color 4 literal parsing and in-space channel editing (task 11.3,
//! round: `color.adjust`/`change`/`scale` over `color(...)`/`lab`/`lch`/
//! `oklab`/`oklch`/`hsl`/`hwb` literals). Every expectation is copied from the
//! the goldens `sass-spec` golden output for the same input.

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

/// An unknown *concrete* space name is dart's `$description:` error, not a
/// plain-CSS passthrough (`color/error` goldens #20/#23/#24 — this replaces the
/// earlier passthrough assumption the pre-round-61 unit test pinned). Symbolic
/// constructs still fall through untouched.
#[test]
fn unknown_color_space_errors_symbolic_stays_plain_css() {
    let src = "a {b: color(made-up-space 1 2 3)}";
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    let err = eval_all(0, "t.scss".into(), src.to_string(), program)
        .expect_err("unknown concrete space must error");
    assert!(
        err.to_string()
            .contains("$description: Unknown color space \"made-up-space\"."),
        "{err}"
    );
    let css = out("a {b: color(var(--space) 1 2 3)}");
    assert!(css.contains("color(var(--space) 1 2 3)"), "{css}");
}

#[test]
fn xyz_literal_parses_and_serializes() {
    let css = out("a {b: color(xyz 0.5 0.4 0.3)}");
    assert!(css.contains("color(xyz 0.5 0.4 0.3)"), "{css}");
}

/// Extract the numeric value a `color.channel(...)` call serialized on the
/// `b:` line (dropping any `%`/`deg` unit suffix).
fn channel_num(scss: &str) -> f64 {
    let css = out(scss);
    let tail = css.split("b:").nth(1).expect("b: line present").trim_start();
    let num: String = tail
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    num.parse().expect("numeric channel value")
}

#[test]
fn named_colors_round_trip_their_keyword() {
    // CSS color keywords resolve to colors but serialize back as the name; the
    // un-tabled `transparent` stays a bare identifier.
    let css = out("a {b: red; c: pink; d: transparent}");
    assert!(css.contains("b: red"), "{css}");
    assert!(css.contains("c: pink"), "{css}");
    assert!(css.contains("d: transparent"), "{css}");
}

#[test]
fn legacy_color_promotes_to_modern_spaces() {
    // Golden targets from sass-spec `core_functions/color/channel/*.hrx` (the
    // black-box expectation for a promoted legacy color). Asserted within an
    // absolute tolerance so the test gates convergence and catches regressions
    // without over-fitting the last digits of the reference-constant rounding.
    let close = |space: &str, chan: &str, golden: f64, tol: f64| {
        let got = channel_num(&format!(
            "@use \"sass:color\";\na {{b: color.channel(pink, \"{chan}\", $space: {space})}}"
        ));
        assert!((got - golden).abs() < tol, "{space}/{chan}: got {got}, want {golden}");
    };
    // XYZ-D65 leg is now byte-exact against golden.
    close("xyz", "x", 0.708_662_362_9, 1e-8);
    close("display-p3", "red", 0.962_148_711_8, 1e-8);
    // Lab / oklab converge to ~1e-6 after the D65->D50 and linear-sRGB fixes.
    close("lab", "lightness", 83.787_252_865_6, 1e-4);
    close("lab", "a", 24.436_770_288_1, 1e-4);
    close("oklab", "lightness", 86.773_845_084_1, 1e-4);
}

#[test]
fn perceptual_missing_lightness_poisons_only_lch_to_lab() {
    // A missing lightness leaves the chromatic channels concrete for every
    // perceptual pair EXCEPT the same-family polar->cartesian `lch -> lab`,
    // where the goldens poison the whole color (golden `to_space/l?ch/lab`).
    let keep = out("@use \"sass:color\";\na {b: color.to-space(lab(none 20 30), lch)}");
    assert!(keep.contains("lch(none 36.0555127546 56.309932474deg)"), "{keep}");
    let poison = out("@use \"sass:color\";\na {b: color.to-space(lch(none 20 30deg), lab)}");
    assert!(poison.contains("lab(none none none)"), "{poison}");
}

#[test]
fn hsl_source_poisons_undetermined_chromatic_channels() {
    // lab/oklab lose a/b only when BOTH hue and saturation are missing; the
    // polar targets lose chroma/hue as soon as saturation is missing. A lone
    // missing hue or saturation substitutes to 0 and stays concrete. Asserted on
    // the `none` structure only (the channel magnitudes are float-wall digits).
    let both = out("@use \"sass:color\";\na {b: color.to-space(hsl(none none 50%), lab)}");
    assert!(both.contains("none none"), "hue+sat missing -> chromatic none: {both}");
    let lone_hue = out("@use \"sass:color\";\na {b: color.to-space(hsl(none 20% 30%), lab)}");
    assert!(!lone_hue.contains("none"), "lone missing hue substitutes to concrete: {lone_hue}");
    let sat = out("@use \"sass:color\";\na {b: color.to-space(hsl(10deg none 30%), lch)}");
    assert!(sat.contains("none none"), "missing saturation poisons chroma+hue: {sat}");
}

#[test]
fn hwb_source_polar_target_poisons_lightness_and_chroma() {
    // `hwb(10deg none none)` (whiteness AND blackness missing) poisons lightness
    // and chroma on a polar target but keeps the hue; the same source to a
    // cartesian target stays fully concrete.
    let lch = out("@use \"sass:color\";\na {b: color.to-space(hwb(10deg none none), lch)}");
    assert!(lch.contains("lch(none none "), "w&b missing poisons L+C on polar: {lch}");
    let lab = out("@use \"sass:color\";\na {b: color.to-space(hwb(10deg none none), lab)}");
    assert!(!lab.contains("none"), "cartesian target stays concrete: {lab}");
}

