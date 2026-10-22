//! Dart-style source-snippet rendering for errors, matching the annotation
//! blocks of the `expressions/if/error/*.hrx` goldens: a `,` corner, the
//! numbered source line, one marker run per annotation with an optional
//! trailing label, a `'` foot, and the module-load stack beneath it
//! (`<file> <line>:<col>  <label>`, `root stylesheet` for the entry file).
//!
//! Layout is derived from the goldens byte-for-byte: the marker line is
//! `"  |" + col spaces + marker×len (+ " " + label)`, so a 1-based column `C`
//! lines up with the source line's `"1 | "` gutter of width 4.

use crate::error::{Frame, Located, Marker, SassError};
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
///
/// The lines under the `'` foot are the module-load stack: one
/// `<file> <line>:<col>  <label>` per frame, the `<file> <line>:<col>` fields
/// left-aligned in a common column (the goldens pad the shorter ones before the
/// two-space label gap, e.g. `_midstream.scss 1:27  @use` / `input.scss 1:1` +
/// 6 spaces + `root stylesheet`). An empty stack means the error came from the
/// entry stylesheet, which renders as the single `root stylesheet` line.
pub(crate) fn snippet(message: &str, loc: &Located, markers: &[Marker], trace: &[Frame]) -> String {
    let Some(primary) = markers.first() else {
        return String::new();
    };
    let mut out = format!(
        "Error: {message}\n  ,\n{} | {}\n",
        primary.line,
        loc.excerpt.as_deref().unwrap_or("")
    );
    out.push_str(
        &markers
            .iter()
            .filter(|m| m.line == primary.line)
            .map(|m| {
                format!(
                    "  |{}{}{}\n",
                    " ".repeat(m.col as usize),
                    m.marker.to_string().repeat(m.len as usize),
                    m.label.as_ref().map_or(String::new(), |l| format!(" {l}"))
                )
            })
            .collect::<String>(),
    );
    out.push_str("  '\n");
    out.push_str(&footer(loc, primary.line, primary.col, trace));
    out
}

/// The lines under a snippet's `'` foot: the module-load stack, one
/// `<file> <line>:<col>  <label>` per frame, the `<file> <line>:<col>` fields
/// left-aligned in a common column (the goldens pad the shorter ones before the
/// two-space label gap, e.g. `_midstream.scss 1:27  @use` / `input.scss 1:1` +
/// 6 spaces + `root stylesheet`). An empty stack means the error came from the
/// entry stylesheet, which renders as the single `root stylesheet` line under the
/// location the golden gives it.
fn footer(loc: &Located, line: u32, col: u32, trace: &[Frame]) -> String {
    let root = Frame {
        file: loc.file.clone(),
        line,
        col,
        label: "root stylesheet".to_string(),
    };
    let frames: Vec<&Frame> = if trace.is_empty() { vec![&root] } else { trace.iter().collect() };
    let cells: Vec<String> = frames
        .iter()
        .map(|f| format!("{} {}:{}", short_name(&f.file), f.line, f.col))
        .collect();
    let width = cells.iter().map(|c| c.chars().count()).max().unwrap_or(0);
    cells
        .iter()
        .zip(&frames)
        .map(|(cell, f)| format!("  {cell}{}  {}", " ".repeat(width - cell.chars().count()), f.label))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Goldens name the source file without its directory.
fn short_name(file: &str) -> &str {
    file.rsplit(['/', '\\']).next().unwrap_or(file)
}
