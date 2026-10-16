//! SCSS parser expressed **natively in the rxrust + ownership medium**.
//!
//! There is no token array and no cursor. The stream hands the accumulator one
//! owned [`Spanned`] at a time; the accumulator is only ever "the not-yet-closed
//! owned half-products": a stack of open blocks (each owning the statements
//! collected inside it) plus the current statement's owned token run (`pending`).
//! Each arriving token triggers one reaction; the moment a boundary (`;`/`{`/`}`)
//! is seen at depth 0 an owned node is materialized and **moved up** into its
//! parent scope. Top-level statements leave the pipeline through `flat_map`.
//!
//! No `self.i`, no `peek(k)` lookahead, no end-of-stream batch parse. Expression
//! and directive construction live in the sibling folds [`parser_expr`] and
//! [`parser_at`] — both consume an owned `Vec<Spanned>` run and return owned AST,
//! never touching a shared cursor.

use std::convert::Infallible;

use rxrust::prelude::*;

use crate::ast::{Block, IfClause, InterpText, Stmt};
use crate::ast_expr::{Arg, Expr};
use crate::error::{ErrorKind, Located, SassError};
use crate::span::{BytePos, FileId, Span};
use crate::token::{Punct, Spanned, Token};

pub(crate) type PsResult = Result<Stmt, SassError>;

#[path = "parser_at.rs"]
mod parser_at;
#[path = "parser_expr.rs"]
mod parser_expr;
#[path = "parser_args.rs"]
mod parser_args;
#[path = "parser_if.rs"]
mod parser_if;
#[path = "parser_if_raw.rs"]
mod parser_if_raw;
#[path = "parser_module.rs"]
mod parser_module;
#[path = "parser_text.rs"]
mod parser_text;
pub(crate) use parser_text::token_to_pieces;
pub(crate) use parser_module::config_key;
#[path = "named_colors.rs"]
pub(crate) mod named_colors;

// ─── Pushdown state (owned values, moved — not a mutable cursor object) ────────

/// What owns an open block; reconstructed once, from the pending head run, when
/// the block's `{` arrives.
pub(crate) enum Owner {
    Ruleset {
        selectors: InterpText,
        span: Span,
    },
    Media {
        queries: InterpText,
        span: Span,
    },
    Supports {
        condition: InterpText,
        span: Span,
    },
    Keyframes {
        name: InterpText,
        span: Span,
    },
    AtRoot {
        header: Option<(String, Vec<String>)>,
        /// `<selector>` in the `@at-root <selector> { … }` header form (empty in
        /// the block form `@at-root { … }`).
        selector: InterpText,
        span: Span,
    },
    Mixin {
        name: InterpText,
        params: Vec<crate::ast::Param>,
        span: Span,
    },
    Function {
        name: String,
        params: Vec<crate::ast::Param>,
        span: Span,
    },
    Include {
        name: InterpText,
        args: Vec<Arg>,
        content_params: Vec<String>,
        span: Span,
    },
    /// A `@if` or `@else [if cond]` body frame; `cond` is `None` for a bare `@else`.
    If {
        cond: Option<Expr>,
        span: Span,
    },
    Each {
        vars: Vec<String>,
        list: Expr,
        span: Span,
    },
    For {
        var: String,
        from: Expr,
        to: Expr,
        inclusive: bool,
        span: Span,
    },
    While {
        cond: Expr,
        span: Span,
    },
    AtRule {
        name: String,
        value: InterpText,
        span: Span,
    },
}

/// One open scope: what owns it, the statements finished inside it, and — for the
/// frame where an `@if` chain appeared — the clauses collected so far plus whether
/// an `@else` may still follow.
pub(crate) struct Frame {
    owner: Option<Owner>,
    stmts: Block,
    if_clauses: Vec<IfClause>,
    /// Span of the originating `@if` (kept so the collapsed `@else` chain has one).
    if_span: Option<Span>,
}

/// The `scan_map` accumulator: everything is owned partial structure. It never
/// holds the token stream or an index into it.
pub struct Parser {
    file: FileId,
    name: String,
    cs: Vec<char>,
    /// Full source text; kept so raw-capture regions (modern `if()`) can slice
    /// byte spans verbatim instead of rebuilding from tokens.
    content: String,
    stack: Vec<Frame>,
    pending: Vec<Spanned>,
    /// Nesting depth of `(`/`[` inside `pending` so `;`/`{`/`}` there stay literal.
    depth: i32,
    failed: bool,
}

impl Parser {
    pub fn new(file: FileId, name: String, content: String) -> Self {
        let cs = content.chars().collect();
        Self {
            file,
            name,
            cs,
            content,
            stack: vec![Frame {
                owner: None,
                stmts: Vec::new(),
                if_clauses: Vec::new(),
                if_span: None,
            }],
            pending: Vec::new(),
            depth: 0,
            failed: false,
        }
    }

    // ─── The single reactive transition (one token in, 0..1 statements out) ───

    pub fn step(&mut self, tok: Option<Spanned>) -> Vec<PsResult> {
        if self.failed {
            return Vec::new();
        }
        let out = match tok {
            Some(s) => self.react(s),
            None => self.finish(),
        };
        if out.iter().any(|r| r.is_err()) {
            self.failed = true;
        }
        out
    }

    fn react(&mut self, s: Spanned) -> Vec<PsResult> {
        use Punct::*;
        // A structural brace/semicolon only counts at depth 0; inside `(...)` the
        // tokens are part of the current pending value and are buffered verbatim.
        if self.depth == 0 {
            let boundary = match s.token {
                Token::Punct(LBrace) => Some(self.open_block()),
                Token::Punct(RBrace) => Some(self.close_block()),
                Token::Punct(Semicolon) => Some(self.finish_simple(false)),
                _ => None,
            };
            if let Some(r) = boundary {
                return self.wrap(r);
            }
        }
        // A standalone loud comment (`/* … */`) at a statement boundary becomes
        // its own `Stmt::Comment`; a comment trailing other tokens stays buffered
        // and is dropped by value folding (`token_to_pieces`).
        if self.pending.is_empty()
            && let Token::Comment(text) = s.token.clone()
        {
            self.stack
                .last_mut()
                .unwrap()
                .stmts
                .push(Stmt::Comment(crate::ast::Comment { text, span: s.span }));
            return Vec::new();
        }
        match s.token {
            Token::Punct(LParen | LBracket) => self.depth += 1,
            Token::Punct(RParen | RBracket) => self.depth = (self.depth - 1).max(0),
            _ => {}
        }
        // The first token of a new statement ends any dangling `@if…@else` chain at
        // this level unless it is itself an `@else` (detected reactively, no peek).
        if self.pending.is_empty() && !matches!(s.token, Token::AtKeyword(ref n) if n == "else") {
            self.flush_if_chain();
        }
        self.pending.push(s);
        Vec::new()
    }

    // ─── Boundary reactions ────────────────────────────────────────────────

    /// `{` reached: turn the pending head into an [`Owner`] and open a child scope.
    fn open_block(&mut self) -> Result<(), SassError> {
        let head = std::mem::take(&mut self.pending);
        let owner = parser_at::build_owner(self, head)?;
        self.stack.push(Frame {
            owner: Some(owner),
            stmts: Vec::new(),
            if_clauses: Vec::new(),
            if_span: None,
        });
        Ok(())
    }

    /// `}` reached: finish any semicolon-less last statement, seal this scope, and
    /// move its owned statements up to the parent (as a clause or a node).
    fn close_block(&mut self) -> Result<(), SassError> {
        if !self.pending.is_empty() {
            self.finish_simple(true)?; // declaration without trailing `;` is legal
        }
        if self.stack.len() == 1 {
            let at = self.cur_pending_start();
            return Err(self.err("unexpected '}'", at));
        }
        let mut frame = self.stack.pop().unwrap();
        let file = self.file;
        frame.flush_if(file); // a nested chain ends when its enclosing block closes
        let parent = self.stack.last_mut().unwrap();
        match frame.owner {
            Some(Owner::If { cond, span }) => {
                parent.if_span.get_or_insert(span);
                parent.if_clauses.push(IfClause {
                    cond,
                    body: frame.stmts,
                });
            }
            Some(owner) => {
                let stmt = parser_at::seal_block(owner, frame.stmts);
                parent.stmts.push(stmt);
            }
            None => unreachable!("root frame is never popped as a child"),
        }
        Ok(())
    }

    /// `;` reached (or `}` with a pending semicolon-less statement): materialize a
    /// simple statement and move it into the current scope. A stray `;` (empty run)
    /// folds to `None` and is a no-op — expressed as data flow, not an early `return`.
    fn finish_simple(&mut self, no_semi: bool) -> Result<(), SassError> {
        Some(std::mem::take(&mut self.pending))
            .filter(|run| !run.is_empty())
            .map(|run| parser_at::build_simple(self, run, no_semi))
            .transpose()?
            .into_iter()
            .for_each(|stmt| self.stack.last_mut().unwrap().stmts.push(stmt));
        Ok(())
    }

    /// Stream end: an unbalanced `(`/`[` in `pending` is unterminated; otherwise a
    /// final statement may omit its trailing `;` (the goldens treat EOF like a `}`
    /// boundary), so drain the pending run semicolon-less before sealing the root.
    fn finish(&mut self) -> Vec<PsResult> {
        if self.depth != 0 {
            self.failed = true;
            let at = self.cur_pending_start();
            return vec![Err(self.err("unexpected end of input", at))];
        }
        if !self.pending.is_empty() && let Err(e) = self.finish_simple(true) {
            self.failed = true;
            return vec![Err(e)];
        }
        if self.stack.len() > 1 {
            self.failed = true;
            return vec![Err(self.err("unterminated block, expected '}'", BytePos(0)))];
        }
        let mut root = self.stack.pop().unwrap();
        root.flush_if(self.file);
        root.stmts.into_iter().map(Ok).collect()
    }

    /// Fold a `Result` boundary transition into the emitted batch.
    fn wrap(&mut self, r: Result<(), SassError>) -> Vec<PsResult> {
        match r {
            Ok(()) => Vec::new(),
            Err(e) => vec![Err(e)],
        }
    }

    fn flush_if_chain(&mut self) {
        let file = self.file;
        if let Some(top) = self.stack.last_mut() {
            top.flush_if(file);
        }
    }

    fn cur_pending_start(&self) -> BytePos {
        self.pending
            .first()
            .map(|s| s.span.start)
            .unwrap_or(BytePos(0))
    }
}

impl Frame {
    /// Turn any collected `@if…@else` clauses into a single [`Stmt::If`] appended to
    /// this scope's statements.
    fn flush_if(&mut self, file: FileId) {
        if self.if_clauses.is_empty() {
            return;
        }
        let clauses = std::mem::take(&mut self.if_clauses);
        let span = self.if_span.take().unwrap_or(Span {
            file,
            start: BytePos(0),
            end: BytePos(0),
        });
        self.stmts.push(Stmt::If(crate::ast::If { clauses, span }));
    }
}

// ─── Shared helpers for the sub-folds ──────────────────────────────────────────

impl Parser {
    /// Verbatim source text of a byte-span region (token boundaries are char
    /// boundaries, so slicing is always safe).
    pub(crate) fn slice(&self, from: BytePos, to: BytePos) -> String {
        let end = to.0.min(self.content.len());
        if from.0 >= end {
            return String::new();
        }
        self.content[from.0..end].to_string()
    }

    pub(crate) fn err(&self, msg: impl Into<String>, at: BytePos) -> SassError {
        let (line, col) = self.line_col(at.0);
        let excerpt = self
            .cs
            .iter()
            .collect::<String>()
            .lines()
            .nth(line.saturating_sub(1) as usize)
            .map(str::to_owned);
        SassError::new(ErrorKind::Parse, msg).with_located(Located {
            file: self.name.clone(),
            line,
            col,
            excerpt,
        })
    }

    pub(crate) fn line_col(&self, at: usize) -> (u32, u32) {
        let (mut line, mut col) = (1u32, 1u32);
        for &ch in &self.cs[..at.min(self.cs.len())] {
            if ch == '\n' {
                line += 1;
                col = 1;
            } else if ch != '\r' {
                col += 1;
            }
        }
        (line, col)
    }

    /// Render a token run into an interpolated text run (selectors / property
    /// names / at-rule parameters). Pure fold over owned tokens.
    ///
    /// The lexer drops inter-token whitespace, but a selector's descendant
    /// combinator *is* a space (`& .foo` ≠ `&.foo`), so when two tokens are
    /// separated by whitespace in the source a single space piece is re-inserted
    /// to preserve the original combinator structure.
    pub(crate) fn pieces(&self, tokens: &[Spanned]) -> InterpText {
        use crate::token::Piece;
        let mut out = InterpText::new();
        let mut prev_end: Option<BytePos> = None;
        for s in tokens {
            if let Some(end) = prev_end
                && s.span.start.0 > end.0
            {
                // A gap in the source is whitespace (the lexer emits no other
                // gap), but a comment token already carries its own spacing.
                if !matches!(s.token, Token::Comment(_)) {
                    out.push(Piece::Text(" ".into()));
                }
            }
            token_to_pieces(&s.token, &mut out);
            prev_end = Some(s.span.end);
        }
        out
    }

    /// Span covering an owned token run.
    pub(crate) fn span_of(&self, tokens: &[Spanned]) -> Span {
        let start = tokens.first().map(|s| s.span.start).unwrap_or(BytePos(0));
        let end = tokens.last().map(|s| s.span.end).unwrap_or(start);
        Span {
            file: self.file,
            start,
            end,
        }
    }

    /// Index of the first depth-0 `:` in a run, if any (functional; bounded to the
    /// statement's own tokens, not a stream-wide scan).
    pub(crate) fn first_colon(run: &[Spanned]) -> Option<usize> {
        let mut depth = 0i32;
        run.iter().position(|s| match &s.token {
            Token::Punct(Punct::LParen | Punct::LBracket) => {
                depth += 1;
                false
            }
            Token::Punct(Punct::RParen | Punct::RBracket) => {
                depth -= 1;
                false
            }
            Token::Punct(Punct::Colon) if depth == 0 => true,
            _ => false,
        })
    }
}

// ─── Public reactive API ─────────────────────────────────────────────────────

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

/// Synchronous convenience API for tests: parse a full token vector into a program.
pub fn parse(
    file: FileId,
    name: String,
    content: String,
    tokens: Vec<Spanned>,
) -> Result<Vec<Stmt>, SassError> {
    let mut p = Parser::new(file, name, content);
    let mut stmts = Vec::new();
    for t in tokens.into_iter().map(Some).chain(std::iter::once(None)) {
        for r in p.step(t) {
            stmts.push(r?);
        }
    }
    Ok(stmts)
}
