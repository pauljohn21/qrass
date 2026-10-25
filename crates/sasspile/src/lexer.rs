//! Lexer (push): `chars → Tok → structural events`, all as forward `scan_map` folds over an
//! owned accumulator (the `#{}`/interp depth is *data* in the accumulator, not call-stack
//! recursion). No AST, no back-walk: downstream consumes the event stream forward.

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

/// A structural event produced by the delimiter fold.
#[derive(Debug, Clone)]
pub enum Event {
    Open(Vec<Tok>),
    Decl { prop: Vec<Tok>, value: Vec<Tok> },
    Close,
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

    /// feed one char, emit 0..N tokens (owned accumulator, moved via `scan_map`).
    fn feed(&mut self, ch: char) -> Vec<Tok> {
        self.out.clear();
        if self.block {
            // inside /* .. */ loud comment: consume until */
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
        if self.hash {
            self.hash = false;
            if ch == '{' {
                self.flush_word();
                self.interp.push(Vec::new());
                return std::mem::take(&mut self.out);
            }
            self.buf.push('#');
        }
        if self.var.is_some() {
            if is_var_char(ch) {
                self.var.as_mut().unwrap().push(ch);
                return std::mem::take(&mut self.out);
            }
            let name = std::mem::take(self.var.as_mut().unwrap());
            self.var = None;
            self.push(Tok::Var(name));
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

/// First structural pass: `Tok → Vec<Event>` on delimiters only (no tree). Own accumulator.
#[derive(Default)]
struct MsgAcc {
    run: Vec<Tok>,
    events: Vec<Event>,
}

impl MsgAcc {
    fn feed(&mut self, tok: Tok) -> Vec<Event> {
        self.events.clear();
        match tok {
            Tok::Punct('{') => self.events.push(Event::Open(std::mem::take(&mut self.run))),
            Tok::Punct(';') => self.push_decl(),
            Tok::Punct('}') => {
                self.push_decl();
                self.events.push(Event::Close);
            }
            other => self.run.push(other),
        }
        self.events.clone()
    }

    fn push_decl(&mut self) {
        let run = std::mem::take(&mut self.run);
        if let Some(colon) = run.iter().position(|t| matches!(t, Tok::Punct(':'))) {
            let prop = run[..colon].to_vec();
            let value = run[colon + 1..].to_vec();
            let non_empty = render(&prop).chars().any(|c| !c.is_whitespace());
            if non_empty {
                self.events.push(Event::Decl { prop, value });
            }
        }
    }
}

/// Render a token run back to source text (P0: values/selectors pass through verbatim; the
/// full `Value`/`Expr` algebra arrives in P1). Interpolation is glued with no extra space.
pub fn render(toks: &[Tok]) -> String {
    let s = toks
        .iter()
        .map(|t| match t {
            Tok::Word(w) => w.clone(),
            Tok::Punct(c) => c.to_string(),
            Tok::Var(n) => format!("${n}"),
            Tok::Interp(inner) => render(inner),
        })
        .collect::<String>();
    canon(&s)
}

fn canon(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The full lexer: content → a cold `Shared` stream of structural events.
pub fn events(content: String) -> SharedBoxedObservable<'static, Event, Infallible> {
    Shared::from_iter(content.chars().collect::<Vec<char>>())
        .scan_map(LexAcc::default(), |acc, ch| acc.feed(ch))
        .flat_map(Shared::from_iter)
        .scan_map(MsgAcc::default(), |acc, tok| acc.feed(tok))
        .flat_map(Shared::from_iter)
        .box_it()
}
