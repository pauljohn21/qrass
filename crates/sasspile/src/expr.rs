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
        }
    }
}
