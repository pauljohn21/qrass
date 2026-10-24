//! The expression model + evaluator — the foundation that lets values be owned
//! data flowing through the stream instead of strings re-parsed by flags.
//!
//! `Expr::eval` walks an owned expression tree against an owned `Env`; `and`/`or`/`if`
//! are **lazy** (they evaluate only the taken operand), so `false and #{$undefined}`
//! short-circuits to `false` without ever touching the undefined variable — the exact
//! behaviour several `sass-spec` `if`/`short_circuit` goldens require. An undefined
//! `$var` yields `Err` (the caller turns a failed declaration into a dropped one).
//!
//! This module is standalone for now (parser/wiring land next, A/B-gated); it is
//! `pub` so it is unit-testable without touching `compile_string`.

use std::collections::HashMap;

use crate::value::Value;

pub type Env = HashMap<String, Value>;

/// Binary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// a literal value (number/string/color/bool/null or verbatim CSS `Raw`)
    Lit(Value),
    /// a `$name` variable reference (name excludes `$`)
    Var(String),
    /// a sequence of parts that concatenate into one CSS value
    Concat(Vec<Expr>),
    /// a `#{ … }` interpolation of an expression
    Interp(Box<Expr>),
    /// `a and b` (lazy: evaluates `b` only if `a` is truthy)
    And(Box<Expr>, Box<Expr>),
    /// `a or b` (lazy: evaluates `b` only if `a` is falsey)
    Or(Box<Expr>, Box<Expr>),
    /// `if(cond, then, else)` (lazy on the taken branch); `else` may be `Null`
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// arithmetic on numeric values
    Bin(Op, Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn eval(&self, env: &Env) -> Result<Value, String> {
        match self {
            Expr::Lit(v) => Ok(v.clone()),
            Expr::Var(name) => env
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Undefined variable \"${name}\".")),
            Expr::Concat(parts) => {
                let css: Result<Vec<String>, String> =
                    parts.iter().map(|p| Ok(p.eval(env)?.to_css())).collect();
                Ok(Value::Raw(css?.join(" ")))
            }
            Expr::Interp(inner) => Ok(Value::Raw(inner.eval(env)?.to_css())),
            Expr::And(lhs, rhs) => {
                let a = lhs.eval(env)?;
                if a.is_falsey() {
                    Ok(a)
                } else {
                    rhs.eval(env)
                }
            }
            Expr::Or(lhs, rhs) => {
                let a = lhs.eval(env)?;
                if a.is_falsey() {
                    rhs.eval(env)
                } else {
                    Ok(a)
                }
            }
            Expr::If(cond, then, els) => {
                if cond.eval(env)?.is_falsey() {
                    els.eval(env)
                } else {
                    then.eval(env)
                }
            }
            Expr::Bin(op, l, r) => arith(*op, l.eval(env)?, r.eval(env)?),
        }
    }
}

fn num(v: &Value) -> Result<(f64, String), String> {
    match v {
        Value::Number(n, u) => Ok((*n, u.clone())),
        other => Err(format!("{other:?} is not a number")),
    }
}

/// The unit of an add/sub result: the non-empty of the two (equal or one empty).
fn add_unit(a: &str, b: &str) -> Result<String, String> {
    match (a.is_empty(), b.is_empty()) {
        _ if a == b => Ok(a.to_string()),
        (false, true) | (true, false) => Ok(if a.is_empty() { b.to_string() } else { a.to_string() }),
        _ => Err(format!("incompatible units {a} and {b}")),
    }
}

fn arith(op: Op, lhs: Value, rhs: Value) -> Result<Value, String> {
    let (x, xu) = num(&lhs)?;
    let (y, yu) = num(&rhs)?;
    let (v, unit) = match op {
        Op::Add => (x + y, add_unit(&xu, &yu)?),
        Op::Sub => (x - y, add_unit(&xu, &yu)?),
        Op::Mul => (x * y, format!("{xu}{yu}")),
        Op::Div => {
            if xu == yu {
                (x / y, String::new())
            } else {
                (x / y, add_unit(&xu, &yu)?)
            }
        }
        Op::Mod => (x.rem_euclid(y), add_unit(&xu, &yu)?),
    };
    Ok(Value::Number(v, unit))
}

fn is_var_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Canonicalise a raw declaration-value string to CSS, resolving `$name` references
/// and `#{ … }` interpolation, and normalising whitespace/comma spacing the way Sass
/// emits (`a,b`→`a, b`; runs of spaces collapse to one; interpolation glues to
/// adjacent text so `col-#{$k}`→`col-2`). `None` if an interpolation names an
/// undefined variable (the caller then drops the declaration).
///
/// A single left-to-right `fold` over chars with an interpolation stack — no loop,
/// no recursion, no per-case flags.
#[tracing::instrument(level = "debug", skip(env), fields(src_len = src.len()))]
pub fn value_to_css(src: &str, env: &Env) -> Option<String> {
    struct Vs {
        out: String,
        stack: Vec<String>,
        dollar: Option<String>,
        pending: bool,
        after_comma: bool,
        failed: bool,
    }
    let final_state = src.chars().fold(
        Vs {
            out: String::new(),
            stack: Vec::new(),
            dollar: None,
            pending: false,
            after_comma: false,
            failed: false,
        },
        |mut s, c| {
            if s.failed {
                return s;
            }
            if s.dollar.is_some() {
                if is_var_char(c) {
                    s.dollar.as_mut().unwrap().push(c);
                    return s;
                }
                let name = s.dollar.take().unwrap();
                match env.get(&name) {
                    Some(v) => s.out.push_str(&v.to_css()),
                    None if !s.stack.is_empty() => {
                        s.failed = true;
                        return s;
                    }
                    None => {
                        s.out.push('$');
                        s.out.push_str(&name);
                    }
                }
            }
            let emit = |s: &mut Vs, text: &str| {
                if s.pending && !s.out.is_empty() {
                    if s.after_comma {
                        s.after_comma = false;
                    } else {
                        s.out.push(' ');
                    }
                }
                s.pending = false;
                s.out.push_str(text);
            };
            match c {
                '{' if !s.stack.is_empty() => {}
                '}' if !s.stack.is_empty() => {
                    let inner = std::mem::take(&mut s.out);
                    s.out = s.stack.pop().unwrap();
                    s.out.push_str(&inner);
                }
                '#' => emit(&mut s, "#"),
                '$' => s.dollar = Some(String::new()),
                '{' if s.out.ends_with('#') => {
                    s.out.pop();
                    s.stack.push(std::mem::take(&mut s.out));
                }
                c if c.is_whitespace() => {
                    if !s.out.is_empty() {
                        s.pending = true;
                    }
                }
                ',' => {
                    s.out = std::mem::take(&mut s.out).trim_end().to_string();
                    s.out.push_str(", ");
                    s.pending = false;
                    s.after_comma = true;
                }
                c => emit(&mut s, &c.to_string()),
            }
            s
        },
    );
    let Vs {
        mut out,
        stack,
        dollar,
        failed,
        ..
    } = final_state;
    if failed || !stack.is_empty() {
        return None;
    }
    if let Some(name) = dollar {
        match env.get(&name) {
            Some(v) => out.push_str(&v.to_css()),
            None => {
                out.push('$');
                out.push_str(&name);
            }
        }
    }
    Some(out.trim().to_string())
}
