//! Lexer — the two owned accumulator **values** behind the `Lexer`/`Structurize` phase kernels
//! (`crate::phases`). Each is a forward fold step over a single character or token; the
//! `#{}`/interp depth is *data* inside the accumulator, never a call stack.
//!
//! They hand their output to an **emit sink** (`&mut dyn FnMut(..)`) that the kernel drives with
//! `observer.next(..)`, so a `char → 0..N Tok` / `Tok → 0..N Event` fan-out is emitted item by
//! item. There is deliberately no returned `Vec`: batching a phase's output and flattening it
//! again downstream was the non-framework shape this refactor removes.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// a verbatim run of source text (never spans whitespace or an arithmetic sign)
    Word(String),
    /// one whitespace character — **data**, because Sass reads `10px -5px` as a
    /// space-separated list but `10px - 5px` as subtraction
    Space(char),
    /// `+`, or a `-` that stands alone at a token boundary (an operator, decided downstream by the
    /// spacing law). A `-` that belongs to an identifier is **not** an `Op`: it stays inside the
    /// `Word`, because a classification never gets finer than the lexer's lexemes — `-a-calc(` cut
    /// into `Op('-')+Word("a")+…` made "is this a vendor-prefixed CSS call?" unanswerable
    /// (`css/functions/special/prefixed/lowercase::*/script_like`).
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

/// The sink a kernel installs for its downstream observer: `FnMut(Item)` that moves each
/// produced item straight into the stream.
pub type Emit<'a, T> = &'a mut dyn FnMut(T);

const STRUCTURAL: [char; 5] = ['{', '}', ';', ':', ','];

fn is_var_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// A character that continues an identifier lexeme — the test that decides a pending `-`'s role.
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

#[derive(Default)]
pub struct LexAcc {
    buf: String,
    hash: bool,
    var: Option<String>,
    interp: Vec<Vec<Tok>>,
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
    /// a `-` seen at a token boundary whose role is not decided yet: Sass's spacing law needs the
    /// next character, exactly like the pending-slash fold
    minus: bool,
}

impl LexAcc {
    /// A completed token: an open interpolation still collects its own inner tokens, at the top
    /// level the token goes straight into the stream.
    fn push(&mut self, tok: Tok, emit: Emit<'_, Tok>) {
        match self.interp.last_mut() {
            Some(inner) => inner.push(tok),
            None => emit(tok),
        }
    }

    fn flush_word(&mut self, emit: Emit<'_, Tok>) {
        if !self.buf.is_empty() {
            let word = std::mem::take(&mut self.buf);
            self.push(Tok::Word(word), emit);
        }
    }

    /// Push the `// …` text collected so far as one [`Tok::Silent`] and restart the run.
    fn take_comment(&mut self, emit: Emit<'_, Tok>) {
        if !self.comment.is_empty() {
            let text = std::mem::take(&mut self.comment);
            self.push(Tok::Silent(text), emit);
        }
    }

    /// Feed one character, emitting 0..N owned tokens into `emit`.
    pub fn feed(&mut self, ch: char, emit: Emit<'_, Tok>) {
        if self.block {
            // inside /* .. */ loud comment: consume until */
            if self.block_star && ch == '/' {
                self.block = false;
                self.block_star = false;
            } else {
                self.block_star = ch == '*';
            }
            return;
        }
        if self.line {
            // A comment swallows the line, but never a declaration terminator: a custom
            // property keeps its `// …` text and still ends at `;` (`--b: c // comment;`).
            match ch {
                '\n' => {
                    self.line = false;
                    self.take_comment(emit);
                    self.buf.push(' ');
                }
                ';' => {
                    self.take_comment(emit);
                    self.push(Tok::Punct(';'), emit);
                }
                c => self.comment.push(c),
            }
            return;
        }
        if self.slash {
            self.slash = false;
            if ch == '*' {
                self.block = true;
                self.block_star = false;
                return;
            }
            if ch == '/' {
                self.flush_word(emit);
                self.line = true;
                self.comment = String::from("//");
                return;
            }
            self.buf.push('/');
        }
        if self.hash {
            self.hash = false;
            if ch == '{' {
                self.flush_word(emit);
                self.interp.push(Vec::new());
                return;
            }
            self.buf.push('#');
        }
        if self.var.is_some() {
            if is_var_char(ch) {
                self.var.as_mut().unwrap().push(ch);
                return;
            }
            let name = std::mem::take(self.var.as_mut().unwrap());
            self.var = None;
            self.push(Tok::Var(name), emit);
        }
        // A pending `-` resolves against this character: an identifier char continues the lexeme
        // (vendor prefix, or an in-ident hyphen), anything else makes the `-` a standalone
        // operator/sign and lets `ch` be handled on its own below.
        if self.minus {
            self.minus = false;
            if is_ident_char(ch) {
                self.buf.push('-');
            } else {
                self.flush_word(emit);
                self.push(Tok::Op('-'), emit);
            }
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
                self.flush_word(emit);
                self.hash = true;
            }
            '$' => {
                self.flush_word(emit);
                self.var = Some(String::new());
            }
            '}' if !self.interp.is_empty() => {
                self.flush_word(emit);
                let inner = self.interp.pop().unwrap();
                self.push(Tok::Interp(inner), emit);
            }
            c if STRUCTURAL.contains(&c) => {
                self.flush_word(emit);
                self.push(Tok::Punct(c), emit);
            }
            c if c.is_whitespace() => {
                self.flush_word(emit);
                self.push(Tok::Space(c), emit);
            }
            '+' => {
                self.flush_word(emit);
                self.push(Tok::Op(ch), emit);
            }
            // Defer: whether a `-` is Sass's operator (whitespace on both sides) or part of an
            // identifier (`-a-calc(`, `10px -5px`) is only knowable once the next char arrives.
            '-' => self.minus = true,
            c => self.buf.push(c),
        }
    }
}

/// First structural pass: `Tok → Event` on delimiters only (no tree). Own accumulator.
#[derive(Default)]
pub struct MsgAcc {
    run: Vec<Tok>,
}

impl MsgAcc {
    /// Feed one token, emitting 0..N structural events into `emit`.
    pub fn feed(&mut self, tok: Tok, emit: Emit<'_, Event>) {
        match tok {
            Tok::Punct('{') => {
                emit(Event::Open(std::mem::take(&mut self.run)));
            }
            Tok::Punct(';') => self.push_decl(emit),
            Tok::Punct('}') => {
                self.push_decl(emit);
                emit(Event::Close);
            }
            other => self.run.push(other),
        }
    }

    fn push_decl(&mut self, emit: Emit<'_, Event>) {
        let run = std::mem::take(&mut self.run);
        if let Some(colon) = run.iter().position(|t| matches!(t, Tok::Punct(':'))) {
            let prop = run[..colon].to_vec();
            let value = run[colon + 1..].to_vec();
            let non_empty = render(&prop).chars().any(|c| !c.is_whitespace());
            if non_empty {
                emit(Event::Decl { prop, value });
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
