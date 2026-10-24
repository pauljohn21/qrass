//! Lexer: an owned char `from_iter` folded through `scan_map` (an owned `LexAcc` DFA is
//! the moved accumulator), emitting 0..N tokens flattened with `flat_map`. One structured
//! pass so downstream builds values/selectors from tokens (never re-scanning a rebuilt
//! string):
//! - `$name` → `Tok::Var(name)` (name excludes `$`);
//! - `#{ … }` → `Tok::Interp(inner_tokens)` — recognised anywhere, incl. inside `(...)`;
//!   `#{}` depth is a data stack in the accumulator, not recursion;
//! - `; : , { }` block/declaration delimiters → `Tok::Punct`;
//! - `/* … */` loud comments dropped; every other run → `Tok::Word` (verbatim text).

use std::convert::Infallible;

use rxrust::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// a verbatim run of source text (may contain spaces)
    Word(String),
    Punct(char),
    /// a `$name` reference (name excludes the leading `$`)
    Var(String),
    /// the inner tokens of a `#{ … }` interpolation
    Interp(Vec<Tok>),
}

const STRUCTURAL: [char; 5] = ['{', '}', ';', ':', ','];

fn is_var_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

#[derive(Default)]
struct LexAcc {
    buf: String,
    hash: bool,
    var: Option<String>,
    interp: Vec<Vec<Tok>>,
    out: Vec<Tok>,
    slash: bool,
    block: bool,
    block_star: bool,
}

impl LexAcc {
    fn push(&mut self, tok: Tok) {
        match self.interp.last_mut() {
            Some(inner) => inner.push(tok),
            None => self.out.push(tok),
        }
    }

    fn flush_word(&mut self) {
        if !self.buf.is_empty() {
            let word = std::mem::take(&mut self.buf);
            self.push(Tok::Word(word));
        }
    }

    fn feed(&mut self, ch: char) -> Vec<Tok> {
        self.out.clear();
        // loud block comment /* .. */
        if self.block {
            if self.block_star && ch == '/' {
                self.block = false;
                self.block_star = false;
            } else {
                self.block_star = ch == '*';
            }
            return std::mem::take(&mut self.out);
        }
        if self.slash {
            self.slash = false;
            if ch == '*' {
                self.block = true;
                self.block_star = false;
                return std::mem::take(&mut self.out);
            }
            self.buf.push('/');
        }
        // a pending '#' resolves to interpolation `#{` or a literal hash
        if self.hash {
            self.hash = false;
            if ch == '{' {
                self.flush_word();
                self.interp.push(Vec::new());
                return std::mem::take(&mut self.out);
            }
            self.buf.push('#');
        }
        // a pending `$name`
        if self.var.is_some() {
            if is_var_char(ch) {
                self.var.as_mut().unwrap().push(ch);
                return std::mem::take(&mut self.out);
            }
            let name = std::mem::take(self.var.as_mut().unwrap());
            self.var = None;
            self.push(Tok::Var(name));
            // fall through to handle `ch` normally below
        }
        match ch {
            '/' => self.slash = true,
            '#' => {
                self.flush_word();
                self.hash = true;
            }
            '$' => {
                self.flush_word();
                self.var = Some(String::new());
            }
            '}' if !self.interp.is_empty() => {
                self.flush_word();
                let inner = self.interp.pop().unwrap();
                self.push(Tok::Interp(inner));
            }
            c if STRUCTURAL.contains(&c) => {
                self.flush_word();
                self.push(Tok::Punct(c));
            }
            c => self.buf.push(c),
        }
        std::mem::take(&mut self.out)
    }
}

pub fn token_stream(content: String) -> LocalBoxedObservable<'static, Tok, Infallible> {
    Local::from_iter(content.chars().collect::<Vec<char>>())
        .scan_map(LexAcc::default(), |acc, ch| acc.feed(ch))
        .flat_map(Local::from_iter)
        .box_it()
}
