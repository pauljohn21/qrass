//! sasspile — a clean-room SCSS→CSS compiler whose control flow IS a **lazy rxrust
//! owned-value stream**.
//!
//! This is a fresh, minimal skeleton (the legacy tree/state-machine implementation was
//! cleared). It is rebuilt from scratch per OpenSpec change
//! `rebuild-sasspile-on-rxrust-lazy-stream`, governed by `.qoder/rules/rust.md` and
//! `openspec/config.yaml`: every stage is a cold observable of owned values; nesting is
//! data (not a walked tree); loops are `flat_map`; one-to-many is multicast; state/env is
//! a stream-carried owned accumulator; errors ride in-stream `Result`; exactly one
//! `subscribe` at the public edge materialises CSS. Behaviour oracle = `sass-spec/*.hrx`
//! + `element-plus`; every step gated by `ledger diff regress=0`.
//!
//! Nothing is implemented yet — `compile_string` is a stub so the workspace compiles.

pub mod expr;
pub mod lexer;
pub mod value;

use std::fmt;

/// Compiler error carried as an in-stream value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassError {
    pub message: String,
}

impl SassError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for SassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SassError {}

/// Compile SCSS/CSS text to expanded CSS. Currently a stub for the clean rebuild.
pub fn compile_string(_content: impl Into<String>) -> Result<String, SassError> {
    Ok(String::new())
}
