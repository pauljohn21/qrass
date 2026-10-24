//! Structural fold over the token stream → owned leaves. Two pure rxrust `scan_map`
//! stages: (1) tokens → `Vec<Msg>` (open/decl/close, delimiters only, no tree);
//! (2) `Msg` → `Vec<Leaf>` via an owned `Acc` (a frame stack + `env`); nesting is
//! flattened as each block closes (a frame hands its finished leaves to its parent, or
//! to the output at the root), so no `Vec<Rule>` tree and no recursion. Selectors/values
//! are resolved through `Expr` at the moment a frame closes (lazily, with the then-current
//! env).

use crate::expr::{Env, Expr};
use crate::lexer::Tok;

/// A structural message produced by the delimiter scan.
#[derive(Debug, Clone)]
pub enum Msg {
    Open(Vec<Tok>),
    Decl { prop: Vec<Tok>, value: Vec<Tok> },
    Close,
}

/// A resolved, flat output rule (document order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    pub selector: String,
    pub decls: Vec<(String, String)>,
}

// ---- stage 1: tokens -> Msg ----

#[derive(Default)]
pub(crate) struct MsgAcc {
    run: Vec<Tok>,
    msgs: Vec<Msg>,
}

impl MsgAcc {
    pub(crate) fn feed(&mut self, tok: Tok) -> Vec<Msg> {
        self.msgs.clear();
        match tok {
            Tok::Punct('{') => self.msgs.push(Msg::Open(std::mem::take(&mut self.run))),
            Tok::Punct(';') => self.push_decl(),
            Tok::Punct('}') => {
                self.push_decl();
                self.msgs.push(Msg::Close);
            }
            other => self.run.push(other),
        }
        self.msgs.clone()
    }

    fn push_decl(&mut self) {
        let run = std::mem::take(&mut self.run);
        if let Some(colon) = run.iter().position(|t| matches!(t, Tok::Punct(':'))) {
            let prop = run[..colon].to_vec();
            let value = run[colon + 1..].to_vec();
            let non_empty = Expr::render(&prop, &Env::new())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if non_empty {
                self.msgs.push(Msg::Decl { prop, value });
            }
        }
    }
}

// ---- stage 2: Msg -> Vec<Leaf> ----

struct Frame {
    selector: String,
    directive: bool,
    decls: Vec<(String, String)>,
    /// completed descendant leaves, flattened up when this frame closes
    pending: Vec<Leaf>,
}

#[derive(Default)]
pub struct Acc {
    env: Env,
    stack: Vec<Frame>,
}

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
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn parent_selector(&self) -> String {
        self.stack.last().map(|f| f.selector.clone()).unwrap_or_default()
    }

    pub fn feed(&mut self, msg: Msg) -> Vec<Leaf> {
        match msg {
            Msg::Open(head) => {
                let head_text = Expr::render(&head, &self.env).unwrap_or_default();
                let head_text = head_text.trim().to_string();
                let directive = head_text.starts_with('@');
                let selector = if directive {
                    self.parent_selector()
                } else {
                    combine(&self.parent_selector(), &head_text)
                };
                self.stack.push(Frame {
                    selector,
                    directive,
                    decls: Vec::new(),
                    pending: Vec::new(),
                });
                Vec::new()
            }
            Msg::Decl { prop, value } => self.decl(prop, value),
            Msg::Close => self.close(),
        }
    }

    fn decl(&mut self, prop: Vec<Tok>, value: Vec<Tok>) -> Vec<Leaf> {
        let Some(v) = Expr::render(&value, &self.env) else {
            return Vec::new();
        };
        let p = Expr::render(&prop, &self.env).unwrap_or_default();
        let p = p.trim().to_string();
        if let Some(name) = p.strip_prefix('$') {
            self.env
                .insert(name.to_string(), crate::value::Value::Raw(v));
        } else if let Some(frame) = self.stack.last_mut() {
            frame.decls.push((p, v));
        }
        Vec::new()
    }

    fn close(&mut self) -> Vec<Leaf> {
        let Some(f) = self.stack.pop() else {
            return Vec::new();
        };
        let mut produced = Vec::new();
        if !f.directive && !f.decls.is_empty() {
            produced.push(Leaf {
                selector: f.selector,
                decls: f.decls,
            });
        }
        produced.extend(f.pending);
        match self.stack.last_mut() {
            Some(parent) => {
                parent.pending.extend(produced);
                Vec::new()
            }
            None => produced,
        }
    }
}
