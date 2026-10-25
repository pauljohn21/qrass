//! Lexer (push): `chars → Tok → structural events`, all as forward `scan_map` folds over an
//! owned accumulator (the `#{}`/interp depth is *data* in the accumulator, not call-stack
//! recursion). No AST, no back-walk: downstream consumes the event stream forward.

use std::convert::Infallible;

use rxrust::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// a verbatim run of source text (never spans whitespace or an arithmetic sign)
    Word(String),
    /// one whitespace character — **data**, because Sass reads `10px -5px` as a
    /// space-separated list but `10px - 5px` as subtraction
    Space(char),
    /// `+` or `-` standing at a token boundary (an operator or a sign, decided downstream)
    Op(char),
    Punct(char),
    /// a `$name` reference (name excludes the leading `$`)
    Var(String),
    /// the inner tokens of a `#{ … }` interpolation
    Interp(Vec<Tok>),
    /// a `// …` silent comment **including its slashes**, carried as data so each consumer
    /// decides whether it survives into CSS: selectors and ordinary declaration values drop
    /// it, a custom property (`--x:`) keeps it verbatim because its value is raw CSS text.
    Silent(String),
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
    /// inside a `// … end-of-line` comment, collecting its text
    line: bool,
    /// the run being collected for [`Tok::Silent`] (starts with `//`)
    comment: String,
    /// last source character consumed outside comments — the lookbehind that keeps a
    /// protocol slash (`url(http://x)`, `url(//cdn/x)`) from reading as a silent comment
    prev: Option<char>,
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

    /// Push the `// …` text collected so far as one [`Tok::Silent`] and restart the run.
    fn take_comment(&mut self) {
        if !self.comment.is_empty() {
            let text = std::mem::take(&mut self.comment);
            self.push(Tok::Silent(text));
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
        if self.line {
            // A comment swallows the line, but never a declaration terminator: a custom
            // property keeps its `// …` text and still ends at `;` (`--b: c // comment;`).
            match ch {
                '\n' => {
                    self.line = false;
                    self.take_comment();
                    self.buf.push(' ');
                }
                ';' => {
                    self.take_comment();
                    self.push(Tok::Punct(';'));
                }
                c => self.comment.push(c),
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
            if ch == '/' {
                self.flush_word();
                self.line = true;
                self.comment = String::from("//");
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
        // Lookbehind for the slash rule below: capture the previous character, then record
        // the one about to be consumed.
        let prev = self.prev;
        self.prev = Some(ch);
        match ch {
            // A slash only opens a pending comment/division when it cannot be part of a
            // URL authority (`http://`, `url(//cdn`); the pending-flag fold resolves the
            // second slash in the next step.
            '/' if prev != Some(':') && prev != Some('(') => self.slash = true,
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
            c if c.is_whitespace() => {
                self.flush_word();
                self.push(Tok::Space(c));
            }
            '+' | '-' => {
                self.flush_word();
                self.push(Tok::Op(ch));
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
/// A silent comment is **dropped** — see [`render_raw`] for the context that keeps it.
pub fn render(toks: &[Tok]) -> String {
    canon(&text_of(toks, false))
}

/// Like [`render`] but keeps `// …` comment text, for a custom-property value whose content
/// is raw CSS and must survive untouched.
pub fn render_raw(toks: &[Tok]) -> String {
    canon(&text_of(toks, true))
}

fn text_of(toks: &[Tok], keep_silent: bool) -> String {
    toks.iter()
        .map(|t| match t {
            Tok::Word(w) => w.clone(),
            Tok::Space(c) => c.to_string(),
            Tok::Op(c) => c.to_string(),
            Tok::Punct(c) => c.to_string(),
            Tok::Var(n) => format!("${n}"),
            Tok::Interp(inner) => text_of(inner, keep_silent),
            Tok::Silent(c) if keep_silent => c.clone(),
            Tok::Silent(_) => String::new(),
        })
        .collect()
}

/// One token as source text, silent comments dropped — the piece an interpolated run is
/// assembled from without re-scanning the rendered string.
pub fn token_text(tok: &Tok) -> String {
    text_of(std::slice::from_ref(tok), false)
}

/// Collapse runs of whitespace to single spaces and trim, the canonical CSS text form.
pub fn canon(text: &str) -> String {
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
