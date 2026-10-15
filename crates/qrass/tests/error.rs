//! Tests for the unified error type: span resolution + Display formatting.

use qrass::error::{ErrorKind, SassError};
use qrass::span::{BytePos, Files, Span};

const SRC: &str = "a {\n  color: ;\n}\n";

fn registry() -> Files {
    let mut files = Files::default();
    files.add("test.scss".into(), SRC.into());
    files
}

#[test]
fn error_carries_resolved_line_col() {
    let files = registry();
    // Position of the empty declaration value: line 2, col 10 (";")
    let span = Span::point(0, BytePos(13));
    let err = SassError::new(ErrorKind::Parse, "expected expression").with_span(&files, 0, span);
    let located = err.located.expect("span resolved");
    assert_eq!(located.file, "test.scss");
    assert_eq!((located.line, located.col), (2, 10));
    assert_eq!(located.excerpt.as_deref(), Some("  color: ;"));
}

#[test]
fn display_shows_kind_location_and_excerpt() {
    let files = registry();
    let err = SassError::new(ErrorKind::Parse, "expected expression").with_span(
        &files,
        0,
        Span::point(0, BytePos(13)),
    );
    let text = err.to_string();
    assert!(
        text.starts_with("parse error: expected expression"),
        "{text}"
    );
    assert!(text.contains("--> test.scss:2:10"), "{text}");
    assert!(text.contains("|   color: ;"), "{text}");
}

#[test]
fn display_includes_notes() {
    let err = SassError::new(ErrorKind::Cycle, "module cycle").note("a.scss -> b.scss -> a.scss");
    let text = err.to_string();
    assert!(text.contains("circular dependency: module cycle"), "{text}");
    assert!(text.contains("= a.scss -> b.scss -> a.scss"), "{text}");
}

#[test]
fn io_error_reports_path() {
    let err = SassError::io(std::io::Error::other("nope"), "missing.scss");
    assert_eq!(err.kind, ErrorKind::Io);
    assert!(err.to_string().contains("missing.scss"), "{err}");
}
