//! Path/text helpers for module loading, split out of [`super`] (`eval_module`)
//! for the 500-line budget (OpenSpec `refactor-rfp-strict` task 5b.2). Pure
//! functions — no `&mut self` state — so the parent reaches them by `use`;
//! `parent_dir` is re-exported as `pub(crate)` for the module-graph callers.

use std::path::{Path, PathBuf};

use crate::span::{BytePos, Span};

/// The underlineable text of a `@use`/`@forward` statement: from its keyword to
/// just before the terminating `;`, which the goldens never mark
/// (`multi_configuration/one_file` underlines 31 of the 32 characters of
/// `@use "other" as o2 with ($a: b);`).
pub(super) fn underline(content: &str, span: Span) -> (BytePos, BytePos) {
    let start = span.start.0.min(content.len());
    let end = span.end.0.clamp(start, content.len());
    let text = &content[start..end];
    (
        span.start,
        BytePos(start + text.trim_end_matches([';', ' ', '\t', '\n', '\r']).len()),
    )
}

/// Directory containing `path` (empty for a bare relative name) — the base a
/// relative `@use`/`@forward` URL resolves against.
pub(crate) fn parent_dir(path: &str) -> PathBuf {
    Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default()
}

/// Merge a module's own member names with its `@forward`ed (exported, defining)
/// pairs, first-wins on the exported name.
pub(super) fn dedup_members(
    own: Vec<(String, String)>,
    forwarded: Vec<(String, String)>,
) -> Vec<(String, String)> {
    // first-wins against the *accumulating* list, so a duplicate inside
    // `forwarded` is dropped as well
    forwarded.into_iter().fold(own, |mut acc, pair| {
        let seen = acc.iter().any(|(n, _)| n == &pair.0);
        acc.extend((!seen).then_some(pair));
        acc
    })
}
