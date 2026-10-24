//! Owned expression model: a value/selector token run folds ONCE into an `Expr`, which
//! `eval` resolves against an owned `Env` to a `Value` (lazy — nothing is rendered until
//! `eval`/`to_css` is asked). No string is rebuilt and re-scanned; the tokens carry the
//! structure (`Var`, `Interp`) directly.
//!
//! Beyond literals/`$var`/`#{}`/concat, this layer models arithmetic (`Bin`), comparisons
//! (`Cmp`), lazy boolean (`And`/`Or`), the 3-arg `if`, and function calls (`Func`) whose
//! bodies resolve through the per-domain `builtins` allowlist. An unrecognised name → `Err`,
//! so the caller keeps the value verbatim (CSS-native functions like `url`/`calc` are never
//! evaluated). The unit algebra (`arith`) stays here because `eval`'s `Bin` uses it too.

use std::collections::HashMap;

use crate::lexer::Tok;
use crate::value::Value;

pub type Env = HashMap<String, Value>;

/// Binary arithmetic operators (Sass `math` semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Value),
    Var(String),
    Interp(Box<Expr>),
    Concat(Vec<Expr>),
    /// `lhs op rhs` arithmetic
    Bin(Box<Expr>, Op, Box<Expr>),
    /// `lhs cmp rhs` comparison (yields `Value::Bool`)
    Cmp(Box<Expr>, Cmp, Box<Expr>),
    /// lazy `and` (short-circuits on a falsey lhs)
    And(Box<Expr>, Box<Expr>),
    /// lazy `or` (short-circuits on a truthy lhs)
    Or(Box<Expr>, Box<Expr>),
    /// `if(cond, then, else)` — evaluates only the taken branch
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `name(args…)`
    Func { name: String, args: Vec<Expr> },
}

impl Expr {
    /// Fold a value/selector token run into an `Expr` (one structured pass).
    pub fn from_tokens(toks: &[Tok]) -> Expr {
        let parts: Vec<Expr> = toks
            .iter()
            .map(|t| match t {
                Tok::Word(w) => Expr::Lit(Value::Raw(w.clone())),
                Tok::Punct(',') => Expr::Lit(Value::Raw(", ".to_string())),
                Tok::Punct(c) => Expr::Lit(Value::Raw(c.to_string())),
                Tok::Var(n) => Expr::Var(n.clone()),
                Tok::Interp(inner) => Expr::Interp(Box::new(Expr::from_tokens(inner))),
            })
            .collect();
        if parts.len() == 1 {
            parts.into_iter().next().unwrap()
        } else {
            Expr::Concat(parts)
        }
    }

    /// Render a token run to CSS text. Interpolation of an undefined `$var` yields
    /// `None` (the caller drops that declaration); a bare unknown `$var` is kept
    /// verbatim so plain-CSS values never regress.
    pub fn render(toks: &[Tok], env: &Env) -> Option<String> {
        let expr = Expr::from_tokens(toks);
        let css = expr.eval(env).ok()?.to_css();
        Some(canon(&css))
    }

    pub fn eval(&self, env: &Env) -> Result<Value, String> {
        Ok(match self {
            Expr::Lit(v) => v.clone(),
            Expr::Var(name) => env
                .get(name)
                .cloned()
                .unwrap_or_else(|| Value::Raw(format!("${name}"))),
            Expr::Interp(inner) => Value::Raw(inner.eval_strict(env)?),
            Expr::Concat(parts) => Value::Raw(
                parts
                    .iter()
                    .map(|p| p.eval(env).map(|v| v.to_css()))
                    .collect::<Result<String, String>>()?,
            ),
            Expr::Bin(l, op, r) => arith(*op, l.eval(env)?, r.eval(env)?)?,
            Expr::Cmp(l, c, r) => compare(l.eval(env)?, *c, r.eval(env)?)?,
            // lazy: the untaken side is never evaluated
            Expr::And(l, r) => {
                let v = l.eval(env)?;
                if v.is_falsey() { v } else { r.eval(env)? }
            }
            Expr::Or(l, r) => {
                let v = l.eval(env)?;
                if v.is_falsey() { r.eval(env)? } else { v }
            }
            Expr::If(c, t, e) => {
                if c.eval(env)?.is_falsey() { e.eval(env)? } else { t.eval(env)? }
            }
            Expr::Func { name, args } => {
                let vals: Result<Vec<Value>, String> = args.iter().map(|a| a.eval(env)).collect();
                call_builtin(name, vals?)?
            }
        })
    }

    /// Like `eval` but an undefined `$var` is an error (used inside `#{ … }`).
    fn eval_strict(&self, env: &Env) -> Result<String, String> {
        match self {
            Expr::Var(name) => match env.get(name) {
                Some(v) => Ok(v.to_css()),
                None => Err(format!("Undefined variable \"${name}\".")),
            },
            Expr::Interp(inner) => inner.eval_strict(env),
            other => other.eval(env).map(|v| v.to_css()),
        }
    }
}

/// Entry point for builtin resolution; delegates to the per-domain `builtins` modules so
/// each family lives in its own file rather than one growing `match` here.
pub fn call_builtin(name: &str, args: Vec<Value>) -> Result<Value, String> {
    crate::builtins::call(name, &args)
}

/// Sass numeric arithmetic with unit rules. Non-numeric operands or unit mismatches → `Err`,
/// so the caller keeps the value verbatim (never a wrong CSS number).
pub(crate) fn arith(op: Op, l: Value, r: Value) -> Result<Value, String> {
    let (a, ua) = num(&l)?;
    let (b, ub) = num(&r)?;
    let out = match op {
        Op::Add | Op::Sub => {
            let unit = combine_add_unit(&ua, &ub)?;
            let n = if op == Op::Add { a + b } else { a - b };
            Value::Number(n, unit)
        }
        Op::Mul => Value::Number(a * b, mul_unit(&ua, &ub)?),
        Op::Div => Value::Number(a / b, div_unit(&ua, &ub)?),
        Op::Mod => {
            let unit = combine_add_unit(&ua, &ub)?;
            Value::Number(a % b, unit)
        }
    };
    Ok(out)
}

/// Extract a `(value, unit)` pair from a numeric `Value`.
pub(crate) fn num(v: &Value) -> Result<(f64, String), String> {
    match v {
        Value::Number(n, u) => Ok((*n, u.clone())),
        _ => Err(format!("{v:?} is not a number")),
    }
}

fn combine_add_unit(a: &str, b: &str) -> Result<String, String> {
    if a == b {
        Ok(a.to_string())
    } else if a.is_empty() {
        Ok(b.to_string())
    } else if b.is_empty() {
        Ok(a.to_string())
    } else {
        Err(format!("Incompatible units \"{a}\" and \"{b}\"."))
    }
}

fn mul_unit(a: &str, b: &str) -> Result<String, String> {
    if a.is_empty() {
        Ok(b.to_string())
    } else if b.is_empty() || a == b {
        Ok(a.to_string())
    } else {
        Ok(format!("{a}{b}"))
    }
}

fn div_unit(a: &str, b: &str) -> Result<String, String> {
    if a == b {
        Ok(String::new())
    } else if b.is_empty() {
        Ok(a.to_string())
    } else {
        Err(format!("Cannot divide \"{a}\" by \"{b}\"."))
    }
}

/// Sass comparison; yields `Value::Bool`. Used by `Expr::Cmp` and by `builtins` min/max.
pub(crate) fn compare(l: Value, c: Cmp, r: Value) -> Result<Value, String> {
    use std::cmp::Ordering;
    let ord = match (&l, &r) {
        (Value::Number(a, ua), Value::Number(b, ub)) => {
            if ua != ub {
                return Err(format!("Cannot compare \"{ua}\" with \"{ub}\"."));
            }
            a.partial_cmp(b)
        }
        (Value::Str { text: a, .. }, Value::Str { text: b, .. }) => Some(a.cmp(b)),
        (Value::Raw(a), Value::Raw(b)) => Some(a.cmp(b)),
        _ => None,
    }
    .ok_or_else(|| "Cannot compare these values.".to_string())?;
    let b = match c {
        Cmp::Eq => ord == Ordering::Equal,
        Cmp::Ne => ord != Ordering::Equal,
        Cmp::Gt => ord == Ordering::Greater,
        Cmp::Lt => ord == Ordering::Less,
        Cmp::Ge => ord != Ordering::Less,
        Cmp::Le => ord != Ordering::Greater,
    };
    Ok(Value::Bool(b))
}

/// Normalise whitespace runs to single spaces (interpolation gluing is already exact
/// because the tokens carry surrounding spaces verbatim).
fn canon(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
