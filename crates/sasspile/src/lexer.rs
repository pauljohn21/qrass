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

#[derive(Default)]
struct LexAcc {
    buf: String,
    /// saw `#`, deciding `#{` (interpolation) vs `#id`/`#hex` on the next char
    hash: bool,
    interp: String,
    idepth: u32,
}

impl LexAcc {
    fn feed(&mut self, ch: char) -> Vec<Tok> {
        let mut out = Vec::new();
        if self.idepth > 0 {
            match ch {
                '{' => {
                    self.idepth += 1;
                    self.interp.push(ch);
                }
                '}' => {
                    self.idepth -= 1;
                    self.interp.push('}');
                    if self.idepth == 0 {
                        out.push(Tok::Word(std::mem::take(&mut self.interp)));
                    }
                }
                c => self.interp.push(c),
            }
            return out;
        }
        if self.hash {
            self.hash = false;
            if ch == '{' {
                self.interp = std::mem::take(&mut self.buf);
                self.interp.push_str("#{");
                self.idepth = 1;
                return out;
            }
            self.buf.push('#');
        }
        match ch {
            '#' => self.hash = true,
            c if STRUCTURAL.contains(&c) => {
                let word = std::mem::take(&mut self.buf).trim().to_string();
                if !word.is_empty() {
                    out.push(Tok::Word(word));
                }
                out.push(Tok::Punct(c));
            }
            c => self.buf.push(c),
        }
        out
    }
}

pub(crate) fn token_stream(content: String) -> LocalBoxedObservable<'static, Tok, Infallible> {
    Local::from_iter(content.chars().collect::<Vec<char>>())
        .scan_map(LexAcc::default(), |acc, ch| acc.feed(ch))
        .flat_map(Local::from_iter)
        .box_it()
}
