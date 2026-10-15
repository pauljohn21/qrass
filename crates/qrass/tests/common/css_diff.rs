//! Golden CSS comparison helpers (task 10.4). The element-plus output and the
//! dart-sass baseline are both re-serialized through the same deterministic CSS
//! serializer ([`lightningcss`]) before a line-by-line whitespace-normalized
//! comparison. Canonicalizing both sides on the identical printer removes
//! purely-cosmetic differences — most notably dart-sass expanded's irregular
//! selector line-breaking (some selector lists joined, others split) — while
//! still catching any real divergence in rule order, selectors, declarations or
//! values. [`first_diff`] reports the first divergent normalized line with
//! surrounding context; [`diff_report`] summarizes every divergence.

use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use tracing::warn;

/// Re-serialize a CSS document through lightningcss so formatting is a pure
/// function of content, not of the producing engine. Falls back to the original
/// text if parsing fails OR the parser panics, so a genuinely malformed or
/// edge-case document still surfaces via the line diff (and via the structural
/// `css_validate` gate). lightningcss can `panic!` (e.g. an out-of-range hue in
/// `cssparser-color`) rather than return `Err`, so the whole pass is guarded
/// with `catch_unwind` to keep the harness robust against dependency panics.
pub fn canonicalize(css: &str) -> String {
    let printed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        StyleSheet::parse(css, ParserOptions::default())
            .ok()
            .and_then(|sheet| sheet.to_css(PrinterOptions::default()).ok())
            .map(|out| out.code)
    }));
    match printed {
        Ok(Some(code)) => code,
        Ok(None) => {
            warn!("lightningcss failed to parse CSS, comparing raw text");
            css.to_string()
        }
        Err(_) => {
            warn!("lightningcss panicked parsing CSS, comparing raw text");
            css.to_string()
        }
    }
}

/// Normalize a whole CSS document for comparison: drop blank lines, trim each
/// remaining line and collapse internal whitespace runs to a single space.
pub fn normalize(css: &str) -> Vec<String> {
    css.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

/// A single line-level divergence between our output and the golden baseline.
#[derive(Debug, Clone)]
pub struct Diff {
    /// Index into the normalized line vectors (0-based).
    pub index: usize,
    /// The golden (expected) normalized line, if present at this index.
    pub golden: Option<String>,
    /// Our (actual) normalized line, if present at this index.
    pub ours: Option<String>,
}

impl Diff {
    /// Render a human-readable one-liner describing the divergence.
    pub fn describe(&self) -> String {
        match (&self.golden, &self.ours) {
            (Some(g), Some(o)) => format!("line {}: mismatch\n  golden: {g}\n  ours:   {o}", self.index + 1),
            (Some(g), None) => format!("line {}: missing (ours ended early)\n  golden: {g}", self.index + 1),
            (None, Some(o)) => format!("line {}: extra (not in golden)\n  ours:   {o}", self.index + 1),
            (None, None) => format!("line {}: empty", self.index + 1),
        }
    }
}

/// Return up to `context + 1` lines ending at index `i` (inclusive) from `doc`,
/// clamped to the document bounds so a shorter side never panics.
fn context_window(doc: &[String], i: usize, context: usize) -> Vec<String> {
    if doc.is_empty() {
        return Vec::new();
    }
    let upper = i.min(doc.len() - 1);
    let lo = upper.saturating_sub(context);
    doc[lo..=upper].to_vec()
}

/// Compare normalized `ours` vs `golden`, returning the first divergence (if
/// any) along with up to `context` trailing lines of both sides for inspection.
pub fn first_diff(ours: &str, golden: &str, context: usize) -> Option<(Diff, Vec<String>, Vec<String>)> {
    let a = normalize(&canonicalize(ours));
    let b = normalize(&canonicalize(golden));
    let n = a.len().max(b.len());
    for i in 0..n {
        let ai = a.get(i);
        let bi = b.get(i);
        if ai != bi {
            let diff = Diff {
                index: i,
                golden: bi.map(|s| s.to_string()),
                ours: ai.map(|s| s.to_string()),
            };
            // Report up to `context` matched lines before the divergence, plus the
            // divergent line itself, clamped so a short side never underflows.
            let ctx_golden = context_window(&b, i, context);
            let ctx_ours = context_window(&a, i, context);
            return Some((diff, ctx_golden, ctx_ours));
        }
    }
    None
}

/// Collect every divergence between normalized `ours` and `golden`. A `Vec`
/// empty means the two documents are line-for-line identical after
/// normalization.
pub fn diff_report(ours: &str, golden: &str) -> Vec<Diff> {
    let a = normalize(&canonicalize(ours));
    let b = normalize(&canonicalize(golden));
    let n = a.len().max(b.len());
    (0..n)
        .filter_map(|i| {
            let ai = a.get(i);
            let bi = b.get(i);
            if ai != bi {
                Some(Diff {
                    index: i,
                    golden: bi.map(|s| s.to_string()),
                    ours: ai.map(|s| s.to_string()),
                })
            } else {
                None
            }
        })
        .collect()
}
