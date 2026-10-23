//! Per-state SCSS token scanners for [`Lexer`]. Split out of `lexer.rs` to keep
//! each file well under the 500-line limit; every method advances `self.i` to the
//! token end and returns via the `spanned`/`err` helpers defined in the parent.

use super::{Lexer, LxResult};
use crate::token::{Piece, Punct, Quote, Token};

impl Lexer {
    pub(crate) fn lex_block_comment(&mut self, start: usize) -> LxResult {
        // the closing `*/` is the first one at least two chars past the opener
        let close = (start + 2..self.len.saturating_sub(1))
            .find(|&j| self.cs[j] == '*' && self.cs[j + 1] == '/');
        match close {
            Some(j) => {
                self.i = j + 2;
                let raw: String = self.cs[start..self.i].iter().collect();
                self.spanned(Token::Comment(raw), start)
            }
            None => {
                self.i = self.len;
                Err(self.err("unterminated block comment", start))
            }
        }
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
        let end = self.word_run_end(self.i);
        buf.extend(self.cs[self.i..end].iter());
        self.i = end;
    }

    /// End (exclusive) of the identifier run at `from`: alphanumerics and `_`
    /// always, `-` only when another identifier character follows it.
    fn word_run_end(&self, from: usize) -> usize {
        from + (from..self.len)
            .take_while(|&k| {
                let c = self.cs[k];
                c.is_alphanumeric()
                    || c == '_'
                    || (c == '-'
                        && self
                            .cs
                            .get(k + 1)
                            .is_some_and(|n| n.is_alphanumeric() || *n == '_' || *n == '-'))
            })
            .count()
    }

    /// Lex an identifier-ish run, merging adjacent `#{...}` into Interpolated.
    pub(crate) fn lex_word(&mut self, start: usize) -> LxResult {
        // adjacent `#{…}` runs merge into an Interpolated token: consume the
        // leading interpolation sequence, then exactly one identifier run
        let interps = std::iter::from_fn(|| self.at_interp().then(|| self.lex_interp_raw()))
            .collect::<Result<Vec<_>, _>>()?;
        let pieces: Vec<Piece> = interps.into_iter().map(Piece::Expr).collect();
        let dot = pieces.is_empty() && self.cs.get(self.i) == Some(&'.');
        let before = self.i;
        let text = self.read_word_token(dot);
        let invalid = self.i == before && text.is_empty() && pieces.is_empty();
        match (invalid, pieces.is_empty()) {
            (true, _) => Err(self.err("expected identifier", start)),
            (false, true) => {
                let is_url = text.eq_ignore_ascii_case("url")
                    && self.cs.get(self.i) == Some(&'(')
                    && !matches!(self.peek(1), Some('"') | Some('\'') | Some('$') | Some('#'));
                match is_url {
                    true => self.lex_url(start),
                    false => self.spanned(Token::Ident(text), start),
                }
            }
            (false, false) => {
                // the trailing text run still belongs to the interpolated token
                let pieces: Vec<Piece> = pieces
                    .into_iter()
                    .chain((!text.is_empty()).then_some(Piece::Text(text)))
                    .collect();
                self.spanned(
                    Token::Interpolated {
                        quote: None,
                        pieces,
                    },
                    start,
                )
            }
        }
    }

    /// The cursor sits on the `#` of an interpolation opener.
    fn at_interp(&self) -> bool {
        self.cs.get(self.i) == Some(&'#') && self.cs.get(self.i + 1) == Some(&'{')
    }

    /// Read the identifier run of a word token, with a leading `.` when `dot`.
    fn read_word_token(&mut self, dot: bool) -> String {
        self.i += usize::from(dot);
        let end = self.word_run_end(self.i);
        let text: String = dot
            .then_some('.')
            .into_iter()
            .chain(self.cs[self.i..end].iter().copied())
            .collect();
        self.i = end;
        text
    }

    fn lex_url(&mut self, start: usize) -> LxResult {
        self.i += 1;
        // fold the nesting depth until the matching `)`; the state freezes at
        // depth zero so the cursor lands right after that closer
        let (end, depth) = (self.i..self.len).fold(
            (self.i, 1usize),
            |(k, depth), idx| match (depth, idx == k) {
                (0, _) => (k, 0),
                (_, false) => (k, depth),
                (_, true) => match self.cs[idx] {
                    '(' => (idx + 1, depth + 1),
                    ')' => (idx + 1, depth - 1),
                    _ => (idx + 1, depth),
                },
            },
        );
        self.i = end;
        match depth {
            0 => {
                let raw: String = self.cs[start..self.i].iter().collect();
                self.spanned(Token::Url(raw), start)
            }
            _ => Err(self.err("unterminated url()", start)),
        }
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
        let end = start
            + 1
            + (start + 1..self.len)
                .take_while(|&k| self.cs[k].is_ascii_hexdigit())
                .count();
        self.i = end;
        let digits: String = self.cs[start + 1..end].iter().collect();
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
        let negative = self.cs[self.i] == '-';
        let body = self.i + usize::from(negative);
        let int_end = self.digit_run(body);
        // fractional part only when a digit follows the dot (else it is a dot token)
        let has_fraction = self.cs.get(int_end) == Some(&'.')
            && self.cs.get(int_end + 1).is_some_and(|c| c.is_ascii_digit());
        let frac_end = if has_fraction {
            self.digit_run(int_end + 1)
        } else {
            int_end
        };
        let end = self.exponent_end(frac_end).unwrap_or(frac_end);
        let text: String = self.cs[start..end].iter().collect();
        let value: f64 = text
            .parse()
            .map_err(|_| self.err(format!("invalid number '{text}'"), start))?;
        let (unit, next) = match self.cs.get(end).copied() {
            Some('%') => (Some("%".to_string()), end + 1),
            Some(c) if c.is_alphabetic() => {
                let unit_end = self.word_run_end(end);
                (Some(self.cs[end..unit_end].iter().collect()), unit_end)
            }
            _ => (None, end),
        };
        self.i = next;
        self.spanned(Token::Number { value, unit }, start)
    }

    /// End (exclusive) of the ASCII-digit run starting at `from`.
    fn digit_run(&self, from: usize) -> usize {
        from + (from..self.len)
            .take_while(|&k| self.cs[k].is_ascii_digit())
            .count()
    }

    /// End of a scientific-notation suffix at `from`, only when digits follow.
    fn exponent_end(&self, from: usize) -> Option<usize> {
        let digits_at = match (
            self.cs.get(from).copied(),
            self.cs.get(from + 1).copied(),
        ) {
            (Some('e' | 'E'), Some(c)) if c.is_ascii_digit() => Some(from + 1),
            (Some('e' | 'E'), Some('-' | '+'))
                if self.cs.get(from + 2).copied().is_some_and(|c| c.is_ascii_digit()) =>
            {
                Some(from + 2)
            }
            _ => None,
        }?;
        Some(self.digit_run(digits_at))
    }
}
