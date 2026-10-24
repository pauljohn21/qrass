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
    /// a function call `name(args…)`
    Func {
        name: String,
        args: Vec<Expr>,
    },
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
            Expr::Func { name, args } => {
                // `if()` is lazy: evaluate only the taken branch.
                if name == "if" {
                    return match (args.first(), args.get(1), args.get(2)) {
                        (Some(c), Some(t), Some(e)) => {
                            if c.eval(env)?.is_falsey() {
                                e.eval(env)
                            } else {
                                t.eval(env)
                            }
                        }
                        _ => Err("if() requires three arguments".to_string()),
                    };
                }
                let vals: Result<Vec<Value>, String> = args.iter().map(|a| a.eval(env)).collect();
                call_builtin(name, vals?)
            }
        }
    }
}

/// Evaluate a function call against a small allowlist of unambiguous numeric builtins.
/// Unknown names error → the caller keeps the value verbatim, so CSS-native functions
/// (`url`, `calc`, `min`, `max`, …) are never evaluated as Sass builtins.
fn call_builtin(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match (name, args.as_slice()) {
        ("math.div", [a, b]) => arith(Op::Div, a.clone(), b.clone()),
        ("math.sqrt" | "sqrt", [Value::Number(n, _)]) => Ok(Value::Number(n.sqrt(), String::new())),
        ("math.pow" | "pow", [Value::Number(b, _), Value::Number(e, _)]) => {
            Ok(Value::Number(b.powf(*e), String::new()))
        }
        ("percentage", [Value::Number(n, _)]) => Ok(Value::Number(n * 100.0, "%".into())),
        ("abs", [Value::Number(n, u)]) => Ok(Value::Number(n.abs(), u.clone())),
        ("to-upper-case", [a]) => Ok(Value::Raw(a.to_css().to_uppercase())),
        ("to-lower-case", [a]) => Ok(Value::Raw(a.to_css().to_lowercase())),
        ("quote", [a]) => {
            let s = a.to_css();
            Ok(Value::Raw(
                if s.starts_with('"') && s.ends_with('"') { s } else { format!("\"{s}\"") },
            ))
        }
        ("unquote", [a]) => Ok(Value::Raw(a.to_css().trim_matches('"').to_string())),
        (f, [Value::Number(n, u)]) if matches!(f, "ceil" | "floor" | "round") => {
            let r = match f {
                "ceil" => n.ceil(),
                "floor" => n.floor(),
                _ => n.round(),
            };
            Ok(Value::Number(r, u.clone()))
        }
        _ => Err(format!("unsupported function {name}")),
    }
}

/// Recognise `name(...)` as a function call; args split on top-level commas (fold).
fn parse_call(src: &str) -> Option<Expr> {
    let t = src.trim();
    let open = t.find('(')?;
    let name = &t[..open];
    let is_ident = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !is_ident || !t.ends_with(')') {
        return None;
    }
    let inner = &t[open + 1..t.len() - 1];
    Some(Expr::Func {
        name: name.trim().to_string(),
        args: split_commas(inner).into_iter().map(|a| parse_expr(&a)).collect(),
    })
}

fn split_commas(s: &str) -> Vec<String> {
    let (mut parts, cur, _) = s.chars().fold(
        (Vec::<String>::new(), String::new(), 0usize),
        |(mut p, mut cur, d), c| match c {
            '(' => {
                cur.push(c);
                (p, cur, d + 1)
            }
            ')' if d > 0 => {
                cur.push(c);
                (p, cur, d - 1)
            }
            ',' if d == 0 => {
                p.push(std::mem::take(&mut cur));
                (p, cur, d)
            }
            _ => {
                cur.push(c);
                (p, cur, d)
            }
        },
    );
    parts.push(cur);
    parts
        .into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect()
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

#[derive(Debug, Clone)]
enum Tk {
    Num(f64, String),
    Var(String),
    Op(char),
}

fn prec(c: char) -> u8 {
    match c {
        '+' | '-' => 1,
        '*' | '/' | '%' => 2,
        _ => 0,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Idle,
    Num,
    Var,
}

struct Lx {
    toks: Vec<Tk>,
    num: String,
    unit: String,
    var: String,
    mode: Mode,
    bad: bool,
}

impl Lx {
    fn finish_num(&mut self) {
        let n = std::mem::take(&mut self.num);
        let u = std::mem::take(&mut self.unit);
        match n.parse::<f64>() {
            Ok(v) => self.toks.push(Tk::Num(v, u)),
            Err(_) => self.bad = true,
        }
    }
    fn finish_var(&mut self) {
        self.toks.push(Tk::Var(std::mem::take(&mut self.var)));
    }
    fn idle(&mut self, c: char) {
        if c.is_whitespace() {
            // skip
        } else if c == '$' {
            self.mode = Mode::Var;
        } else if c.is_ascii_digit() || c == '.' {
            self.mode = Mode::Num;
            self.num.push(c);
        } else if matches!(c, '+' | '-' | '*' | '/' | '%') {
            self.toks.push(Tk::Op(c));
        } else {
            self.bad = true;
        }
    }
    fn step(&mut self, c: char) {
        if self.bad {
            return;
        }
        match self.mode {
            Mode::Num => {
                if (c.is_ascii_digit() || c == '.') && self.unit.is_empty() {
                    self.num.push(c);
                } else if c.is_ascii_alphabetic() {
                    self.unit.push(c);
                } else {
                    self.finish_num();
                    if self.bad {
                        return;
                    }
                    self.mode = Mode::Idle;
                    self.idle(c);
                }
            }
            Mode::Var => {
                if is_var_char(c) {
                    self.var.push(c);
                } else {
                    self.finish_var();
                    self.mode = Mode::Idle;
                    self.idle(c);
                }
            }
            Mode::Idle => self.idle(c),
        }
    }
}

/// Tokenise an arithmetic expression via a fold over chars (no index loop). `None` (→
/// caller keeps the value verbatim) if it contains anything other than numbers, `$vars`
/// and `+ - * / %`, so identifiers like `c - d`, `url(..)`, `calc(..)`, lists are safe.
fn lex_arith(src: &str) -> Option<Vec<Tk>> {
    let mut lx = src.chars().fold(
        Lx {
            toks: Vec::new(),
            num: String::new(),
            unit: String::new(),
            var: String::new(),
            mode: Mode::Idle,
            bad: false,
        },
        |mut l, c| {
            l.step(c);
            l
        },
    );
    match lx.mode {
        Mode::Num => lx.finish_num(),
        Mode::Var => lx.finish_var(),
        Mode::Idle => {}
    }
    if lx.bad || lx.toks.is_empty() {
        return None;
    }
    let ok = lx.toks.iter().all(|t| !matches!(t, Tk::Op(_))) || lx.toks.len() >= 3;
    ok.then_some(lx.toks)
}

/// Parse a value string into an `Expr`: arithmetic over numbers/`$vars` becomes a
/// `Bin` tree; anything else stays `Lit(Raw)` (verbatim).
pub fn parse_expr(src: &str) -> Expr {
    if let Some(call) = parse_call(src) {
        return call;
    }
    let Some(toks) = lex_arith(src) else {
        return Expr::Lit(Value::Raw(src.trim().to_string()));
    };
    // shunting-yard to postfix (two precedences, left-associative) — a fold, no loop
    let (mut out, mut ops) = toks.into_iter().fold(
        (Vec::<Tk>::new(), Vec::<char>::new()),
        |(mut out, mut ops), tk| {
            match tk {
                Tk::Num(..) | Tk::Var(_) => out.push(tk),
                Tk::Op(c) => {
                    let cut = ops.iter().position(|&top| prec(top) < prec(c)).unwrap_or(ops.len());
                    out.extend(ops.drain(..cut).map(Tk::Op));
                    ops.push(c);
                }
            }
            (out, ops)
        },
    );
    out.extend(ops.drain(..).rev().map(Tk::Op));
    // fold postfix into a tree; bail to Raw on any shape error
    let tree = out.into_iter().fold(Vec::<Expr>::new(), |mut st, tk| {
        match tk {
            Tk::Num(v, u) => st.push(Expr::Lit(Value::Number(v, u))),
            Tk::Var(n) => st.push(Expr::Var(n)),
            Tk::Op(o) => {
                let op = match o {
                    '+' => Op::Add,
                    '-' => Op::Sub,
                    '*' => Op::Mul,
                    '/' => Op::Div,
                    _ => Op::Mod,
                };
                let r = st.pop();
                let l = st.pop();
                match (l, r) {
                    (Some(l), Some(r)) => st.push(Expr::Bin(op, Box::new(l), Box::new(r))),
                    _ => st.push(Expr::Lit(Value::Raw(src.trim().to_string()))),
                }
            }
        }
        st
    });
    if tree.len() == 1 {
        tree.into_iter().next().unwrap()
    } else {
        Expr::Lit(Value::Raw(src.trim().to_string()))
    }
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
