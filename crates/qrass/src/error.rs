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

/// One frame of the module-load stack the goldens print under the `'` foot of a
/// snippet: `<file> <line>:<col>  <label>`. The innermost frame is where the
/// error was raised; every ancestor shows the `@use`/`@forward` statement that
/// pulled the next file in, labelled by the keyword through which *it* entered
/// (the entry file is labelled `root stylesheet`) — the shape of
/// `directives/forward/error/with.hrx` `undefined`/`namespace`/`nested`.
#[derive(Clone, Debug)]
pub struct Frame {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub label: String,
}

/// One annotation run of a rendered source snippet: `marker` repeated `len`
/// times starting at 1-based `col` of `line`, with an optional label (the
/// `arbitrary substitution` / `sass() expression` captions of the
/// `expressions/if/error/raw.hrx` goldens).
#[derive(Clone, Debug)]
pub struct Marker {
    pub marker: char,
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub label: Option<String>,
}

/// The single error type every stage emits into its observable stream.
#[derive(Clone, Debug)]
pub struct SassError {
    pub kind: ErrorKind,
    pub message: String,
    pub located: Option<Located>,
    /// Extra trailing context (e.g. dependency cycle path, candidate list).
    pub notes: Vec<String>,
    /// Golden-style snippet annotations; boxed because a snippet is the rare
    /// path and the error travels through every stage's stream (see
    /// [`crate::diag`]). Non-empty switches rendering to the annotated block.
    pub snippet: Option<Box<Snippet>>,
}

/// Annotation runs attached to an error for snippet rendering, plus the
/// module-load stack rendered as the snippet's trailing location lines. An
/// empty stack keeps the single `<file> <line>:<col>  root stylesheet` line the
/// entry stylesheet's errors have always produced. Both live behind one box so
/// the error type stays small on the `Err` path of every stage.
#[derive(Clone, Debug)]
pub struct Snippet {
    pub markers: Vec<Marker>,
    pub trace: Vec<Frame>,
}

impl SassError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            located: None,
            notes: Vec::new(),
            snippet: None,
        }
    }

    /// Read [`Snippet::trace`] off a frame stack the loader captured. Only a
    /// snippet renders location lines, so an error without one keeps its plain
    /// `kind: message` form.
    pub fn with_trace(mut self, trace: Vec<Frame>) -> Self {
        if let Some(sn) = &mut self.snippet {
            sn.trace = trace;
        }
        self
    }

    /// Whether the loader has already captioned this error with its frames.
    pub fn has_trace(&self) -> bool {
        self.snippet.as_ref().is_some_and(|sn| !sn.trace.is_empty())
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

    /// Attach the snippet annotations a golden-style block renders.
    pub fn with_markers(mut self, markers: Vec<Marker>) -> Self {
        if !markers.is_empty() {
            self.snippet = Some(Box::new(Snippet {
                markers,
                trace: Vec::new(),
            }));
        }
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
        if let Some(sn) = &self.snippet
            && let Some(loc) = &self.located
        {
            return write!(
                f,
                "{}",
                crate::diag::snippet(&self.message, loc, &sn.markers, &sn.trace)
            );
        }
        write!(f, "{}: {}", self.kind.as_str(), self.message)?;
        if let Some(loc) = &self.located {
            write!(f, "\n  --> {}:{}:{}", loc.file, loc.line, loc.col)?;
            if let Some(ex) = &loc.excerpt {
                write!(f, "\n   | {}", ex.trim_end())?;
            }
        }
        // each note rides its own annotated line, in order
        self.notes
            .iter()
            .try_fold((), |(), n| write!(f, "\n   = {n}"))?;
        Ok(())
    }
}

impl std::error::Error for SassError {}
