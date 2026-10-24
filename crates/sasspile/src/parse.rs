//! Parsing as a fold: the token stream is reduced with `reduce_initial`, the
//! partial `Acc` (an owned value) being moved from step to step. There is no
//! parser object and no `&mut self` — `step` consumes `self` and returns the next
//! owned `Acc`, which is exactly the accumulator the stream carries.

use crate::ast::{Program, Rule};
use crate::error::{ErrorKind, SassError};
use crate::lexer::Tok;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Selector,
    Prop,
    Value,
}

#[derive(Debug, Default)]
pub(crate) struct Acc {
    program: Program,
    sel: String,
    decls: Vec<(String, String)>,
    prop: String,
    val: String,
    phase: Phase,
}

impl Acc {
    pub(crate) fn step(mut self, tok: Tok) -> Self {
        match tok {
            Tok::Word(w) => match self.phase {
                Phase::Selector => {
                    if !self.sel.is_empty() {
                        self.sel.push(' ');
                    }
                    self.sel.push_str(&w);
                }
                Phase::Prop => self.prop = w,
                Phase::Value => {
                    if !self.val.is_empty() {
                        self.val.push(' ');
                    }
                    self.val.push_str(&w);
                }
            },
            Tok::Punct('{') => self.phase = Phase::Prop,
            Tok::Punct(':') => self.phase = Phase::Value,
            Tok::Punct(';') => {
                if self.phase == Phase::Value {
                    self.commit_decl();
                    self.phase = Phase::Prop;
                }
            }
            Tok::Punct('}') => {
                if self.phase == Phase::Value {
                    self.commit_decl();
                }
                let selector = std::mem::take(&mut self.sel).trim().to_string();
                self.program.rules.push(Rule {
                    selector,
                    decls: std::mem::take(&mut self.decls),
                });
                self.phase = Phase::Selector;
            }
            Tok::Punct(',') => {
                if self.phase == Phase::Selector {
                    self.sel.push_str(", ");
                }
            }
            Tok::Punct(_) => {}
        }
        self
    }

    fn commit_decl(&mut self) {
        self.decls.push((
            std::mem::take(&mut self.prop),
            std::mem::take(&mut self.val),
        ));
    }

    pub(crate) fn finish(self) -> Result<Program, SassError> {
        if self.phase != Phase::Selector || !self.sel.is_empty() {
            return Err(SassError::new(
                ErrorKind::Parse,
                format!("unexpected end of input in rule `{}`", self.sel.trim()),
            ));
        }
        Ok(self.program)
    }
}
