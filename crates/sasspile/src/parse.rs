//! Parsing as a fold over the token stream: `reduce_initial` moves an owned `Acc`
//! from step to step. Each open block is an owned `Frame` on a stack; a statement's
//! tokens are buffered (`run`) until a delimiter `{` / `;` / `}` decides its role:
//! `{` → the run is a nested selector; `;` / `}` → the run is a declaration split at
//! its first `:`. This keeps a pseudo-class `:` (`&:hover {`) distinct from a
//! declaration `:` without lookahead. No parser object, no `&mut self` — nesting is
//! just push/pop on the moved accumulator's stack.

use crate::ast::{Program, Rule};
use crate::error::{ErrorKind, SassError};
use crate::lexer::Tok;

/// One open block: its resolved selector plus declarations / child rules so far.
#[derive(Debug, Default)]
struct Frame {
    selector: String,
    decls: Vec<(String, String)>,
    children: Vec<Rule>,
}

#[derive(Debug, Default)]
pub(crate) struct Acc {
    program: Program,
    stack: Vec<Frame>,
    run: Vec<Tok>,
}

/// Reconstruct the source text of a buffered statement.
fn stmt_text(run: &[Tok]) -> String {
    run.iter().fold(String::new(), |mut s, t| {
        match t {
            Tok::Word(w) => {
                if !s.is_empty() && !s.ends_with([' ', ':', ',']) {
                    s.push(' ');
                }
                s.push_str(w);
            }
            Tok::Punct(':') => s.push(':'),
            Tok::Punct(',') => s.push_str(", "),
            Tok::Punct(c) => s.push(*c),
        }
        s
    })
}

/// Combine a nested selector with its parent (`&` → parent, else descendant join).
fn combine(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else if child.contains('&') {
        child.replace('&', parent)
    } else {
        format!("{parent} {child}")
    }
}

impl Acc {
    pub(crate) fn step(mut self, tok: Tok) -> Self {
        match tok {
            Tok::Punct('{') => {
                let selector = stmt_text(&std::mem::take(&mut self.run));
                let parent = self.stack.last().map(|f| f.selector.as_str()).unwrap_or("");
                let resolved = combine(parent, selector.trim());
                self.stack.push(Frame {
                    selector: resolved,
                    decls: Vec::new(),
                    children: Vec::new(),
                });
            }
            Tok::Punct(';') => {
                self.commit_stmt();
            }
            Tok::Punct('}') => {
                self.commit_stmt();
                if let Some(frame) = self.stack.pop() {
                    let rule = Rule {
                        selector: frame.selector,
                        decls: frame.decls,
                        children: frame.children,
                    };
                    match self.stack.last_mut() {
                        Some(parent) => parent.children.push(rule),
                        None => self.program.rules.push(rule),
                    }
                }
            }
            other => self.run.push(other),
        }
        self
    }

    /// Commit the buffered run as a declaration (`prop: value`) into the innermost
    /// open block; a run with no `:` or an empty run contributes nothing.
    fn commit_stmt(&mut self) {
        let text = stmt_text(&std::mem::take(&mut self.run));
        if let Some((prop, value)) = text.split_once(':') {
            let (prop, value) = (prop.trim().to_string(), value.trim().to_string());
            if !prop.is_empty() && let Some(frame) = self.stack.last_mut() {
                frame.decls.push((prop, value));
            }
        }
    }

    pub(crate) fn finish(self) -> Result<Program, SassError> {
        (!self.stack.is_empty())
            .then(|| {
                SassError::new(
                    ErrorKind::Parse,
                    format!("unexpected end of input: {} unclosed block(s)", self.stack.len()),
                )
            })
            .or_else(|| {
                let text = stmt_text(&self.run);
                (!text.trim().is_empty()).then(|| {
                    SassError::new(
                        ErrorKind::Parse,
                        format!("unexpected end of input near `{}`", text.trim()),
                    )
                })
            })
            .map_or(Ok(self.program), Err)
    }
}
