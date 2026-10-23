//! The parser's public reactive API, split out of `parser.rs` for the source-size
//! budget (OpenSpec `refactor-rfp-strict` task 5b.2). Pure code motion: `parse_stream`
//! is the cold `scan_map` statement stream the pipeline drives, and `parse` is its
//! synchronous drain wrapper (so the crate keeps a single parsing fold).

use std::convert::Infallible;

use rxrust::prelude::*;

use super::{check_module_placement, Parser, PsResult};
use crate::ast::Stmt;
use crate::error::SassError;
use crate::span::FileId;
use crate::token::Spanned;

/// Cold statement stream: tokens flow one at a time through `scan_map`; completed
/// statements leave via `flat_map`. Errors ride the value channel.
pub fn parse_stream(
    file: FileId,
    name: String,
    content: String,
    tokens: Vec<Spanned>,
) -> LocalBoxedObservable<'static, PsResult, Infallible> {
    Local::from_iter(tokens.into_iter().map(Some).chain(std::iter::once(None)))
        .scan_map(Parser::new(file, name, content), |p, t| p.step(t))
        .filter(|batch| !batch.is_empty())
        .flat_map(Local::from_iter)
        .box_it()
}

/// Synchronous convenience API: parse a full token vector into a program.
/// Drains the very stream the pipeline runs, so there is one parsing fold in
/// the crate.
pub fn parse(
    file: FileId,
    name: String,
    content: String,
    tokens: Vec<Spanned>,
) -> Result<Vec<Stmt>, SassError> {
    let program = crate::drain::drain(parse_stream(file, name, content, tokens))?;
    check_module_placement(&program)?;
    Ok(program)
}
