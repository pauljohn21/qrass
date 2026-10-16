//! Dart-style source-snippet rendering for errors, matching the annotation
//! blocks of the `expressions/if/error/*.hrx` goldens: a `,` corner, the
//! numbered source line, one marker run per annotation with an optional
//! trailing label, a `'` foot, and the final
//! `<file> <line>:<col>  root stylesheet` location line.
//!
//! Layout is derived from the goldens byte-for-byte: the marker line is
//! `"  |" + col spaces + marker×len (+ " " + label)`, so a 1-based column `C`
//! lines up with the source line's `"1 | "` gutter of width 4.

use crate::error::{Located, Marker, SassError};
use crate::parser::Parser;
use crate::span::{BytePos, excerpt_line, line_col};

impl Parser {
    /// A snippet marker spanning `start..end`; `label` captions the run the way
    /// the `expressions/if/error/raw.hrx` goldens do.
    pub(crate) fn marker(
        &self,
        marker: char,
        start: BytePos,
        end: BytePos,
        label: Option<String>,
    ) -> Marker {
        let (line, col) = self.line_col(start.0);
        Marker {
            marker,
            line,
            col,
            len: self.slice(start, end).chars().count() as u32,
            label,
        }
    }

    /// A one-column caret marker at `at`, for the single-token golden errors.
    pub(crate) fn caret(&self, at: BytePos) -> Marker {
        let (line, col) = self.line_col(at.0);
        Marker {
            marker: '^',
            line,
            col,
            len: 1,
            label: None,
        }
    }

    /// A parse error carrying a golden-style snippet block annotated by `markers`.
    pub(crate) fn err_marked(
        &self,
        msg: impl Into<String>,
        at: BytePos,
        markers: Vec<Marker>,
    ) -> SassError {
        self.err(msg, at).with_markers(markers)
    }
}

/// Eval-side marker: line, 1-based column and run length resolved straight from
/// the importing file's text, so a configuration error raised while a module
/// was instantiating still points back at the `@use … with` entry.
pub(crate) fn marker_in(source: &str, marker: char, start: BytePos, end: BytePos) -> Marker {
    let lc = line_col(source, start);
    let from = start.0.min(source.len());
    let to = end.0.clamp(from, source.len());
    Marker {
        marker,
        line: lc.line,
        col: lc.col,
        len: source[from..to].chars().count() as u32,
        label: None,
    }
}

/// Eval-side location for the golden's trailing `<file> <line>:<col>` line.
pub(crate) fn located_in(file: &str, source: &str, at: BytePos) -> Located {
    let lc = line_col(source, at);
    Located {
        file: file.to_owned(),
        line: lc.line,
        col: lc.col,
        excerpt: excerpt_line(source, lc),
    }
}

/// Render one error block. Markers are emitted in the order given (the goldens
/// put the `^` run before the `=` run regardless of their source order) and
/// only markers on the primary marker's line are drawn — every golden snippet
/// annotates a single line.
pub(crate) fn snippet(message: &str, loc: &Located, markers: &[Marker]) -> String {
    let Some(primary) = markers.first() else {
        return String::new();
    };
    let mut out = format!(
        "Error: {message}\n  ,\n{} | {}\n",
        primary.line,
        loc.excerpt.as_deref().unwrap_or("")
    );
    for m in markers {
        if m.line != primary.line {
            continue;
        }
        out.push_str("  |");
        for _ in 0..m.col {
            out.push(' ');
        }
        for _ in 0..m.len {
            out.push(m.marker);
        }
        if let Some(label) = &m.label {
            out.push(' ');
            out.push_str(label);
        }
        out.push('\n');
    }
    out.push_str("  '\n");
    out.push_str(&format!(
        "  {} {}:{}  root stylesheet",
        short_name(&loc.file),
        primary.line,
        primary.col
    ));
    out
}

/// Goldens name the source file without its directory.
fn short_name(file: &str) -> &str {
    file.rsplit(['/', '\\']).next().unwrap_or(file)
}
