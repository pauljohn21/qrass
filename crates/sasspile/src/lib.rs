//! sasspile — a clean-room SCSS→CSS compiler whose control flow IS a **lazy rxrust
//! owned-value stream**. Every stage is an operator over owned values; nesting is data
//! (not a walked tree); CSS is merged only at the single terminal `subscribe`. Rebuilt
//! from scratch per OpenSpec `rebuild-sasspile-on-rxrust-lazy-stream`; governed by
//! `.qoder/rules/rust.md` and `openspec/config.yaml`. Behaviour oracle = `sass-spec/*.hrx`
//! + `element-plus`; every step gated by `ledger diff regress=0`.

pub mod expr;
pub mod lexer;
mod parse;
mod render;
pub mod value;

use std::fmt;

use rxrust::prelude::*;

use crate::lexer::token_stream;

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

/// Compile SCSS/CSS text to expanded CSS. The whole compiler is the observable chain
/// below: chars→tokens→(`scan_map`)structural messages→(`scan_map` owned `Acc`)flat
/// leaves→(`map`)CSS blocks→(`reduce`)merged text, met by exactly ONE `subscribe`.
pub fn compile_string(content: impl Into<String>) -> Result<String, SassError> {
    let _span = tracing::info_span!("compile").entered();
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, SassError>>();
    token_stream(content.into())
        .scan_map(parse::MsgAcc::default(), |acc, tok| acc.feed(tok))
        .flat_map(Local::from_iter)
        .scan_map(parse::Acc::new(), |acc, msg| acc.feed(msg))
        .flat_map(Local::from_iter)
        .map(|leaf| render::leaf_block(&leaf))
        .reduce_initial(String::new(), |mut css, chunk| {
            css.push_str(&chunk);
            css
        })
        .map(Ok::<_, SassError>)
        .subscribe(move |out| {
            tx.send(out).ok();
        });
    rx.recv()
        .unwrap_or_else(|_| Err(SassError::new("no output")))
}
