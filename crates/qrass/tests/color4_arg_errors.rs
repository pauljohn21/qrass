//! Modern Color 4 constructor argument validation (task 11.3, color round W4a):
//! a concrete-but-wrong channel in `lab()/lch()/oklab()/oklch()/hsl()/hwb()`
//! raises dart's `$<channel>:` / `$channels:` messages, while symbolic `var()`
//! channels keep falling through to plain-CSS passthrough.

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

fn eval_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src.into(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn color4_wrong_unit_channel_errors() {
    let msg = eval_err(".a { p: lab(1px 2 3); }");
    assert!(
        msg.contains("$lightness: Expected 1px to have unit \"%\" or no units."),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab(1% 2px 3); }");
    assert!(
        msg.contains("$a: Expected 2px to have unit \"%\" or no units."),
        "{msg}"
    );
}

#[test]
fn color4_non_number_channel_errors() {
    let msg = eval_err(".a { p: lab(c 2 3); }");
    assert!(
        msg.contains("$channels: Expected lightness channel to be a number, was c."),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab(1% 2 c); }");
    assert!(
        msg.contains("$channels: Expected b channel to be a number, was c."),
        "{msg}"
    );
}

#[test]
fn color4_symbolic_channel_still_passes_through() {
    // A var() channel must remain plain-CSS passthrough (no color error).
    let out = css(".a { p: lab(var(--x) 2 3); }");
    assert!(out.contains("lab(var(--x) 2 3)"), "{out}");
}

// ─── shape / count / arity guards (round 60, lab/lch/oklab/oklch) ─────────────

#[test]
fn color4_list_shape_errors() {
    let msg = eval_err(".a { p: lab([1% 2 3]); }");
    assert!(
        msg.contains("$channels: Expected an unbracketed list, was [1% 2 3]"),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab((1%, 2, 3)); }");
    assert!(
        msg.contains("$channels: Expected a space- or slash-separated list, was (1%, 2, 3)"),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab(()); }");
    assert!(
        msg.contains("$channels: Color component list may not be empty."),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab(list.slash(1%, 2, 3)); }");
    assert!(
        msg.contains("$channels: Only 2 slash-separated elements allowed, but 3 were passed."),
        "{msg}"
    );
}

#[test]
fn color4_channel_count_errors() {
    let msg = eval_err(".a { p: lab(1% 2); }");
    assert!(
        msg.contains("$channels: The lab color space has 3 channels but (1% 2) has 2."),
        "{msg}"
    );
    let msg = eval_err(".a { p: lch(1% 2 3deg 0.4); }");
    assert!(
        msg.contains("$channels: The lch color space has 3 channels but (1% 2 3deg 0.4) has 4."),
        "{msg}"
    );
    // Symbolic channels keep the passthrough even at the wrong count.
    let out = css(".a { p: lab(var(--x) var(--y)); }");
    assert!(out.contains("lab(var(--x) var(--y))"), "{out}");
}

#[test]
fn color4_arity_errors() {
    let msg = eval_err(".a { p: lab(); }");
    assert!(msg.contains("Missing argument $channels."), "{msg}");
    let msg = eval_err(".a { p: oklab(1%, 2, 3, 0.4); }");
    assert!(
        msg.contains("Only 1 argument allowed, but 4 were passed."),
        "{msg}"
    );
}

#[test]
fn color4_hue_and_alpha_validation() {
    let msg = eval_err(".a { p: lch(1% 2 3px); }");
    assert!(
        msg.contains("$hue: Expected 3px to have an angle unit (deg, grad, rad, turn)."),
        "{msg}"
    );
    let msg = eval_err(".a { p: lab(1% 2 3 / 0.4px); }");
    assert!(
        msg.contains("$alpha: Expected 0.4px to have unit \"%\" or no units."),
        "{msg}"
    );
    // A bare string in the alpha slot reports the `$channels:` prefix (golden).
    let msg = eval_err(".a { p: lab(1% 2 3 / c); }");
    assert!(msg.contains("$channels: c is not a number."), "{msg}");
    // `%` and unitless alphas are fine, also through the list.slash wrapper.
    let out = css(".a { p: lab(list.slash(1% 2 3, 25%)); }");
    assert!(out.contains("lab(1% 2 3 / 0.25)"), "{out}");
}

#[test]
fn relative_color_syntax_still_passes_through() {
    // The shape guards must not claim relative color syntax (regression pin for
    // the first guard cut: `from` / case-mutated `From` / list.slash wrapper).
    let out = css(".a { p: lab(from #aaa l a b); }");
    assert!(out.contains("lab(from"), "{out}");
    let out = css(".a { p: lab(From #aaa l a b); }");
    assert!(out.contains("lab(From"), "{out}");
    let out = css(".a { p: lab(list.slash(from #aaa r g b, 25%)); }");
    assert!(out.contains("list.slash") || out.contains("lab(from"), "{out}");
}

// ─── color() wrapper guards (round 61, $description: prefix) ─────────────────

#[test]
fn color_wrapper_shape_errors() {
    let msg = eval_err(".a { p: color([srgb 0.1 0.2 0.3]); }");
    assert!(
        msg.contains("$description: Expected an unbracketed list, was [srgb 0.1 0.2 0.3]"),
        "{msg}"
    );
    let msg = eval_err(".a { p: color((srgb, 0.1, 0.2, 0.3)); }");
    assert!(
        msg.contains("$description: Expected a space- or slash-separated list, was (srgb, 0.1, 0.2, 0.3)"),
        "{msg}"
    );
    let msg = eval_err(".a { p: color(list.slash(srgb 0.1, 0.2, 0.3)); }");
    assert!(
        msg.contains("$description: Only 2 slash-separated elements allowed, but 3 were passed."),
        "{msg}"
    );
    let msg = eval_err(".a { p: color(); }");
    assert!(msg.contains("Missing argument $description."), "{msg}");
}

#[test]
fn color_wrapper_space_and_count_errors() {
    let msg = eval_err(".a { p: color(foo 1 2 3); }");
    assert!(
        msg.contains("$description: Unknown color space \"foo\"."),
        "{msg}"
    );
    let msg = eval_err(r#".a { p: color("from" #aaa srgb r g b); }"#);
    assert!(
        msg.contains("$description: Expected \"from\" to be an unquoted string."),
        "{msg}"
    );
    let msg = eval_err(".a { p: color(1 2 3); }");
    assert!(msg.contains("$description: 1 is not a string."), "{msg}");
    let msg = eval_err(".a { p: color(srgb); }");
    assert!(
        msg.contains("$description: The srgb color space has 3 channels but srgb has 0."),
        "{msg}"
    );
    let msg = eval_err(".a { p: color(srgb 0.1 0.2); }");
    assert!(
        msg.contains("$description: The srgb color space has 3 channels but (srgb 0.1 0.2) has 2."),
        "{msg}"
    );
    // Channel and alpha messages carry the `$description:` prefix here…
    let msg = eval_err(".a { p: color(srgb c 0.2 0.3); }");
    assert!(
        msg.contains("$description: Expected red channel to be a number, was c."),
        "{msg}"
    );
    let msg = eval_err(".a { p: color(list.slash(srgb 0.1 0.2 0.3, c)); }");
    assert!(msg.contains("$description: c is not a number."), "{msg}");
}

#[test]
fn color_wrapper_multi_argument_var_still_passes_through() {
    // var() substitutes before parsing and may occupy several channel slots —
    // a wrong-looking count with a symbolic component is CSS, not a dart error
    // (regression pin for the special_functions #8/#9 first-cut regressions).
    let out = css(".a { p: color(srgb var(--foo) 0.3); }");
    assert!(out.contains("color(srgb var(--foo) 0.3)"), "{out}");
    let out = css(".a { p: color(srgb 0.1 var(--foo)); }");
    assert!(out.contains("color(srgb 0.1 var(--foo))"), "{out}");
}

// ─── modern single-argument hsl/hwb (round 64, hsl+hwb/one_arg goldens) ───────

#[test]
fn hsl_family_single_argument_routes_to_legacy_forms() {
    // The modern space-separated single argument produces dart's legacy `hsl()`
    // serialization via the shared space-color rebuild.
    let out = css("@use \"sass:color\"; a { p: color.hwb(0 30% 40%); }");
    assert!(out.contains("hsl(0, 33.3333333333%, 45%)"), "{out}");
    let out = css("@use \"sass:color\"; a { p: color.hwb(180 30% 40% / 0.4); }");
    assert!(out.contains("hsla(180, 33.3333333333%, 45%, 0.4)"), "{out}");
}

#[test]
fn hwb_strict_percent_and_type_errors() {
    let msg = eval_err("@use \"sass:color\"; a { p: color.hwb(0 30 40%); }");
    assert!(
        msg.contains("$whiteness: Expected 30 to have unit \"%\"."),
        "{msg}"
    );
    let msg = eval_err("@use \"sass:color\"; a { p: color.hwb(0 30px 40%); }");
    assert!(
        msg.contains("$whiteness: Expected 30px to have unit \"%\"."),
        "{msg}"
    );
    // A quoted string in a channel is a concrete type error, not a passthrough…
    let msg = eval_err(r#"a { p: hsl("foo" 100% 50%); }"#);
    assert!(
        msg.contains("$channels: Expected hue channel to be a number, was \"foo\"."),
        "{msg}"
    );
    // …and so is a bare word, even with extra slots (relative anchor checked first).
    let msg = eval_err("a { p: hwb(c #aaa h w b / 25%); }");
    assert!(
        msg.contains("$channels: Expected hue channel to be a number, was c."),
        "{msg}"
    );
}

#[test]
fn hsl_family_symbolic_and_relative_stay_verbatim_css() {
    // Symbolic channels and the relative `from` anchor keep the call verbatim
    // with the alpha slash rendered literal (no surrounding spaces).
    let out = css("@use \"sass:color\"; a { p: color.hwb(var(--c) 30% 40%); }");
    assert!(out.contains("hwb(var(--c) 30% 40%)"), "{out}");
    let out = css("@use \"sass:color\"; a { p: color.hwb(0 30% var(--c) / 0.5); }");
    assert!(out.contains("hwb(0 30% var(--c)/0.5)"), "{out}");
    let out = css("a { p: hwb(from #aaa h w b); }");
    assert!(out.contains("hwb(from"), "{out}");
}

// ─── rgb/rgba one_arg shape errors (round 65) ────────────────────────────────

#[test]
fn rgb_one_arg_shape_errors() {
    let msg = eval_err("a { p: rgb(()); }");
    assert!(
        msg.contains("$channels: Color component list may not be empty."),
        "{msg}"
    );
    let msg = eval_err("a { p: rgb(1); }");
    assert!(
        msg.contains("$channels: The rgb color space has 3 channels but 1 has 1."),
        "{msg}"
    );
    let msg = eval_err("a { p: rgb(1 2); }");
    assert!(
        msg.contains("$channels: The rgb color space has 3 channels but (1 2) has 2."),
        "{msg}"
    );
    let msg = eval_err("a { p: rgb((1, 2, 3)); }");
    assert!(
        msg.contains("$channels: Expected a space- or slash-separated list, was (1, 2, 3)"),
        "{msg}"
    );
    let msg = eval_err("a { p: rgb([1 2 3]); }");
    assert!(
        msg.contains("$channels: Expected an unbracketed list, was [1 2 3]"),
        "{msg}"
    );
    let msg = eval_err("a { p: rgb(\"foo\" 1 2); }");
    assert!(
        msg.contains("$channels: Expected red channel to be a number, was \"foo\"."),
        "{msg}"
    );
    let msg = eval_err("a { p: rgb(list.slash(1 2 3, 0.4, 1)); }");
    assert!(
        msg.contains("$channels: Only 2 slash-separated elements allowed, but 3 were passed."),
        "{msg}"
    );
}

#[test]
fn rgb_symbolic_still_dispatches_historically() {
    // The round-28 rgb special-function rules must survive the new shape pass.
    let out = css("a { p: rgb(var(--foo) 2 3); }");
    assert!(out.contains("rgb(var(--foo), 2, 3)"), "{out}");
    let out = css("a { p: rgb(1, 2, var(--foo)); }");
    assert!(out.contains("rgb(1, 2, var(--foo))"), "{out}");
    let out = css("@use \"sass:list\"; a { p: hsl(list.slash(var(--foo), 0.4)); }");
    // A slash *pair* renders verbatim with spaced `/` (unlike a division slot).
    assert!(out.contains("hsl(var(--foo) / 0.4)"), "{out}");
}
