//! The lexer as a stream. An owned char `from_iter` folded through `scan_map`
//! (the pending-token `String` is the moved accumulator), each step emitting 0..2
//! tokens, flattened with `flat_map` into a `Tok` stream. No cursor, no `&mut self`
//! object — the buffer lives in the stream and moves along it.

use std::convert::Infallible;

use rxrust::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    Word(String),
    Punct(char),
}

const STRUCTURAL: [char; 5] = ['{', '}', ';', ':', ','];

pub(crate) fn token_stream(content: String) -> LocalBoxedObservable<'static, Tok, Infallible> {
    Local::from_iter(content.chars().collect::<Vec<char>>())
        .scan_map(String::new(), |buf, ch| {
            match ch {
                c if STRUCTURAL.contains(&c) => {
                    let word = buf.trim().to_string();
                    buf.clear();
                    let mut out = Vec::new();
                    if !word.is_empty() {
                        out.push(Tok::Word(word));
                    }
                    out.push(Tok::Punct(c));
                    out
                }
                c => {
                    buf.push(c);
                    Vec::new()
                }
            }
        })
        .flat_map(Local::from_iter)
        .box_it()
}
