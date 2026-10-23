//! Per-state SCSS token scanners for [`Lexer`]. Split out of `lexer.rs` to keep
//! each file well under the 500-line limit; every method advances `self.i` to the
//! token end and returns via the `spanned`/`err` helpers defined in the parent.

use super::{Lexer, LxResult};
use crate::token::{Piece, Punct, Quote, Token};

impl Lexer {
    pub(crate) fn lex_block_comment(&mut self, start: usize) -> LxResult {
        self.i += 2;
        loop {
            if self.i + 1 >= self.len {
                return Err(self.err("unterminated block comment", start));
            }
            if self.cs[self.i] == '*' && self.cs[self.i + 1] == '/' {
                self.i += 2;
                break;
            }
            self.i += 1;
        }
        let raw: String = self.cs[start..self.i].iter().collect();
        self.spanned(Token::Comment(raw), start)
    }

    /// Consume `#{...}` (positioned at `#`), returning the inner raw text.
    fn lex_interp_raw(&mut self) -> Result<String, crate::error::SassError> {
        let open = self.i;
        self.i += 2;
        let mut depth = 1usize;
        let mut buf = String::new();
        while self.i < self.len {
            let ch = self.cs[self.i];
            match ch {
                '#' if self.peek(1) == Some('{') => {
                    depth += 1;
                    buf.push_str("#{");
                    self.i += 2;
                }
                '{' => {
                    depth += 1;
                    buf.push(ch);
                    self.i += 1;
                }
                '}' => {
                    depth -= 1;
                    self.i += 1;
                    if depth == 0 {
                        return Ok(buf);
                    }
                    buf.push('}');
                }
                q @ ('"' | '\'') => {
                    buf.push(q);
                    self.i += 1;
                    while self.i < self.len {
                        let c = self.cs[self.i];
                        buf.push(c);
                        self.i += 1;
                        if c == '\\' && self.i < self.len {
                            buf.push(self.cs[self.i]);
                            self.i += 1;
                        } else if c == q {
                            break;
                        }
                    }
                }
                _ => {
                    buf.push(ch);
                    self.i += 1;
                }
            }
        }
        Err(self.err("unterminated interpolation", open))
    }

    /// Read one plain run of identifier characters (Unicode letters included).
    fn read_word_run(&mut self, buf: &mut String) {
        while self.i < self.len {
            let ch = self.cs[self.i];
            if ch.is_alphanumeric() || ch == '_' {
                buf.push(ch);
                self.i += 1;
            } else if ch == '-' {
                let next = self.peek(1);
                if next.is_some_and(|n| n.is_alphanumeric() || n == '_' || n == '-') {
                    buf.push('-');
                    self.i += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    /// Lex an identifier-ish run, merging adjacent `#{...}` into Interpolated.
    pub(crate) fn lex_word(&mut self, start: usize) -> LxResult {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut text = String::new();
        loop {
            if self.i < self.len && self.cs[self.i] == '#' && self.peek(1) == Some('{') {
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Expr(self.lex_interp_raw()?));
                continue;
            }
            let before = self.i;
            if self.i < self.len && self.cs[self.i] == '.' && text.is_empty() && pieces.is_empty() {
                text.push('.');
                self.i += 1;
            }
            self.read_word_run(&mut text);
            if self.i == before && text.is_empty() && pieces.is_empty() {
                return Err(self.err("expected identifier", start));
            }
            break;
        }
        if pieces.is_empty() {
            if text.eq_ignore_ascii_case("url")
                && self.peek(0) == Some('(')
                && !matches!(self.peek(1), Some('"') | Some('\'') | Some('$') | Some('#'))
            {
                return self.lex_url(start);
            }
            self.spanned(Token::Ident(text), start)
        } else {
            if !text.is_empty() {
                pieces.push(Piece::Text(text));
            }
            self.spanned(
                Token::Interpolated {
                    quote: None,
                    pieces,
                },
                start,
            )
        }
    }

    fn lex_url(&mut self, start: usize) -> LxResult {
        self.i += 1;
        let mut depth = 1usize;
        while self.i < self.len {
            match self.cs[self.i] {
                '(' => {
                    depth += 1;
                    self.i += 1;
                }
                ')' => {
                    depth -= 1;
                    self.i += 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => self.i += 1,
            }
        }
        if depth != 0 {
            return Err(self.err("unterminated url()", start));
        }
        let raw: String = self.cs[start..self.i].iter().collect();
        self.spanned(Token::Url(raw), start)
    }

    pub(crate) fn lex_string(&mut self, start: usize) -> LxResult {
        let q = self.cs[self.i];
        let quote = if q == '"' {
            Quote::Double
        } else {
            Quote::Single
        };
        self.i += 1;
        let mut pieces: Vec<Piece> = Vec::new();
        let mut text = String::new();
        loop {
            if self.i >= self.len || self.cs[self.i] == '\n' {
                return Err(self.err("unterminated string", start));
            }
            let ch = self.cs[self.i];
            match ch {
                '\\' => {
                    text.push('\\');
                    self.i += 1;
                    if self.i < self.len {
                        text.push(self.cs[self.i]);
                        self.i += 1;
                    }
                }
                c if c == q => {
                    self.i += 1;
                    break;
                }
                '#' if self.peek(1) == Some('{') => {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                    pieces.push(Piece::Expr(self.lex_interp_raw()?));
                }
                _ => {
                    text.push(ch);
                    self.i += 1;
                }
            }
        }
        if pieces.is_empty() {
            self.spanned(Token::Str { text, quote }, start)
        } else {
            if !text.is_empty() {
                pieces.push(Piece::Text(text));
            }
            self.spanned(
                Token::Interpolated {
                    quote: Some(quote),
                    pieces,
                },
                start,
            )
        }
    }

    pub(crate) fn lex_at_keyword(&mut self, start: usize) -> LxResult {
        self.i += 1;
        let mut text = String::new();
        if self.peek(0) == Some('-') {
            text.push('-');
            self.i += 1;
        }
        self.read_word_run(&mut text);
        if text.is_empty() {
            return Err(self.err("expected at-rule keyword", start));
        }
        self.spanned(Token::AtKeyword(text), start)
    }

    pub(crate) fn lex_variable(&mut self, start: usize) -> LxResult {
        self.i += 1;
        let mut text = String::new();
        self.read_word_run(&mut text);
        // A lone `$` lexes as an empty-named variable token: the goldens raise the
        // context-specific parse error (`Expected variable, mixin, or function
        // name` after `@forward … show`, `Expected identifier.` inside
        // `with (…)`), which a lexical error here could never word correctly.
        self.spanned(Token::Variable(text), start)
    }

    pub(crate) fn lex_placeholder(&mut self, start: usize) -> LxResult {
        self.i += 1;
        let mut text = String::new();
        self.read_word_run(&mut text);
        self.spanned(Token::Placeholder(text), start)
    }

    pub(crate) fn lex_hash(&mut self, start: usize) -> LxResult {
        self.i += 1;
        let mut digits = String::new();
        while self.i < self.len && self.cs[self.i].is_ascii_hexdigit() {
            digits.push(self.cs[self.i]);
            self.i += 1;
        }
        // Digit run stopped at the buffer edge (not EOF): more hex digits may
        // still arrive, so this decision is provisional — mark and let the
        // caller rewind & wait rather than committing prematurely.
        if self.i >= self.len && !self.is_eof {
            self.provisional = true;
        }
        let boundary = self
            .peek(0)
            .is_none_or(|c| !c.is_alphanumeric() && c != '-' && c != '_');
        if matches!(digits.len(), 3 | 4 | 6 | 8) && boundary {
            self.spanned(Token::HexColor(format!("#{digits}")), start)
        } else {
            self.i = start + 1;
            self.spanned(Token::Punct(Punct::Hash), start)
        }
    }

    pub(crate) fn lex_number(&mut self, start: usize) -> LxResult {
        let mut buf = String::new();
        if self.cs[self.i] == '-' {
            buf.push('-');
            self.i += 1;
        }
        while self.i < self.len && self.cs[self.i].is_ascii_digit() {
            buf.push(self.cs[self.i]);
            self.i += 1;
        }
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            buf.push('.');
            self.i += 1;
            while self.i < self.len && self.cs[self.i].is_ascii_digit() {
                buf.push(self.cs[self.i]);
                self.i += 1;
            }
        }
        // scientific notation only when digits actually follow (else it is a unit)
        if matches!(self.peek(0), Some('e' | 'E'))
            && (matches!(self.peek(1), Some(c) if c.is_ascii_digit())
                || (matches!(self.peek(1), Some('-' | '+'))
                    && matches!(self.peek(2), Some(c) if c.is_ascii_digit())))
        {
            buf.push(self.cs[self.i]);
            self.i += 1;
            if matches!(self.cs[self.i], '-' | '+') {
                buf.push(self.cs[self.i]);
                self.i += 1;
            }
            while self.i < self.len && self.cs[self.i].is_ascii_digit() {
                buf.push(self.cs[self.i]);
                self.i += 1;
            }
        }
        let value: f64 = buf
            .parse()
            .map_err(|_| self.err(format!("invalid number '{buf}'"), start))?;
        let mut unit = String::new();
        if self.i < self.len {
            let c = self.cs[self.i];
            if c == '%' {
                unit.push('%');
                self.i += 1;
            } else if c.is_alphabetic() {
                self.read_word_run(&mut unit);
            }
        }
        let unit = if unit.is_empty() { None } else { Some(unit) };
        self.spanned(Token::Number { value, unit }, start)
    }
}
