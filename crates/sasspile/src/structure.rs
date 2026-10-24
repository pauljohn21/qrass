//! Structure: fold the structured token stream into an owned tree of `Block`s. This is
//! *data* (head tokens + declarations + nested `body`), produced by a recursive-descent
//! parser (recursion, not `for`/`while`/`loop`, and not a mutable `Acc` that *drives* a
//! traversal later). Each block carries an `order` key = its creation sequence, so the
//! `Shared`/multi-threaded fan-out downstream (tasks 2.3/2.4) can merge back into exact
//! document order. Values/selectors stay as token runs; the `Expr` fold happens at eval.

use crate::lexer::Tok;

/// One declaration: `(prop tokens, value tokens)`.
pub type Decl = (Vec<Tok>, Vec<Tok>);

/// An owned block: its head tokens (selector / at-rule; empty for the root), its own
/// declarations, and its nested child blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub head: Vec<Tok>,
    pub decls: Vec<Decl>,
    pub body: Vec<Block>,
    pub order: u32,
}

impl Block {
    /// Parse a token stream into the implicit root block (`head` empty). The root is not
    /// emitted as CSS; it only groups top-level declarations and blocks.
    pub fn parse(tokens: &[Tok]) -> Block {
        let mut p = P { t: tokens, i: 0, ord: 0 };
        let mut decls = Vec::new();
        let mut body = Vec::new();
        p.siblings(&mut decls, &mut body, false);
        Block { head: Vec::new(), decls, body, order: 0 }
    }
}

struct P<'a> {
    t: &'a [Tok],
    i: usize,
    ord: u32,
}

fn is_punct(tok: &Tok, c: char) -> bool {
    matches!(tok, Tok::Punct(x) if *x == c)
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn at(&self, c: char) -> bool {
        self.peek().is_some_and(|t| is_punct(t, c))
    }
    fn at_boundary(&self) -> bool {
        match self.peek() {
            None => true,
            Some(t) => is_punct(t, '{') || is_punct(t, '}') || is_punct(t, ';'),
        }
    }
    fn next_id(&mut self) -> u32 {
        let n = self.ord;
        self.ord += 1;
        n
    }

    /// Consume tokens into a run up to (not including) the next `{ } ;` boundary.
    fn take_run(&mut self) -> Vec<Tok> {
        if self.at_boundary() {
            Vec::new()
        } else {
            let tok = self.t[self.i].clone();
            self.i += 1;
            let mut out = vec![tok];
            out.append(&mut self.take_run());
            out
        }
    }

    /// Collect sibling declarations and blocks into `decls`/`body`; stop at end, or at a
    /// `}` when `stop_close`. Recursion (not a loop) advances across siblings.
    fn siblings(&mut self, decls: &mut Vec<Decl>, body: &mut Vec<Block>, stop_close: bool) {
        if self.peek().is_none() || (stop_close && self.at('}')) {
            return;
        }
        let run = self.take_run();
        if self.at('{') {
            self.i += 1; // consume '{'
            let (cdecls, cbody) = self.block_body();
            let child = Block { head: run, decls: cdecls, body: cbody, order: self.next_id() };
            body.push(child);
        } else {
            if let Some(decl) = split_decl(&run) {
                decls.push(decl);
            }
            if self.at(';') {
                self.i += 1; // consume ';'
            }
        }
        self.siblings(decls, body, stop_close);
    }

    /// Parse a block's inner declarations + children up to its closing `}` (consumed).
    fn block_body(&mut self) -> (Vec<Decl>, Vec<Block>) {
        let mut decls = Vec::new();
        let mut body = Vec::new();
        self.siblings(&mut decls, &mut body, true);
        if self.at('}') {
            self.i += 1; // consume '}'
        }
        (decls, body)
    }
}

/// Split a declaration run at the first `:` into `(prop, value)` tokens; `None` if the
/// property part is empty (e.g. a bare nested-selector run with no declaration).
fn split_decl(run: &[Tok]) -> Option<Decl> {
    let colon = run.iter().position(|t| is_punct(t, ':'))?;
    let prop = run[..colon].to_vec();
    let value = run[colon + 1..].to_vec();
    let prop_ok = match prop.first() {
        Some(Tok::Word(w)) => !w.trim().is_empty(),
        Some(Tok::Var(_) | Tok::Interp(_)) => true,
        _ => false,
    };
    prop_ok.then_some((prop, value))
}
