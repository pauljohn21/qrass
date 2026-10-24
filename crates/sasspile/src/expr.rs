//! Owned expression model: a value/selector token run folds ONCE into an `Expr`, which
//! `eval` resolves against an owned `Env` to a `Value` (lazy — nothing is rendered until
//! `eval`/`to_css` is asked). No string is rebuilt and re-scanned; the tokens carry the
//! structure (`Var`, `Interp`) directly.
//!
//! This layer covers literals, `$var`, `#{ … }` interpolation and concatenation. Numeric
//! arithmetic and function calls land after the value-tokenizer emits number/operator
//! tokens (deferred so nothing here re-scans text).

use std::collections::HashMap;

use crate::lexer::Tok;
use crate::value::Value;

pub type Env = HashMap<String, Value>;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Value),
    Var(String),
    Interp(Box<Expr>),
    Concat(Vec<Expr>),
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

    fn eval(&self, env: &Env) -> Result<Value, String> {
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

/// Normalise whitespace runs to single spaces (interpolation gluing is already exact
/// because the tokens carry surrounding spaces verbatim).
fn canon(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
