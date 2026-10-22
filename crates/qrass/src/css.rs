//! Expanded CSS serialization (task group 9): the output-node layer that turns
//! evaluated declarations, comments, and nested at-rule blocks into indented text.
//!
//! The eval fold streams owned chunk `String`s (the reactive "event → String"
//! protocol the render stage reduces); this module owns *how a block looks* —
//! two-space indent, block order = source order, `!important` already baked into
//! the value string by `SassValue::to_css`, and loud `/* … */` comments preserved
//! while silent `//` comments are dropped upstream by the lexer. Statement code
//! therefore stays free of formatting concerns.
//!
//! Value-level numeric/color formatting lives in [`crate::value`] (`to_css`,
//! `format_number`); the token-passthrough helper [`number_to_string`] (raw
//! literals kept inside selectors and at-rule heads) is consolidated here.

/// One entry in a rule body, kept in source order so comments interleave with
/// declarations exactly as they appeared in the stylesheet.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    /// A `prop: value;` declaration (`value` already carries any `!important`).
    Decl { prop: String, value: String },
    /// A loud `/* … */` comment. Silent `//` comments never reach this layer.
    Comment(String),
    /// A verbatim line (e.g. a nested unknown at-rule like `@b c;`) kept as-is.
    Raw(String),
}

/// Format one qualified rule (`selectors { items }`). Callers are responsible for
/// dropping empty rules; a rule whose only content is comments still renders.
pub(crate) fn render_rule(selectors: &[String], items: &[Item]) -> String {
    // dart's bogus-combinators tolerance (`css/selector/combinator/*` ~27
    // sections): a selector leading with a combinator (`+ a`, `:is(> a)`) or
    // carrying adjacent combinators (`a > + b`) is INVALID — dart drops the
    // selector (empty output when none survive) and warns. The shared
    // lightningcss canonicalization lets an empty rule fall through harmlessly.
    let selectors: Vec<String> = selectors
        .iter()
        .filter(|s| {
            let bogus = split_top_commas(s).iter().all(|seg| is_bogus_combinator(seg));
            if bogus {
                tracing::warn!("DEPRECATION WARNING [bogus-combinators]: The selector \"{s}\" is invalid.");
            }
            !bogus
        })
        .cloned()
        .collect();
    if selectors.is_empty() {
        return String::new();
    }
    // the goldens' expanded output puts each comma-separated selector on its own line, the
    // `{` trailing the last one; the shared body indent keeps continuation lines
    // aligned, and a wrapping `@media`/`@supports` shifts them uniformly.
    let sel = selectors
        .iter()
        .map(|s| normalize_selector(s))
        .collect::<Vec<_>>()
        .join(",\n");
    let body = items
        .iter()
        .map(|it| match it {
            Item::Decl { prop, value } => format!("  {prop}: {value};"),
            Item::Comment(text) => format!("  {}", text.trim()),
            Item::Raw(line) => format!("  {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{sel} {{\n{body}\n}}")
}

// ─── Pure-transformation streams (RFP contract R1/R2: every sequence flows
// through an rxrust fold; `reduce_initial` moves the owned accumulator by
// value and emits it exactly once — the EvalFold/lexer paradigm, cf. eval.rs) ───

use std::iter;

use rxrust::prelude::*;

use crate::error::SassError;
use std::convert::Infallible;

/// Drive a single-emission fold stream to its one value through the shared
/// `drain_one` terminal (errors are unobservable here: pure transforms never
/// fail, so the `Infallible` channel is never used).
fn fold_once<T: 'static>(
    stream: LocalBoxedObservable<'static, Result<T, SassError>, Infallible>,
) -> T {
    crate::drain::drain_one(stream)
}

/// Normalize one selector to the goldens output form. Attribute-selector values
/// that are valid CSS identifiers are emitted unquoted (`[a^='top']` → `[a^=top]`);
/// quoted values that need quoting and bare values pass through unchanged. Only
/// the contents of `[…]` blocks are rewritten so pseudo-class parentheses etc.
/// are untouched.
fn normalize_selector(sel: &str) -> String {
    /// Char-fed transducer: `[…]` bodies buffer into `buf`, everything else
    /// streams straight to `out`. A `'\u{0}'` sentinel flushes an open bracket.
    #[derive(Default)]
    struct Norm {
        out: String,
        buf: String,
        in_attr: bool,
    }
    impl Norm {
        fn step(self, ch: char) -> Self {
            if ch == '\u{0}' {
                return Self {
                    out: self.out
                        + &self.in_attr.then_some(format!("[{}", normalize_attr(&self.buf))).unwrap_or_default(),
                    buf: String::new(),
                    in_attr: false,
                };
            }
            match (self.in_attr, ch) {
                (false, '[') => Self { in_attr: true, ..self },
                (true, ']') => Self {
                    out: self.out + "[" + &normalize_attr(&self.buf) + "]",
                    buf: String::new(),
                    in_attr: false,
                },
                (true, _) => Self { buf: self.buf + &ch.to_string(), ..self },
                (false, _) => Self { out: self.out + &ch.to_string(), ..self },
            }
        }
    }
    fold_once(
        Local::from_iter(
            sel.chars().chain(iter::once('\u{0}')).collect::<Vec<char>>(),
        )
        .reduce_initial(Norm::default(), |st, ch| st.step(ch))
        .map(|st| Ok(st.out))
        .box_it(),
    )
}

/// Within one `[ … ]` body, drop the quotes around an `=` value when that value
/// is a valid CSS identifier (matching the goldens selector serialization).
fn normalize_attr(body: &str) -> String {
    let Some(eq) = body.find('=') else {
        return body.to_string();
    };
    let (lhs, rhs) = body.split_at(eq); // `rhs` starts with the `=`
    let value = rhs[1..].trim();
    let quoted = value
        .chars()
        .rev()
        .zip(value.chars())
        .next()
        .is_some_and(|(last, first)| {
            matches!(first, '\'' | '"') && value.chars().count() >= 2 && last == first
        });
    quoted
        .then(|| value[1..value.len() - 1].to_string())
        .filter(|content| is_css_ident(content))
        .map_or(body.to_string(), |content| format!("{lhs}={content}"))
}

/// Whether `s` is a bare CSS identifier safe to emit unquoted in an attribute
/// selector: non-empty, not starting with a digit, and composed only of
/// letters/digits/`-`/`_`/non-ASCII (a leading `-` must be followed by an
/// identifier char or another `-`).
fn is_css_ident(s: &str) -> bool {
    let c: Vec<char> = s.chars().collect();
    let cont = |ch: char| ch.is_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii();
    let ident_start = |f: char| f.is_alphabetic() || f == '_' || !f.is_ascii();
    c.first()
        .is_some_and(|&f| match f {
            '-' => c.get(1).is_some_and(|&n| ident_start(n) || n == '-'),
            d if d.is_ascii_digit() => false,
            other => ident_start(other),
        })
        && c.iter().skip(1).all(|&ch| cont(ch))
}

/// Indent every line of each child block two spaces and wrap the group in a
/// single `header { … }` at-rule, so sibling rules under one `@media`/`@supports`
/// /`@keyframes` share a merged block.
pub(crate) fn render_at_block(header: &str, children: &[String]) -> String {
    let inner = children
        .iter()
        .flat_map(|b| b.lines())
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{header} {{\n{inner}\n}}")
}

/// A top-level loud comment, emitted verbatim (no indentation).
pub(crate) fn render_comment(text: &str) -> String {
    text.trim().to_string()
}

/// Format a number for token passthrough (raw literals preserved inside selectors,
/// at-rule heads, and interpolated text): no exponent, no trailing zeros.
pub(crate) fn number_to_string(v: f64) -> String {
    (v == 0.0)
        .then(|| "0".to_string())
        .or_else(|| {
            let s = if format!("{v}").contains(['e', 'E']) {
                format!("{v:.10}")
            } else {
                format!("{v}")
            };
            s.contains('.')
                .then(|| s.trim_end_matches('0').trim_end_matches('.').to_string())
                .or(Some(s))
        })
        .unwrap_or_default()
}

/// Split a selector list on top-level commas (paren/bracket depth aware). The
/// depth walk is an owned `reduce_initial` fold over the char stream; the final
/// slicing stays local (`&str` borrows are not `'static`, so the pieces
/// themselves never enter the stream).
pub(crate) fn split_top_commas(sel: &str) -> Vec<&str> {
    let cuts: Vec<usize> = fold_once(
        Local::from_iter(sel.char_indices().collect::<Vec<(usize, char)>>())
            .reduce_initial((0usize, Vec::new()), |acc, (i, c)| {
                let (mut depth, mut cuts) = acc;
                depth = match c {
                    '(' | '[' => depth + 1,
                    ')' | ']' => depth.saturating_sub(1),
                    _ => depth,
                };
                if c == ',' && depth == 0 {
                    cuts.push(i);
                }
                (depth, cuts)
            })
            .map(|(_, cuts)| Ok(cuts))
            .box_it(),
    );
    let bounds: Vec<usize> = iter::once(0).chain(cuts).chain(iter::once(sel.len())).collect();
    bounds.windows(2).map(|w| &sel[w[0]..w[1]]).collect()
}

/// Golden matrix (`css/selector/combinator/*`):
/// * top-level leading (`+ a`, `> a` …) — KEPT with a warning;
/// * adjacent combinators at any depth (`a > + b`) — dropped;
/// * trailing combinator (`a +`) — dropped;
/// * combinator straight after a parenthesis (`:is(+ a)`, `:has( > a)` …) —
///   dropped everywhere EXCEPT inside `:has(…)` (relative selectors are the
///   spec-sanctioned exception, `has.hrx` keeps `:has(+ a)` verbatim).
///
/// Owned fold over the trimmed segment: the scan short-circuits by freezing
/// once `bogus` is set, exactly where the imperative version returned early.
pub(crate) fn is_bogus_combinator(seg: &str) -> bool {
    #[derive(Default)]
    struct Scan {
        depth: usize,
        prev_comb: bool,
        heads: Vec<bool>, // per open paren: still at the head position
        hass: Vec<bool>,  // per open paren: opened by `:has`
        seen: String,
        bogus: bool,
        last: Option<char>,
    }
    let is_comb = |c: char| matches!(c, '+' | '~' | '>');
    impl Scan {
        fn step(mut self, c: char) -> Self {
            if self.bogus {
                return self;
            }
            match c {
                '(' | '[' => {
                    self.depth += 1;
                    self.heads.push(true);
                    self.hass.push(self.seen.ends_with(":has"));
                    self.prev_comb = false;
                }
                ')' | ']' => {
                    self.depth = self.depth.saturating_sub(1);
                    self.heads.pop();
                    self.hass.pop();
                    self.prev_comb = false;
                }
                // whitespace keeps both the adjacency and the paren-head state
                ' ' | '\t' => {}
                '+' | '~' | '>' => {
                    let at_paren_head = self.depth > 0
                        && self.heads.last().copied().unwrap_or(false)
                        && !self.hass.last().copied().unwrap_or(false);
                    self.bogus = at_paren_head || self.prev_comb;
                    self.prev_comb = true;
                    self.heads.iter_mut().rev().take(1).for_each(|h| *h = false);
                }
                _ => {
                    self.prev_comb = false;
                    self.heads.iter_mut().rev().take(1).for_each(|h| *h = false);
                }
            }
            self.last = (!c.is_whitespace()).then_some(c).or(self.last);
            self.seen.push(c);
            self
        }
    }
    let scan = fold_once(
        Local::from_iter(seg.trim().chars().collect::<Vec<char>>())
            .reduce_initial(Scan::default(), |st, c| st.step(c))
            .map(Ok)
            .box_it(),
    );
    scan.bogus
        || (scan.prev_comb && scan.last.is_some_and(is_comb))
}

/// Re-serialize an `@supports`/`@media` query head like dart does: a
/// *declaration-shaped* group (top-level `:` before any paren, no nesting)
/// drops loud comments, trims its edges and never leaves a space before the
/// `:`; a function call (`a(/**/ b)`, preceded by an identifier or `)`/`]`)
/// is kept VERBATIM; a bare condition group drops only its LEADING loud
/// comments (`(/**/ a b)` → `(a b)`, `(a /**/ b)` stays).
/// Recursive descent over balanced groups, no index cursors.
pub(crate) fn tidy_condition(q: &str) -> String {
    tidy_slice(&q.chars().collect::<Vec<char>>())
}

fn tidy_slice(chars: &[char]) -> String {
    let (head, rest) = chars.split_at(chars.iter().position(|&c| c == '(').unwrap_or(chars.len()));
    let head_s: String = head.iter().collect();
    let group_open = rest.first().is_some_and(|&c| c == '(');
    // (guard clause: no rest-of-stream to fold without an opening paren)
    if !group_open {
        return head_s;
    }
    let (inner, after) = split_balanced(rest);
    let inner_s: String = inner.iter().collect();
    let group = match (is_declaration_group(&inner_s), function_form(&head_s)) {
        (true, _) => tidy_decl(&inner_s),
        (false, true) => inner_s,
        (false, false) => strip_leading_loud(&inner_s),
    };
    format!("{head_s}({group}){}", tidy_slice(after))
}

/// `rest[0] == '('`: split into balanced inner chars and the tail after `)`.
/// The closing scan is an owned fold that freezes the first zero-depth index.
fn split_balanced(rest: &[char]) -> (&[char], &[char]) {
    let cut: Option<usize> = fold_once(
        Local::from_iter(rest[1..].iter().copied().enumerate().collect::<Vec<(usize, char)>>())
            .reduce_initial((1i32, None::<usize>), |acc, (i, c)| {
                let (mut depth, found) = acc;
                depth += i32::from(c == '(') - i32::from(c == ')');
                (depth, found.or((depth == 0).then_some(i + 1)))
            })
            .map(|(_, found)| Ok(found))
            .box_it(),
    );
    cut.map_or((&rest[1..], &[] as &[char]), |end| {
        (&rest[1..end], &rest[(end + 1).min(rest.len())..])
    })
}

/// A group is a declaration when its first significant paren-free char is `:`.
fn is_declaration_group(inner: &str) -> bool {
    inner.chars().find(|c| matches!(c, '(' | ':')).is_some_and(|c| c == ':')
}

/// The chars before a `(` decide group vs function form: an identifier (not
/// the `not`/`and`/`or` keywords) or a closing `)`/`]` makes it a call. The
/// backward word scan is an owned fold.
fn function_form(head: &str) -> bool {
    #[derive(Default)]
    struct Tail {
        word: String,
        last: Option<char>,
        stopped: bool,
    }
    impl Tail {
        fn step(mut self, c: char) -> Self {
            if self.stopped {
                return self;
            }
            match c {
                c if c.is_whitespace() => self.stopped = self.last.is_some(),
                c if c.is_alphanumeric() || matches!(c, '-' | '_') => {
                    self.word.insert(0, c);
                    self.last = Some(c);
                }
                c => {
                    self.last = Some(c);
                    self.stopped = true;
                }
            }
            self
        }
    }
    let tail = fold_once(
        Local::from_iter(head.chars().rev().collect::<Vec<char>>())
            .reduce_initial(Tail::default(), |st, c| st.step(c))
            .map(Ok)
            .box_it(),
    );
    tail.last
        .is_some_and(|c| {
            (c.is_alphanumeric() || matches!(c, '-' | '_'))
                && !matches!(tail.word.as_str(), "not" | "and" | "or")
                || matches!(c, ')' | ']')
        })
}

fn tidy_decl(inner: &str) -> String {
    let bare = strip_loud_comments(inner);
    let trimmed = bare.trim_matches(|c| c == ' ' || c == '\t');
    trimmed
        .split_once(':')
        .map_or(trimmed.to_string(), |(k, v)| format!("{}:{}", k.trim_end(), v))
}

fn strip_loud_comments(s: &str) -> String {
    let parts: Vec<String> = s.split("/*").skip(1).map(str::to_string).collect();
    let head = s.split("/*").next().unwrap_or("").to_string();
    fold_once(
        Local::from_iter(parts)
            .reduce_initial(String::new(), |mut acc, part| {
                acc.push_str(part.split_once("*/").map_or("", |(_, rest)| rest));
                acc
            })
            .map(move |tail| Ok(format!("{head}{tail}")))
            .box_it(),
    )
}

/// Recursive leading-comment drop for condition groups: `/**/` at the head
/// (with the whitespace after it) goes; mid/trailing comments stay verbatim.
fn strip_leading_loud(inner: &str) -> String {
    let trimmed = inner.trim_start_matches([' ', '\t']);
    match trimmed.strip_prefix("/*") {
        // a leading `/*` with no closer swallows the remainder, as the fold did
        Some(rest) => strip_leading_loud(rest.split_once("*/").map_or("", |(_, tail)| tail)),
        None => trimmed.to_string(),
    }
}
