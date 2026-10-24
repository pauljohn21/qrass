//! Value model tests: CSS serialization, numeric parse, truthiness.

use sasspile::value::{Sep, Value};

#[test]
fn to_css_forms() {
    assert_eq!(Value::Number(10.0, "px".into()).to_css(), "10px");
    assert_eq!(Value::Number(1.5, String::new()).to_css(), "1.5");
    assert_eq!(
        Value::Str { text: "hi".into(), quoted: true }.to_css(),
        "\"hi\""
    );
    assert_eq!(
        Value::List(vec![Value::Raw("a".into()), Value::Raw("b".into())], Sep::Comma).to_css(),
        "a, b"
    );
    assert_eq!(Value::Null.to_css(), "");
}

#[test]
fn parse_number_variants() {
    assert_eq!(Value::parse_number("10px"), Some(Value::Number(10.0, "px".into())));
    assert_eq!(Value::parse_number("50%"), Some(Value::Number(50.0, "%".into())));
    assert_eq!(Value::parse_number("-3"), Some(Value::Number(-3.0, String::new())));
    assert_eq!(Value::parse_number("abc"), None);
    assert_eq!(Value::parse_number("#fff"), None);
}

#[test]
fn truthiness() {
    assert!(Value::Bool(false).is_falsey());
    assert!(Value::Null.is_falsey());
    assert!(Value::Raw("false".into()).is_falsey());
    assert!(!Value::Raw("0".into()).is_falsey());
    assert!(!Value::Number(0.0, String::new()).is_falsey());
    assert!(!Value::Bool(true).is_falsey());
}
