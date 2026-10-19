//! `color.*` builtin tests (split out of `builtins.rs` to keep each file within
//! the 500-line budget). Namespaced calls dispatch directly (no `@use`).

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

/// Evaluate `expr` as the value of declaration `p` and return its serialized CSS.
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
fn color_channel_reads_rgb() {
    assert_eq!(value("color.channel(#ff8000, \"red\")"), "255");
    assert_eq!(value("color.channel(#ff8000, \"green\")"), "128");
    assert_eq!(value("color.channel(#ff8000, \"blue\")"), "0");
}

/// The deprecated single-channel accessors (`color.red` …) are shims over
/// `color.channel` reading one channel in its natural space; `whiteness`/
/// `blackness` recover exactly from the RGB extremes.
#[test]
fn color_channel_accessors() {
    assert_eq!(value("color.red(#ff8000)"), "255");
    assert_eq!(value("color.green(#ff8000)"), "128");
    assert_eq!(value("color.blue(#ff8000)"), "0");
    assert_eq!(value("color.hue(#ff0000)"), "0deg");
    assert_eq!(value("color.saturation(#ff0000)"), "100%");
    assert_eq!(value("color.lightness(#ff0000)"), "50%");
    // black = hwb(_, 0%, 100%), white = hwb(_, 0%, 0%); gray 50/50.
    assert_eq!(value("color.blackness(black)"), "100%");
    assert_eq!(value("color.blackness(white)"), "0%");
    assert_eq!(value("color.whiteness(black)"), "0%");
    assert_eq!(value("color.whiteness(#808080)"), "50.1960784314%");
    // Named-argument form folds `$color` into slot 0 like every other builtin.
    assert_eq!(value("color.red($color: #ed0000)"), "237");
}

#[test]
fn color_constructors() {
    // Explicit rgb()/hsl() constructors serialize in the functional form (the goldens
    // preserves representation origin: only a `#rrggbb` literal emits hex).
    assert_eq!(value("color.rgb(255, 0, 0)"), "rgb(255, 0, 0)");
    assert_eq!(value("color.hsl(0, 100%, 50%)"), "rgb(255, 0, 0)");
}

#[test]
fn color_adjust_scale_change() {
    assert_eq!(value("color.adjust(#000000, $red: 128)"), "#800000");
    assert_eq!(value("color.change(#000000, $red: 200)"), "#c80000");
    assert_eq!(value("color.scale(#000000, $red: 50%)"), "#800000");
}

#[test]
fn color_mix_and_meta() {
    assert_eq!(value("color.mix(#ff0000, #0000ff)"), "#800080");
    assert_eq!(value("color.alpha(#ff0000)"), "1");
    assert_eq!(value("color.ie-hex-str(#ff0000)"), "#FFFF0000");
}

/// Round-68 (modules/color/error goldens): dart's two spellings for a missing
/// `sass:color` function.
#[test]
fn color_module_undefined_spellings() {
    let err_of = |expr: &str| {
        let src = format!(".a {{ p: {expr}; }}");
        let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
        let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
        eval_all(0, "t.scss".into(), src, program)
            .err()
            .unwrap_or_else(|| panic!("{expr} must fail"))
            .to_string()
    };
    for name in [
        "adjust-hue",
        "lighten",
        "darken",
        "saturate",
        "desaturate",
        "opacify",
        "fade-in",
        "transparentize",
        "fade-out",
    ] {
        let msg = err_of(&format!("color.{name}(#abcdef, 10%)"));
        assert!(
            msg.contains(&format!("The function {name}() isn't in the sass:color module.")),
            "{name}: {msg}"
        );
    }
    for name in ["adjust-color", "scale-color", "change-color"] {
        let msg = err_of(&format!("color.{name}(#abcdef, $red: 10%)"));
        assert!(msg.contains("Undefined function."), "{name}: {msg}");
    }
    // untouched names keep the named spelling
    let msg = err_of("color.mystery(#abc)");
    assert!(msg.contains("Undefined function \"color.mystery\""), "{msg}");
}

/// Round-70 (css_overloads + error/type goldens): the ns CSS overloads exist
/// only for a lone *Number* on grayscale/invert/opacity; alpha has none, and
/// symbolic/bare-word arguments keep the Sass `$color` type errors.
#[test]
fn color_ns_css_overloads() {
    let err = |expr: &str| {
        let src = format!(".a {{ p: {expr}; }}");
        let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
        let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
        eval_all(0, "t.scss".into(), src, program)
            .err()
            .unwrap_or_else(|| panic!("{expr} must fail"))
            .to_string()
    };
    assert_eq!(value("color.grayscale(1)"), "grayscale(1)");
    assert_eq!(value("color.invert(1)"), "invert(1)");
    assert_eq!(value("color.opacity(1)"), "opacity(1)");
    assert_eq!(value("color.alpha(c=d)"), "alpha(c=d)");
    assert_eq!(value("color.alpha(c=d, e=f, g=h)"), "alpha(c=d, e=f, g=h)");
    let msg = err("color.alpha(1)");
    assert!(msg.contains("$color: 1 is not a color."), "{msg}");
    let msg = err("color.opacity(var(--c))");
    assert!(msg.contains("$color: var(--c) is not a color."), "{msg}");
    let msg = err("color.grayscale(c)");
    assert!(msg.contains("$color: c is not a color."), "{msg}");
    // the global CSS-filter spelling `alpha(c=d)` passes through verbatim
    assert_eq!(value("alpha(c=d)"), "alpha(c=d)");
}
