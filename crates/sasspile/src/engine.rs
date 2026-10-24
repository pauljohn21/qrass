//! The flat-stream compiler engine (isolated; not yet wired into `compile_string`).
//!
//! Architecture (satisfies "one owned stream, no tree, no recursion"):
//! - tokens → a flat `Vec<Msg>` (`Open`/`Decl`/`Close`) — selector combination is NOT
//!   done here, only structure.
//! - `Engine::run` folds that stream through an OWNED work-queue (`VecDeque<Job>`)
//!   driven by `std::iter::from_fn` — a block's body is replayed for `@each`/`@for` by
//!   re-injecting jobs onto the queue (no recursion, no loop keywords), nesting depth
//!   lives in an owned `Vec<Frame>` stack, and completed rules are flattened into an
//!   ordered `Vec<Leaf>` as frames close.
//!
//! Value resolution mirrors the current layer: `#{}` via `value_to_css`, guarded numeric
//! `+`/`-`/functions via `parse_expr`, else `$var` substitution.

use std::collections::VecDeque;

use crate::expr::{Env as ValueEnv, Expr, parse_expr, value_to_css};
use crate::value::Value;

#[derive(Debug, Clone)]
pub enum Msg {
    Open(String),
    Decl { prop: String, value: String },
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    pub selector: String,
    pub decls: Vec<(String, String)>,
}

/// Token stream → flat `Vec<Msg>` (structure only; selector combination deferred).
pub(crate) fn to_msgs<I: Iterator<Item = crate::lexer::Tok>>(toks: I) -> Vec<Msg> {
    let mut msgs = Vec::new();
    let mut run: Vec<crate::lexer::Tok> = Vec::new();
    toks.for_each(|t| match t {
        crate::lexer::Tok::Punct('{') => msgs.push(Msg::Open(stmt_text(&take(&mut run)))),
        crate::lexer::Tok::Punct(';') => push_decl(&mut msgs, &mut run),
        crate::lexer::Tok::Punct('}') => {
            push_decl(&mut msgs, &mut run);
            msgs.push(Msg::Close);
        }
        other => run.push(other),
    });
    msgs
}

fn take(v: &mut Vec<crate::lexer::Tok>) -> Vec<crate::lexer::Tok> {
    std::mem::take(v)
}

fn stmt_text(run: &[crate::lexer::Tok]) -> String {
    run.iter().fold(String::new(), |mut s, t| {
        match t {
            crate::lexer::Tok::Word(w) => {
                if !s.is_empty() && !s.ends_with([' ', ':', ',']) {
                    s.push(' ');
                }
                s.push_str(w.trim());
            }
            crate::lexer::Tok::Punct(':') => s.push(':'),
            crate::lexer::Tok::Punct(',') => s.push_str(", "),
            crate::lexer::Tok::Punct(c) => s.push(*c),
        }
        s
    })
}

fn push_decl(msgs: &mut Vec<Msg>, run: &mut Vec<crate::lexer::Tok>) {
    let text = stmt_text(run);
    take(run);
    if let Some((prop, value)) = text.split_once(':') {
        let (p, v) = (prop.trim().to_string(), value.trim().to_string());
        if !p.is_empty() {
            msgs.push(Msg::Decl {
                prop: p,
                value: v,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Engine

enum Frame {
    Rule {
        selector: String,
        directive: bool,
        decls: Vec<(String, String)>,
        children: Vec<Leaf>,
    },
    If {
        active: bool,
        children: Vec<Leaf>,
    },
    Loop {
        key: String,
        items: Vec<String>,
        depth: usize,
        body: Vec<Msg>,
    },
}

enum Job {
    Msg(Msg),
    Bind(String, String),
}

pub struct Engine {
    env: std::collections::HashMap<String, String>,
    work: VecDeque<Job>,
    stack: Vec<Frame>,
    group_taken: bool,
    out: Vec<Leaf>,
}

impl Engine {
    pub fn run(msgs: Vec<Msg>) -> Vec<Leaf> {
        let mut e = Engine {
            env: std::collections::HashMap::new(),
            work: msgs.into_iter().map(Job::Msg).collect(),
            stack: Vec::new(),
            group_taken: false,
            out: Vec::new(),
        };
        // drive the work queue until empty — via from_fn (no loop keyword)
        std::iter::from_fn(|| e.step()).for_each(drop);
        e.finish_root()
    }

    fn finish_root(&mut self) -> Vec<Leaf> {
        // any frames still open at end are ignored (parse validated balance)
        std::mem::take(&mut self.out)
    }

    fn step(&mut self) -> Option<()> {
        match self.work.pop_front() {
            None => None,
            Some(Job::Bind(k, v)) => {
                self.env.insert(k, v);
                Some(())
            }
            Some(Job::Msg(m)) => {
                self.on_msg(m);
                Some(())
            }
        }
    }

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Open(head) => self.open(head.trim().to_string()),
            Msg::Decl { prop, value } => self.decl(prop, value),
            Msg::Close => self.close(),
        }
    }

    fn emit(&mut self, leaves: Vec<Leaf>) {
        if let Some(top) = self.stack.last_mut() {
            match top {
                Frame::Rule { children, .. } | Frame::If { children, .. } => children.extend(leaves),
                Frame::Loop { .. } => {}
            }
        } else {
            self.out.extend(leaves);
        }
    }

    fn open(&mut self, head: String) {
        // if a Loop frame is currently collecting, buffer instead of opening (and track
        // nested block depth so only the loop's OWN close triggers replay)
        if let Some(Frame::Loop { body, depth, .. }) = self.stack.last_mut() {
            body.push(Msg::Open(head));
            *depth += 1;
            return;
        }
        let parent_sel = self.current_selector();
        if let Some(cond) = head.strip_prefix("@if ") {
            let taken = self.truthy(cond);
            self.group_taken = taken;
            self.stack.push(Frame::If {
                active: taken,
                children: Vec::new(),
            });
        } else if let Some(cond) = head.strip_prefix("@else if ") {
            let active = !self.group_taken && self.truthy(cond);
            if active {
                self.group_taken = true;
            }
            self.stack.push(Frame::If {
                active,
                children: Vec::new(),
            });
        } else if head == "@else" {
            let active = !self.group_taken;
            self.group_taken |= active;
            self.stack.push(Frame::If {
                active,
                children: Vec::new(),
            });
        } else if head.starts_with("@each ") || head.starts_with("@for ") {
            let each = head.starts_with("@each ");
            let spec: String = head.chars().skip(if each { 6 } else { 5 }).collect();
            let (key, items) = if each {
                self.each_bindings(&spec)
            } else {
                self.for_bindings(&spec)
            };
            self.stack.push(Frame::Loop {
                key,
                items,
                depth: 0,
                body: Vec::new(),
            });
        } else {
            self.group_taken = false; // ends any if/else chain
            // only control-flow (@if/@else/@each/@for) is transparent; every other
            // at-rule head (@media/@function/…) is an ordinary block, emitted verbatim
            let selector = combine(&parent_sel, &head);
            self.stack.push(Frame::Rule {
                selector,
                directive: false,
                decls: Vec::new(),
                children: Vec::new(),
            });
        }
    }

    fn current_selector(&self) -> String {
        self.stack
            .iter()
            .rev()
            .find_map(|f| match f {
                Frame::Rule { selector, .. } => Some(selector.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn decl(&mut self, prop: String, value: String) {
        if let Some(Frame::Loop { body, .. }) = self.stack.last_mut() {
            body.push(Msg::Decl { prop, value });
            return;
        }
        if matches!(self.stack.last(), Some(Frame::If { active: false, .. })) {
            return;
        }
        let Some(rendered) = self.render(&value) else {
            return;
        };
        if let Some(name) = prop.strip_prefix('$') {
            self.env.insert(name.to_string(), rendered);
        } else if let Some(Frame::Rule { decls, .. }) = self.stack.last_mut() {
            decls.push((prop, rendered));
        }
    }

    fn close(&mut self) {
        // a nested Close inside a still-collecting Loop belongs to that body, not the loop
        if let Some(Frame::Loop { depth, body, .. }) = self.stack.last_mut()
            && *depth > 0
        {
            *depth -= 1;
            body.push(Msg::Close);
            return;
        }
        match self.stack.pop() {
            Some(Frame::Rule {
                selector,
                directive,
                decls,
                mut children,
            }) => {
                let resolved = self
                    .render(&selector)
                    .unwrap_or_else(|| selector.clone());
                let mut produced = Vec::new();
                if !directive && !decls.is_empty() {
                    produced.push(Leaf {
                        selector: resolved,
                        decls,
                    });
                }
                produced.append(&mut children);
                self.emit(produced);
            }
            Some(Frame::If { active, mut children }) => {
                if active {
                    self.emit(std::mem::take(&mut children));
                }
            }
            Some(Frame::Loop {
                key,
                items,
                mut body,
                ..
            }) => {
                let mut jobs = VecDeque::new();
                items.iter().for_each(|it| {
                    jobs.push_back(Job::Bind(key.clone(), it.clone()));
                    body.iter().cloned().for_each(|m| jobs.push_back(Job::Msg(m)));
                });
                let n = jobs.len();
                (0..n).rev().for_each(|_| self.work.push_front(jobs.pop_back().unwrap()));
                body.clear();
            }
            None => {}
        }
    }

    // -- value helpers (mirror the current layer) --
    fn truthy(&mut self, cond: &str) -> bool {
        let r = self.render(cond).unwrap_or_else(|| cond.to_string());
        let t = r.trim();
        !t.is_empty() && t != "false"
    }

    fn render(&self, text: &str) -> Option<String> {
        let env = self.value_env();
        if text.contains("#{") {
            return value_to_css(text, &env);
        }
        if text.contains(['/', '*', '%']) {
            return Some(self.subst(text));
        }
        let expr = parse_expr(text);
        if matches!(expr, Expr::Bin(..) | Expr::Func { .. })
            && let Ok(v) = expr.eval(&env)
        {
            return Some(v.to_css());
        }
        Some(self.subst(text))
    }

    fn value_env(&self) -> ValueEnv {
        self.env
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    Value::parse_number(v).unwrap_or_else(|| Value::Raw(v.clone())),
                )
            })
            .collect()
    }

    fn subst(&self, text: &str) -> String {
        let is_var = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        let (mut out, var) = text.chars().fold(
            (String::new(), None::<String>),
            |(mut o, mut v), c| {
                let in_var = v.is_some();
                if in_var && is_var(c) {
                    v.as_mut().unwrap().push(c);
                } else if in_var {
                    let nm = v.take().unwrap_or_default();
                    match self.env.get(&nm) {
                        Some(val) => o.push_str(val),
                        None => {
                            o.push('$');
                            o.push_str(&nm);
                        }
                    }
                    o.push(c);
                } else if c == '$' {
                    v = Some(String::new());
                } else {
                    o.push(c);
                }
                (o, v)
            },
        );
        if let Some(nm) = var {
            match self.env.get(&nm) {
                Some(val) => out.push_str(val),
                None => {
                    out.push('$');
                    out.push_str(&nm);
                }
            }
        }
        out
    }

    fn each_bindings(&self, spec: &str) -> (String, Vec<String>) {
        match spec.split_once(" in ") {
            Some((var_part, list_part)) => {
                let var = var_part.trim().trim_start_matches('$').to_string();
                let list = self.render(list_part.trim()).unwrap_or_default();
                let items = if list.contains(',') {
                    list.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                } else {
                    list.split_whitespace().map(String::from).collect()
                };
                (var, items)
            }
            None => (String::new(), Vec::new()),
        }
    }

    fn for_bindings(&self, spec: &str) -> (String, Vec<String>) {
        match spec.split_once(" from ") {
            Some((var_part, rest)) => {
                let var = var_part.trim().trim_start_matches('$').to_string();
                let rendered = self.render(rest.trim()).unwrap_or_else(|| rest.trim().to_string());
                let toks: Vec<&str> = rendered.split_whitespace().collect();
                let num = |s: &str| s.trim().parse::<i64>().ok();
                let (Some(a), Some(b)) = (
                    toks.first().copied().and_then(num),
                    toks.get(2).copied().and_then(num),
                ) else {
                    return (var, Vec::new());
                };
                let end = match toks.get(1).copied() {
                    Some("to") => b,
                    Some("through") => b + 1,
                    _ => return (var, Vec::new()),
                };
                (var, (a..end).map(|n| n.to_string()).collect())
            }
            None => (String::new(), Vec::new()),
        }
    }
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

/// Isolated entry point (NOT wired into `compile_string`): lex -> `Msg` -> `Engine` ->
/// flat `Vec<Leaf>` -> CSS. Used to prove the new engine before cutover.
pub fn compile_engine(content: impl Into<String>) -> Result<String, crate::error::SassError> {
    use rxrust::prelude::*;
    let tokens: Vec<crate::lexer::Tok> = {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<crate::lexer::Tok>>();
        crate::lexer::token_stream(content.into())
            .collect::<Vec<_>>()
            .subscribe(move |v| {
                tx.send(v).ok();
            });
        rx.recv().unwrap_or_default()
    };
    let depth = tokens
        .iter()
        .fold(0i32, |d, t| match t {
            crate::lexer::Tok::Punct('{') => d + 1,
            crate::lexer::Tok::Punct('}') => d - 1,
            _ => d,
        });
    if depth != 0 {
        return Err(crate::error::SassError::new(
            crate::error::ErrorKind::Parse,
            "unbalanced blocks",
        ));
    }
    let leaves = Engine::run(to_msgs(tokens.into_iter()));
    Ok(leaves.iter().map(render_leaf).collect())
}

fn render_leaf(leaf: &Leaf) -> String {
    let body = leaf
        .decls
        .iter()
        .map(|(p, v)| format!("  {p}: {v};\n"))
        .collect::<String>();
    format!("{} {{\n{}}}\n", leaf.selector, body)
}
