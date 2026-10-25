//! Interim error-policy contract for P0′.3a.
//!
//! P0′.3a plumbed the real `Err` channel (`ErrorKind`, `Observer::error` from the `Exec` kernel, a
//! terminal that no longer relabels failures) **without moving the pass count**: the `FATAL` set in
//! the crate root is still empty. These tests pin both halves — the passthrough the corpus already
//! requires, and the interim verbatim behaviour that P0′.3b deliberately turns into an error. When
//! 0′.3b lands, the last test below is expected to change — that is the point of writing it down.

use sasspile::compile;

/// Corpus-mandated passthrough: an IE `progid:` filter value is raw CSS, and Sass does not resolve a
/// variable inside a literal it never parses as an expression
/// (`css/functions/special/prefixed/lowercase::*/script_like`).
#[tokio::test]
async fn progid_filter_value_stays_literal() {
    let css =
        compile("a { filter: progid:DXImageTransform.Microsoft.AlphaImageLoader(src=\"$i.png\") }")
            .await
            .expect("literal filter value compiles");
    assert!(
        css.contains("progid:DXImageTransform.Microsoft.AlphaImageLoader"),
        "got {css}"
    );
    assert!(css.contains("$i"), "the literal must survive, got {css}");
}

/// Corpus-mandated passthrough: an escaped dollar in a selector is text, not a variable reference
/// (`css/selector/escaping::dollar_char`).
#[tokio::test]
async fn escaped_dollar_in_selector_is_not_a_variable() {
    let css = compile(r"a\\$b { c: d }").await.expect("escape compiles");
    assert!(css.contains(r"\$"), "escape must stay, got {css}");
}

/// Interim policy (P0′.3a): an unbound variable in an ordinary value still falls back to the source
/// text, because `FATAL` is empty until the literal-context rule of P0′.3b exists. Measured: making
/// this fatal today moved 598 → 984 pass but regressed 60 cases, so the flip is deliberately gated.
#[tokio::test]
async fn unbound_variable_stays_verbatim_until_p0_3b() {
    let css = compile("a { b: $undefined }")
        .await
        .expect("interim policy: not fatal yet");
    assert_eq!(css, "a {\n  b: $undefined;\n}\n");
}
