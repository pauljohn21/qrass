//! Unified compiler error type carried across every pipeline stage.

use std::fmt;

use crate::span::{FileId, Files, Span, excerpt_line, line_col};

/// Which phase / semantic family an error belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Load,
    Lex,
    Parse,
    Eval,
    Builtins,
    Render,
    UndefinedVariable,
    UndefinedFunction,
    UndefinedMixin,
    InvalidArguments,
    Cycle,
    Unsupported,
    Config,
    Io,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Load => "load error",
            ErrorKind::Lex => "lexical error",
            ErrorKind::Parse => "parse error",
            ErrorKind::Eval => "evaluation error",
            ErrorKind::Builtins => "builtin error",
            ErrorKind::Render => "render error",
            ErrorKind::UndefinedVariable => "undefined variable",
            ErrorKind::UndefinedFunction => "undefined function",
            ErrorKind::UndefinedMixin => "undefined mixin",
            ErrorKind::InvalidArguments => "invalid arguments",
            ErrorKind::Cycle => "circular dependency",
            ErrorKind::Unsupported => "unsupported feature",
            ErrorKind::Config => "configuration error",
            ErrorKind::Io => "io error",
        }
    }
}

/// A resolved source location attached to an error.
#[derive(Clone, Debug)]
pub struct Located {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub excerpt: Option<String>,
}

/// The single error type every stage emits into its observable stream.
#[derive(Clone, Debug)]
pub struct SassError {
    pub kind: ErrorKind,
    pub message: String,
    pub located: Option<Located>,
    /// Extra trailing context (e.g. dependency cycle path, candidate list).
    pub notes: Vec<String>,
}

impl SassError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            located: None,
            notes: Vec::new(),
        }
    }

    /// Attach a span, resolving it to line/col and a source excerpt when possible.
    pub fn with_span(mut self, files: &Files, file: FileId, span: Span) -> Self {
        if let Some(content) = files.content(file) {
            let lc = line_col(content, span.start);
            self.located = Some(Located {
                file: files.name(file).to_owned(),
                line: lc.line,
                col: lc.col,
                excerpt: excerpt_line(content, lc),
            });
        } else {
            self.located = Some(Located {
                file: files.name(file).to_owned(),
                line: 0,
                col: 0,
                excerpt: None,
            });
        }
        self
    }

    /// Attach an already-resolved location.
    pub fn with_located(mut self, located: Located) -> Self {
        self.located = Some(located);
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn io(err: std::io::Error, path: &str) -> Self {
        let mut e = Self::new(ErrorKind::Io, format!("{path}: {}", err));
        e.located = Some(Located {
            file: path.to_owned(),
            line: 0,
            col: 0,
            excerpt: None,
        });
        e
    }
}

impl fmt::Display for SassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)?;
        if let Some(loc) = &self.located {
            write!(f, "\n  --> {}:{}:{}", loc.file, loc.line, loc.col)?;
            if let Some(ex) = &loc.excerpt {
                write!(f, "\n   | {}", ex.trim_end())?;
            }
        }
        for n in &self.notes {
            write!(f, "\n   = {n}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SassError {}
