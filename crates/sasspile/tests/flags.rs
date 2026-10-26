//! Declaration flags on a binding (the `variables/double_flag.hrx` pair): `!default` / `!global`
//! describe the *binding*, so they are never part of the value and never reach CSS. The third test is
//! the boundary that keeps this from becoming a general "eat the `!`" rule.

use sasspile::compile;

/// `variables/double_flag.hrx::default` — a repeated flag is deprecated but still parsed, and both
/// copies disappear from the bound value.
#[tokio::test]
async fn default_flag_is_not_part_of_the_bound_value() {
    assert_eq!(
        compile("$a: b !default !default;\n\nc {d: $a}")
            .await
            .expect("double flag parses"),
        "c {\n  d: b;\n}\n"
    );
}

/// `variables/double_flag.hrx::global` — same for `!global`, whose *scoping* meaning is P3.6: with a
/// flat environment today the visible result is identical, and the fixture states exactly this text.
#[tokio::test]
async fn global_flag_is_not_part_of_the_bound_value() {
    assert_eq!(
        compile("$a: b;\nc {\n  $a: d !global !global;\n  e: $a\n}")
            .await
            .expect("double global flag parses"),
        "c {\n  e: d;\n}\n"
    );
}

/// The boundary: a flag is peeled from a **binding's** value only. A custom property is raw CSS text, so
/// `!important` and even a bare `!` run survive exactly as written — this is the
/// `css/custom_properties/exclamation.hrx` fixture, which is in the passing baseline and must stay so.
#[tokio::test]
async fn a_flag_survives_in_a_custom_property_value() {
    assert_eq!(
        compile(".x {\n  --important: value !important;\n  --multiple: !important !important;\n}")
            .await
            .expect("custom property compiles"),
        ".x {\n  --important: value !important;\n  --multiple: !important !important;\n}\n"
    );
}
