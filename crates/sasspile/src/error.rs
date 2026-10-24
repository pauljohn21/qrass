//! Error type for the sasspile pipeline. Compiler failures ride the **in-stream
//! `Result` value channel** (the observable `Err` channel stays `Infallible`), so
//! every stage emits `Result<T, SassError>` and a single `drain` terminal short-
//! circuits on the first error.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Lex,
    Parse,
    Render,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassError {
    pub kind: ErrorKind,
    pub message: String,
}

impl SassError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for SassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for SassError {}
