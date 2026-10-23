//! Text-level predicates for statement traversal — a sibling of [`crate::eval::eval_stmt`]
//! split for the 500-line budget (task 5b.2): the `@import` CSS-vs-module test and
//! the selector-combination helpers that turn a resolved parent list plus a child
//! selector text into the concrete selector list a block is expanded with.

/// A CSS `@import` target (passed through) is a `url(…)`, a quoted string whose
/// URL ends in `.css`, or a quoted URL carrying a support condition (a media
/// query, `layer`, `supports(…)`, or a bare ident/function after the URL — the
/// condition makes it unambiguously CSS rather than an SCSS module reference).
/// A bare `@import "a"` (nothing after the quote) stays an SCSS import (D3).
pub(super) fn looks_like_css_import(value: &str) -> bool {
    value.contains("url(")
        || crate::builtins::selector::split_commas(value)
            .into_iter()
            .any(|spec| {
                let s = spec.trim();
                // a `.css` URL, or a quoted URL followed by its support condition
                s.trim_matches(|c| c == '"' || c == '\'').ends_with(".css")
                    || after_leading_quoted(s).is_some_and(|rest| !rest.trim().is_empty())
            })
}

/// If `s` begins with a single- or double-quoted string, return the text after
/// its closing quote (honoring backslash escapes); otherwise `None`.
fn after_leading_quoted(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let open = *bytes.first()?;
    match open == b'"' || open == b'\'' {
        false => None,
        true => {
            // fold the byte run with a `found` latch: `\` swallows the next byte,
            // the matching quote freezes the cursor just past it (the old early
            // `return`)
            let (tail, found) = (1..bytes.len()).fold((1usize, false), |(i, found), k| {
                match (found, k == i) {
                    (true, _) | (false, false) => (i, found),
                    (false, true) => match bytes[k] {
                        b'\\' => (k + 2, false),
                        c if c == open => (k + 1, true),
                        _ => (k + 1, false),
                    },
                }
            });
            found.then(|| &s[tail..])
        }
    }
}

/// Combine a selector list for a `@at-root`-lifted ruleset. Like
/// [`combine_selectors`] a `&` is replaced by the outer parent, but a child
/// *without* `&` stands alone at the root instead of being descendant-prefixed.
pub(super) fn combine_at_root(parents: &[String], child_text: &str) -> Vec<String> {
    let children: Vec<String> = crate::builtins::selector::split_commas(child_text)
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    match parents.is_empty() {
        true => children,
        false => parents
            .iter()
            .flat_map(|p| {
                children.iter().map(move |c| {
                    if c.contains('&') {
                        c.replace('&', p)
                    } else {
                        c.clone()
                    }
                })
            })
            .collect(),
    }
}

/// Combine a selector list with its resolved parent list. A `&` in a child is
/// replaced by the parent; otherwise the child is a descendant (space). At the
/// root (no parent) the child selectors stand alone.
pub(super) fn combine_selectors(parents: &[String], child_text: &str) -> Vec<String> {
    let children: Vec<String> = crate::builtins::selector::split_commas(child_text)
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    match parents.is_empty() {
        true => children,
        false => parents
            .iter()
            .flat_map(|p| {
                children.iter().map(move |c| {
                    if c.contains('&') {
                        c.replace('&', p)
                    } else {
                        format!("{p} {c}")
                    }
                })
            })
            .collect(),
    }
}

/// Whether a resolved selector references a `%placeholder` (so it must be held
/// back for the `@extend` pass). A placeholder is a `%` immediately followed by
/// an identifier-start char (letter / `_` / non-ASCII); a trailing `%` (a
/// keyframe percentage like `50%` or `100%`) is *not* a placeholder.
pub(super) fn contains_placeholder(sel: &str) -> bool {
    let c: Vec<char> = sel.chars().collect();
    c.windows(2).any(|w| {
        w[0] == '%' && (w[1].is_alphabetic() || w[1] == '_' || !w[1].is_ascii())
    })
}
