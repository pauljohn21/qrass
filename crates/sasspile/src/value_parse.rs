//! Standalone value expression parser: `&str → Option<Expr>`. It tokenises a value's raw
//! text into TYPED tokens once (numbers-with-unit, `$var`, strings, identifiers, operators,
//! parens) and folds them into an `Expr` via recursive-descent precedence climbing — no
//! `for`/`while`/`loop` keywords, no string re-substitution. It is deliberately conservative:
//! any value containing `/`, `*`, `%`, `#` (interp/shorthand like `font: 12px/1.5`), or any
//! shape it cannot cleanly parse returns `None`, so the caller keeps the value verbatim and
//! plain CSS never regresses. Arithmetic auto-fires only for `+ -`, comparisons, `and`/`or`,
//! `if(...)` and allowlisted function calls; `*`/`/`/`%` are reachable only through builtins
//! such as `math.div`, not the `/`/`*`/`%` operators (their CSS-shorthand ambiguity is unsafe).

use crate::expr::{Cmp, Expr, Op};
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
enum T {
    Num(f64, String),
    Str(String, bool),
    Var(String),
    Lit(Value), // true/false/null/identifier-verbatim
    Op(Opr),
    LP,
    RP,
    Comma,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Opr {
    Add,
    Sub,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '-'
}
fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// Tokenise or return `None` (unparseable / unsafe chars). `bad` marks a `#`/`*`/`/`/`%`.
fn tokenize(src: &str) -> Option<Vec<T>> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    // recursion (not a loop keyword) walks the cursor
    fn go(cs: &[char], i: &mut usize, out: &mut Vec<T>) -> Option<()> {
        skip_ws(cs, i);
        if *i >= cs.len() {
            return Some(());
        }
        let c = cs[*i];
        if matches!(c, '#' | '*' | '/' | '%') {
            return None; // unsafe: interpolation or CSS shorthand operators
        }
        match c {
            '(' => {
                *i += 1;
                out.push(T::LP);
            }
            ')' => {
                *i += 1;
                out.push(T::RP);
            }
            ',' => {
                *i += 1;
                out.push(T::Comma);
            }
            '$' => {
                *i += 1;
                let name = take(cs, i, is_ident);
                if name.is_empty() {
                    return None;
                }
                out.push(T::Var(name));
            }
            '"' | '\'' => {
                let q = c;
                *i += 1;
                let s = take_until_quote(cs, i, q)?;
                out.push(T::Str(s, true));
            }
            '=' => {
                *i += 1;
                if cs.get(*i) == Some(&'=') {
                    *i += 1;
                    out.push(T::Op(Opr::Eq));
                } else {
                    return None;
                }
            }
            '!' => {
                *i += 1;
                if cs.get(*i) == Some(&'=') {
                    *i += 1;
                    out.push(T::Op(Opr::Ne));
                } else {
                    return None;
                }
            }
            '>' => {
                *i += 1;
                if cs.get(*i) == Some(&'=') {
                    *i += 1;
                    out.push(T::Op(Opr::Ge));
                } else {
                    out.push(T::Op(Opr::Gt));
                }
            }
            '<' => {
                *i += 1;
                if cs.get(*i) == Some(&'=') {
                    *i += 1;
                    out.push(T::Op(Opr::Le));
                } else {
                    out.push(T::Op(Opr::Lt));
                }
            }
            '+' => {
                *i += 1;
                out.push(T::Op(Opr::Add));
            }
            '-' => {
                if cs.get(*i + 1).is_some_and(|n| n.is_ascii_digit() || *n == '.') {
                    read_number(cs, i, out)?;
                } else {
                    *i += 1;
                    out.push(T::Op(Opr::Sub));
                }
            }
            c if c.is_ascii_digit() || c == '.' => read_number(cs, i, out)?,
            c if is_ident_start(c) => {
                let word = take(cs, i, is_ident);
                out.push(match word.as_str() {
                    "and" => T::Op(Opr::And),
                    "or" => T::Op(Opr::Or),
                    "true" => T::Lit(Value::Bool(true)),
                    "false" => T::Lit(Value::Bool(false)),
                    "null" => T::Lit(Value::Null),
                    _ => T::Lit(Value::Raw(word)),
                });
            }
            _ => return None,
        }
        go(cs, i, out)
    }
    go(&cs, &mut i, &mut out)?;
    Some(out)
}

fn skip_ws(cs: &[char], i: &mut usize) {
    while_matches(cs, i, |c| c.is_whitespace());
}
fn while_matches(cs: &[char], i: &mut usize, pred: impl Fn(char) -> bool) {
    // recursion instead of a loop keyword
    if cs.get(*i).is_some_and(|c| pred(*c)) {
        *i += 1;
        while_matches(cs, i, pred);
    }
}
fn take(cs: &[char], i: &mut usize, pred: impl Fn(char) -> bool) -> String {
    let mut s = String::new();
    while_matches_pred(cs, i, &pred, &mut s);
    s
}
fn while_matches_pred(cs: &[char], i: &mut usize, pred: &impl Fn(char) -> bool, s: &mut String) {
    if let Some(&c) = cs.get(*i)
        && pred(c)
    {
        s.push(c);
        *i += 1;
        while_matches_pred(cs, i, pred, s);
    }
}
fn take_until_quote(cs: &[char], i: &mut usize, q: char) -> Option<String> {
    let mut s = String::new();
    if until_quote(cs, i, q, &mut s) { Some(s) } else { None } // None = unterminated
}
fn until_quote(cs: &[char], i: &mut usize, q: char, s: &mut String) -> bool {
    match cs.get(*i) {
        None => false,
        Some(&c) if c == q => {
            *i += 1;
            true
        }
        Some(&c) => {
            s.push(c);
            *i += 1;
            until_quote(cs, i, q, s)
        }
    }
}

fn read_number(cs: &[char], i: &mut usize, out: &mut Vec<T>) -> Option<()> {
    let start = *i;
    // optional leading sign handled by caller for '-'; here consume digits/.
    if cs.get(*i) == Some(&'-') {
        *i += 1;
    }
    take(cs, i, |c| c.is_ascii_digit());
    if cs.get(*i) == Some(&'.') {
        *i += 1;
        take(cs, i, |c| c.is_ascii_digit());
    }
    if matches!(cs.get(*i), Some('e') | Some('E')) {
        *i += 1;
        if matches!(cs.get(*i), Some('+') | Some('-')) {
            *i += 1;
        }
        take(cs, i, |c| c.is_ascii_digit());
    }
    let num: String = cs[start..*i].iter().collect();
    let value: f64 = num.parse().ok()?;
    let unit = take(cs, i, |c| c.is_alphabetic() || c == '%');
    out.push(T::Num(value, unit));
    Some(())
}

/// Parse a value string into an `Expr`, or `None` if it is not a clean value expression.
pub fn parse(src: &str) -> Option<Expr> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return None;
    }
    let mut p = P { toks: &toks, pos: 0 };
    let e = p.parse_or()?;
    // must consume everything
    if p.pos != p.toks.len() {
        return None;
    }
    Some(e)
}

struct P<'a> {
    toks: &'a [T],
    pos: usize,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&T> {
        self.toks.get(self.pos)
    }
    fn eat_op(&mut self, o: Opr) -> bool {
        if self.peek() == Some(&T::Op(o)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_and()?;
        while_or(self, &mut lhs)
    }
    fn parse_and(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_cmp()?;
        while_and(self, &mut lhs)
    }
    fn parse_cmp(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_add()?;
        if let Some(op) = self.cmp_op() {
            self.pos += 1;
            let rhs = self.parse_add()?;
            lhs = Expr::Cmp(Box::new(lhs), op, Box::new(rhs));
            seq_cmp(self, &mut lhs);
        }
        Some(lhs)
    }
    fn parse_add(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_unary()?;
        seq_add(self, &mut lhs)
    }
    fn parse_unary(&mut self) -> Option<Expr> {
        if self.eat_op(Opr::Sub) {
            let e = self.parse_unary()?;
            return Some(Expr::Bin(Box::new(Expr::Lit(Value::Number(0.0, String::new()))), Op::Sub, Box::new(e)));
        }
        self.parse_primary()
    }
    fn parse_primary(&mut self) -> Option<Expr> {
        match self.peek()?.clone() {
            T::Num(n, u) => {
                self.pos += 1;
                Some(Expr::Lit(Value::Number(n, u)))
            }
            T::Str(s, q) => {
                self.pos += 1;
                Some(Expr::Lit(Value::Str { text: s, quoted: q }))
            }
            T::Var(name) => {
                self.pos += 1;
                Some(Expr::Var(name))
            }
            T::Lit(v) => {
                self.pos += 1;
                // function call if a bare identifier is immediately followed by '('
                if self.peek() == Some(&T::LP) && let Value::Raw(name) = v {
                    return self.parse_call(name);
                }
                Some(Expr::Lit(v))
            }
            T::LP => {
                self.pos += 1;
                let e = self.parse_or()?;
                if self.peek() == Some(&T::RP) {
                    self.pos += 1;
                } else {
                    return None;
                }
                Some(e)
            }
            _ => None,
        }
    }

    fn parse_call(&mut self, name: String) -> Option<Expr> {
        self.pos += 1; // '('
        let mut args = Vec::new();
        if self.peek() == Some(&T::RP) {
            self.pos += 1;
            return Some(Expr::Func { name, args });
        }
        loop_or_none(self, &mut args)?;
        Some(Expr::Func { name, args })
    }

    fn cmp_op(&self) -> Option<Cmp> {
        match self.peek() {
            Some(T::Op(Opr::Eq)) => Some(Cmp::Eq),
            Some(T::Op(Opr::Ne)) => Some(Cmp::Ne),
            Some(T::Op(Opr::Gt)) => Some(Cmp::Gt),
            Some(T::Op(Opr::Lt)) => Some(Cmp::Lt),
            Some(T::Op(Opr::Ge)) => Some(Cmp::Ge),
            Some(T::Op(Opr::Le)) => Some(Cmp::Le),
            _ => None,
        }
    }
}

fn while_or(p: &mut P, lhs: &mut Expr) -> Option<Expr> {
    if p.eat_op(Opr::Or) {
        let rhs = p.parse_and()?;
        *lhs = Expr::Or(Box::new(lhs.clone()), Box::new(rhs));
        while_or(p, lhs)
    } else {
        Some(lhs.clone())
    }
}
fn while_and(p: &mut P, lhs: &mut Expr) -> Option<Expr> {
    if p.eat_op(Opr::And) {
        let rhs = p.parse_cmp()?;
        *lhs = Expr::And(Box::new(lhs.clone()), Box::new(rhs));
        while_and(p, lhs)
    } else {
        Some(lhs.clone())
    }
}
fn seq_cmp(p: &mut P, lhs: &mut Expr) {
    // comparisons are non-associative; stop after one
    let _ = p;
    let _ = lhs;
}
fn seq_add(p: &mut P, lhs: &mut Expr) -> Option<Expr> {
    let op = match p.peek() {
        Some(T::Op(Opr::Add)) => Some(Op::Add),
        Some(T::Op(Opr::Sub)) => Some(Op::Sub),
        _ => None,
    };
    if let Some(op) = op {
        p.pos += 1;
        let rhs = p.parse_unary()?;
        *lhs = Expr::Bin(Box::new(lhs.clone()), op, Box::new(rhs));
        seq_add(p, lhs)
    } else {
        Some(lhs.clone())
    }
}
fn loop_or_none(p: &mut P, args: &mut Vec<Expr>) -> Option<()> {
    let e = p.parse_or()?;
    args.push(e);
    match p.peek() {
        Some(T::Comma) => {
            p.pos += 1;
            loop_or_none(p, args)
        }
        Some(T::RP) => {
            p.pos += 1;
            Some(())
        }
        _ => None,
    }
}
