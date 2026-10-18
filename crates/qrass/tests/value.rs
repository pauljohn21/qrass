//! Value-model tests (task 5.1): truthiness table, color↔hsl round-trips, unit
//! conversion, and number/color serialization.

use qrass::value::{Color, SassValue, convert_number, format_number};

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// ─── Truthiness ──────────────────────────────────────────────────────────────

#[test]
fn only_null_and_false_are_falsy() {
    assert!(!SassValue::Null.is_truthy());
    assert!(!SassValue::Bool(false).is_truthy());
    assert!(SassValue::Bool(true).is_truthy());
}

#[test]
fn zero_and_empty_are_truthy() {
    // sass: 0, "" and () are all truthy — only null/false are falsy.
    assert!(
        SassValue::Number {
            value: 0.0,
            unit: None
        }
        .is_truthy()
    );
    assert!(
        SassValue::Str {
            text: String::new(),
            quoted: true
        }
        .is_truthy()
    );
    assert!(
        SassValue::List {
            items: vec![],
            sep: qrass::ast_expr::ListSep::Comma,
            bracketed: false,
            keywords: None,
        }
        .is_truthy()
    );
}

#[test]
fn type_names() {
    assert_eq!(SassValue::Null.type_name(), "null");
    assert_eq!(SassValue::Bool(true).type_name(), "bool");
    assert_eq!(
        SassValue::Number {
            value: 1.0,
            unit: Some("px".into())
        }
        .type_name(),
        "number"
    );
    assert_eq!(
        SassValue::Str {
            text: "x".into(),
            quoted: false
        }
        .type_name(),
        "string"
    );
    assert_eq!(SassValue::Color(Color::rgb(0, 0, 0)).type_name(), "color");
}

// ─── Color ↔ HSL ─────────────────────────────────────────────────────────────

#[test]
fn primary_colors_hsl() {
    let red = Color::rgb(255, 0, 0).to_hsl();
    assert!(
        approx(red.h, 0.0) && approx(red.s, 100.0) && approx(red.l, 50.0),
        "red → {red:?}"
    );
    let green = Color::rgb(0, 255, 0).to_hsl();
    assert!(
        approx(green.h, 120.0) && approx(green.s, 100.0),
        "green → {green:?}"
    );
    let blue = Color::rgb(0, 0, 255).to_hsl();
    assert!(approx(blue.h, 240.0), "blue → {blue:?}");
}

#[test]
fn gray_has_zero_saturation() {
    let g = Color::rgb(128, 128, 128).to_hsl();
    assert!(approx(g.s, 0.0), "gray s should be 0, got {}", g.s);
    assert!(
        approx(g.l, 128.0 / 255.0 * 100.0),
        "gray l = 128/255·100, got {}",
        g.l
    );
}

#[test]
fn hsl_to_rgb_round_trip() {
    // A saturated color must survive rgb → hsl → rgb.
    let original = Color::rgb(13, 110, 253); // #0d6efd
    let hsl = original.to_hsl();
    let back = Color::from_hsl(hsl.h, hsl.s, hsl.l);
    assert!(
        approx(back.r, original.r) && approx(back.g, original.g) && approx(back.b, original.b),
        "round trip failed: {original:?} → {hsl:?} → {back:?}"
    );
}

#[test]
fn black_and_white() {
    assert!(approx(Color::rgb(0, 0, 0).to_hsl().l, 0.0));
    assert!(approx(Color::rgb(255, 255, 255).to_hsl().l, 100.0));
    let w = Color::from_hsl(0.0, 0.0, 100.0);
    assert!(approx(w.r, 255.0) && approx(w.g, 255.0) && approx(w.b, 255.0));
}

// ─── Serialization ───────────────────────────────────────────────────────────

#[test]
fn number_formatting() {
    assert_eq!(format_number(10.0), "10");
    assert_eq!(format_number(1.5), "1.5");
    assert_eq!(format_number(0.25), "0.25");
    assert_eq!(format_number(-3.0), "-3");
    // trailing zeros trimmed
    assert_eq!(format_number(1.10), "1.1");
}

#[test]
fn css_serialization_forms() {
    assert_eq!(
        SassValue::Number {
            value: 10.0,
            unit: Some("px".into())
        }
        .to_css(),
        "10px"
    );
    assert_eq!(
        SassValue::Number {
            value: 50.0,
            unit: Some("%".into())
        }
        .to_css(),
        "50%"
    );
    // the goldens preserve a color's representation *origin*: a function-built
    // color serializes as `rgb(…)`, while one parsed from a `#rrggbb` literal
    // round-trips back to hex.
    assert_eq!(Color::rgb(13, 110, 253).to_css(), "rgb(13, 110, 253)");
    assert_eq!(
        Color {
            r: 13.0,
            g: 110.0,
            b: 253.0,
            a: 1.0,
            hex: true,
            c4: None,
            name: None,
            legacy: None,
            legacy_hsl: None
        }
        .to_css(),
        "#0d6efd"
    );
    assert_eq!(
        Color {
            r: 13.0,
            g: 110.0,
            b: 253.0,
            a: 0.5,
            hex: true,
            c4: None,
            name: None,
            legacy: None,
            legacy_hsl: None
        }
        .to_css(),
        "rgba(13, 110, 253, 0.5)"
    );
    assert_eq!(
        SassValue::Str {
            text: "hi".into(),
            quoted: true
        }
        .to_css(),
        "\"hi\""
    );
    assert_eq!(
        SassValue::Str {
            text: "solid".into(),
            quoted: false
        }
        .to_css(),
        "solid"
    );
}

#[test]
fn list_and_map_serialization() {
    use qrass::ast_expr::ListSep;
    let list = SassValue::List {
        items: vec![
            SassValue::Number {
                value: 1.0,
                unit: None,
            },
            SassValue::Number {
                value: 2.0,
                unit: None,
            },
        ],
        sep: ListSep::Comma,
        bracketed: false,
        keywords: None,
    };
    assert_eq!(list.to_css(), "1, 2");
    let bracketed = SassValue::List {
        items: vec![SassValue::Number {
            value: 1.0,
            unit: None,
        }],
        sep: ListSep::Space,
        bracketed: true,
        keywords: None,
    };
    assert_eq!(bracketed.to_css(), "[1]");
    let map = SassValue::Map {
        entries: vec![(
            SassValue::Str {
                text: "a".into(),
                quoted: false,
            },
            SassValue::Number {
                value: 1.0,
                unit: None,
            },
        )],
    };
    assert_eq!(map.to_css(), "(a: 1)");
}

// ─── Unit conversion ─────────────────────────────────────────────────────────

#[test]
fn absolute_length_conversion() {
    // 1in == 96px == 2.54cm == 72pt == 6pc
    assert!(approx(
        convert_number(1.0, Some("in"), Some("px")).unwrap(),
        96.0
    ));
    assert!(approx(
        convert_number(96.0, Some("px"), Some("in")).unwrap(),
        1.0
    ));
    assert!(approx(
        convert_number(1.0, Some("in"), Some("cm")).unwrap(),
        2.54
    ));
    assert!(approx(
        convert_number(1.0, Some("cm"), Some("mm")).unwrap(),
        10.0
    ));
}

#[test]
fn angle_and_time_conversion() {
    assert!(approx(
        convert_number(1.0, Some("turn"), Some("deg")).unwrap(),
        360.0
    ));
    assert!(approx(
        convert_number(180.0, Some("deg"), Some("rad")).unwrap(),
        std::f64::consts::PI
    ));
    assert!(approx(
        convert_number(1.0, Some("s"), Some("ms")).unwrap(),
        1000.0
    ));
}

#[test]
fn incompatible_and_unitless_conversion() {
    assert!(
        convert_number(1.0, Some("px"), Some("s")).is_none(),
        "cross-dimension fails"
    );
    assert!(
        convert_number(1.0, Some("px"), None).is_none(),
        "px→unitless fails"
    );
    assert!(
        approx(convert_number(5.0, None, None).unwrap(), 5.0),
        "unitless→unitless ok"
    );
    assert!(
        approx(convert_number(2.0, Some("px"), Some("px")).unwrap(), 2.0),
        "same unit identity"
    );
}
