//! Expression evaluation (task 5.3 + 7.2): fold an owned [`Expr`] against the
//! live [`Env`] into an owned [`SassValue`]. Tree-walking lives on [`EvalState`]
//! (a method) so a `FunctionCall` can enter a scope, run a `@function` body and
//! capture `@return` — arithmetic/comparison helpers stay pure over values.

use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep, Literal, UnaryOp};
use crate::error::{ErrorKind, SassError};
use crate::token::{Piece, Quote};
use crate::value::{Color, SassValue, convert_number};

use super::eval_function::is_meta_name;

use crate::ast::Stmt;

use super::{Ev, EvalState};

type Ee<T> = Result<T, SassError>;

impl EvalState {
    /// Evaluate `expr` against the current environment.
    pub(crate) fn eval_expr(&mut self, expr: &Expr) -> Ev<SassValue> {
        match expr {
            // `&` inside a `#{…}` expression refers to the *current* rule's
            // resolved parent selector(s) (baked at definition time), not a
            // literal ampersand. Bare `&` combinators never reach here — they
            // stay as selector `Piece::Text` and are handled by combine.
            Expr::Literal(Literal::Ident(name)) if name == "&" => Ok(SassValue::Str {
                text: self.current_parents.join(", "),
                quoted: false,
            }),
            Expr::Literal(l) => Ok(eval_literal(l)),
            Expr::Variable(name, _) => self.env.get(name).cloned().ok_or_else(|| {
                err(
                    ErrorKind::UndefinedVariable,
                    format!("Undefined variable \"${name}\""),
                )
            }),
            Expr::Member {
                namespace, name, ..
            } => self
                .env
                .get_member(namespace, name)
                .cloned()
                .or_else(|| crate::builtins::module_const(namespace, name))
                .ok_or_else(|| {
                    err(
                        ErrorKind::UndefinedVariable,
                        format!("Undefined variable \"${name}\" in module \"{namespace}\""),
                    )
                }),
            Expr::Paren(inner, _) => self.eval_expr(inner),
            Expr::List { items, sep, .. } => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.eval_expr(item)?);
                }
                Ok(SassValue::List {
                    items: out,
                    sep: *sep,
                })
            }
            Expr::Map { pairs, .. } => {
                let mut entries = Vec::with_capacity(pairs.len());
                for (k, v) in pairs {
                    entries.push((self.eval_expr(k)?, self.eval_expr(v)?));
                }
                Ok(SassValue::Map { entries })
            }
            Expr::Interpolated { quote, pieces, .. } => {
                let mut text = String::new();
                for p in pieces {
                    match p {
                        Piece::Text(t) => text.push_str(t),
                        Piece::Expr(src) => {
                            text.push_str(&self.eval_interp_source(src)?.interp_text())
                        }
                    }
                }
                Ok(SassValue::Str {
                    text,
                    quoted: matches!(quote, Some(_)),
                })
            }
            Expr::Unary { op, operand, .. } => {
                let v = self.eval_expr(operand)?;
                Ok(match op {
                    UnaryOp::Not => SassValue::Bool(!v.is_truthy()),
                    UnaryOp::Neg => negate(v)?,
                    UnaryOp::Plus => v,
                })
            }
            Expr::Binary {
                op, left, right, ..
            } => self.eval_binary(*op, left, right),
            Expr::FunctionCall(call) => self.eval_call(call),
        }
    }

    fn eval_binary(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Ev<SassValue> {
        // Short-circuit logical operators return an operand value, not a bool.
        match op {
            BinOp::Or => {
                let l = self.eval_expr(left)?;
                return Ok(if l.is_truthy() {
                    l
                } else {
                    self.eval_expr(right)?
                });
            }
            BinOp::And => {
                let l = self.eval_expr(left)?;
                return Ok(if !l.is_truthy() {
                    l
                } else {
                    self.eval_expr(right)?
                });
            }
            _ => {}
        }
        let l = self.eval_expr(left)?;
        let r = self.eval_expr(right)?;
        match op {
            // `+` is string concatenation when either side is a string (element-plus
            // builds selectors as `$block + $sep + $element`); numeric otherwise.
            BinOp::Add
                if matches!(l, SassValue::Str { .. }) || matches!(r, SassValue::Str { .. }) =>
            {
                let text = |v: &SassValue| match v {
                    SassValue::Str { text, .. } => text.clone(),
                    other => other.to_css(),
                };
                Ok(SassValue::Str {
                    text: format!("{}{}", text(&l), text(&r)),
                    quoted: false,
                })
            }
            BinOp::Add | BinOp::Sub => arith(op, l, r),
            BinOp::Mul | BinOp::Div => arith(op, l, r),
            BinOp::Eq => Ok(SassValue::Bool(values_equal(&l, &r))),
            BinOp::Ne => Ok(SassValue::Bool(!values_equal(&l, &r))),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => compare(op, l, r),
            _ => Err(err(ErrorKind::Eval, format!("unsupported operator {op:?}"))),
        }
    }

    /// Re-lex/re-parse a raw interpolation source fragment and evaluate it. Used
    /// for `#{…}` inside a value expression (selectors/props use `render_interp`).
    fn eval_interp_source(&mut self, src: &str) -> Ev<SassValue> {
        let wrapped = format!("$v: {src};");
        let tokens = crate::lexer::lex(0, "interp".into(), wrapped.clone())?;
        let program = crate::parser::parse(0, "interp".into(), wrapped, tokens)?;
        match program.first() {
            Some(Stmt::Variable(v)) => self.eval_expr(&v.value),
            _ => Err(err(
                ErrorKind::Eval,
                "expected an expression in interpolation",
            )),
        }
    }

    /// Render an expression as *plain CSS* text inside a passthrough call
    /// (`calc`/`var`/…). Sass math is disabled here, so binary/unary operators and
    /// parentheses are kept literally; interpolation (`#{…}`), Sass function calls
    /// (user or builtin) and variables are still resolved. Nested plain-CSS calls
    /// recurse; every other node falls back to ordinary evaluation via `to_css`.
    pub(crate) fn render_css(&mut self, expr: &Expr) -> Ev<String> {
        match expr {
            Expr::Binary {
                op, left, right, ..
            } => {
                // Inside a passthrough call dart-sass keeps `+`/`-` (and any
                // operand touching a `var()`/function) as literal CSS, but it
                // *does* run Sass arithmetic for `*`/`/` when both sides are
                // plain Sass numbers — e.g. `calc(… - $border-width * 2)` →
                // `calc(… - 2px)`. Detect that narrow case and fold it.
                if matches!(op, BinOp::Mul | BinOp::Div) {
                    if let (Some(lv), Some(rv)) = (self.try_number(left)?, self.try_number(right)?) {
                        if let (SassValue::Number { .. }, SassValue::Number { .. }) = (&lv, &rv) {
                            return Ok(arith(*op, lv, rv)?.to_css());
                        }
                    }
                }
                let l = self.render_css(left)?;
                let r = self.render_css(right)?;
                let sym = op_symbol(*op);
                // Sass math keeps dart-sass's spaced form (`100% - 32px`); an
                // `=`/comparison inside a plain-CSS body stays tight (`opacity=0`).
                Ok(if spaced_op(*op) {
                    format!("{l} {sym} {r}")
                } else {
                    format!("{l}{sym}{r}")
                })
            }
            Expr::Paren(inner, _) => Ok(format!("({})", self.render_css(inner)?)),
            Expr::Unary { op, operand, .. } => {
                let inner = self.render_css(operand)?;
                Ok(match op {
                    UnaryOp::Neg => format!("-{inner}"),
                    UnaryOp::Plus => format!("+{inner}"),
                    UnaryOp::Not => format!("not {inner}"),
                })
            }
            Expr::List { items, sep, .. } => {
                let join = match sep {
                    ListSep::Comma => ", ",
                    ListSep::Space => " ",
                    ListSep::Slash => " / ",
                };
                let parts = items
                    .iter()
                    .map(|i| self.render_css(i))
                    .collect::<Ev<Vec<_>>>()?;
                Ok(parts.join(join))
            }
            Expr::FunctionCall(call) => {
                let passthrough = call.namespace.is_none()
                    && !crate::builtins::contains(None, &call.name)
                    && !self.functions.contains_key(&call.name)
                    && !is_meta_name(&call.name);
                if passthrough {
                    self.render_css_call(call)
                } else {
                    Ok(css_arg(self.eval_call(call)?))
                }
            }
            other => Ok(css_arg(self.eval_expr(other)?)),
        }
    }

    /// Render a whole passthrough call `name(arg, …)` from its argument AST.
    pub(crate) fn render_css_call(&mut self, call: &FunctionCall) -> Ev<String> {
        let mut parts = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            let text = match arg {
                Arg::Positional(e) | Arg::Splat(e) => self.render_css(e)?,
                Arg::Named { name, value } => format!("({name}: {})", self.render_css(value)?),
            };
            parts.push(text);
        }
        Ok(format!("{}({})", call.name, parts.join(", ")))
    }

    /// Resolve `expr` to a Sass number only when it is a *pure numeric leaf* — a
    /// numeric literal or a variable bound to a number. Returns `None` for any
    /// compound/functional node, so callers keep it as literal CSS. `Err` only
    /// when a variable is undefined.
    fn try_number(&self, expr: &Expr) -> Ev<Option<SassValue>> {
        match expr {
            Expr::Literal(Literal::Number { value, unit }) => Ok(Some(SassValue::Number {
                value: *value,
                unit: unit.clone(),
            })),
            Expr::Variable(name, _) => match self.env.get(name) {
                Some(v @ SassValue::Number { .. }) => Ok(Some(v.clone())),
                Some(_) => Ok(None),
                None => Err(err(
                    ErrorKind::UndefinedVariable,
                    format!("Undefined variable ${name}."),
                )),
            },
            _ => Ok(None),
        }
    }
}

// ─── Numeric arithmetic with unit conversion ─────────────────────────────────

fn arith(op: BinOp, l: SassValue, r: SassValue) -> Ee<SassValue> {
    let (SassValue::Number { value: a, unit: ua }, SassValue::Number { value: b, unit: ub }) =
        (l, r)
    else {
        return Err(err(ErrorKind::Eval, "arithmetic operands must be numbers"));
    };
    let ua = ua.as_deref();
    let ub = ub.as_deref();
    let value = match op {
        BinOp::Add => a + convert(b, ub, ua)?,
        BinOp::Sub => a - convert(b, ub, ua)?,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0.0 {
                return Err(err(ErrorKind::Eval, "division by zero"));
            }
            a / b
        }
        _ => unreachable!("arith on non-arithmetic op"),
    };
    let unit = match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul => ua.or(ub).map(str::to_string),
        BinOp::Div => {
            if ua == ub {
                None
            } else {
                ua.map(str::to_string)
            }
        }
        _ => None,
    };
    Ok(SassValue::Number { value, unit })
}

/// Convert `value` in unit `from` into `to` (no-op when equal or one is unitless).
fn convert(value: f64, from: Option<&str>, to: Option<&str>) -> Ee<f64> {
    if from == to || from.is_none() || to.is_none() {
        return Ok(value);
    }
    convert_number(value, from, to).ok_or_else(|| {
        err(
            ErrorKind::Eval,
            format!("incompatible units {from:?} and {to:?}"),
        )
    })
}

fn compare(op: BinOp, l: SassValue, r: SassValue) -> Ee<SassValue> {
    let (ln, rn) = (l.as_number(), r.as_number());
    if let (Some((a, ua)), Some((b, ub))) = (ln, rn) {
        let b = convert(b, ub, ua)?;
        let ordering = a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
        return Ok(SassValue::Bool(ord(&op, ordering)));
    }
    // Non-numeric ordering: compare by CSS text.
    let (x, y) = (l.to_css(), r.to_css());
    Ok(SassValue::Bool(ord(&op, x.cmp(&y))))
}

fn ord(op: &BinOp, o: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match op {
        BinOp::Lt => o == Less,
        BinOp::Le => o != Greater,
        BinOp::Gt => o == Greater,
        BinOp::Ge => o != Less,
        _ => false,
    }
}

fn values_equal(l: &SassValue, r: &SassValue) -> bool {
    if let (SassValue::Number { value: a, unit: ua }, SassValue::Number { value: b, unit: ub }) =
        (l, r)
    {
        return match convert(*b, ub.as_deref(), ua.as_deref()) {
            Ok(b2) => a == &b2,
            Err(_) => false,
        };
    }
    l == r
}

fn negate(v: SassValue) -> Ee<SassValue> {
    match v {
        SassValue::Number { value, unit } => Ok(SassValue::Number {
            value: -value,
            unit,
        }),
        _ => Err(err(ErrorKind::Eval, "cannot negate a non-number")),
    }
}

// ─── Literals & interpolation helpers ────────────────────────────────────────

/// Serialize a value appearing as an argument inside a plain-CSS passthrough
/// call (`calc`/`var`/…). A Sass `null` renders as the empty string (dart-sass
/// drops it, e.g. an absent `map.get` fallback in `var(--x, )`); every other
/// value keeps its normal CSS form, so quoted strings stay quoted for `url(…)`.
fn css_arg(v: SassValue) -> String {
    match v {
        SassValue::Null => String::new(),
        other => other.to_css(),
    }
}

fn eval_literal(l: &Literal) -> SassValue {
    match l {
        Literal::Number { value, unit } => SassValue::Number {
            value: *value,
            unit: unit.clone(),
        },
        Literal::Str { text, quote } => SassValue::Str {
            text: text.clone(),
            quoted: matches!(quote, Quote::Double | Quote::Single),
        },
        Literal::Color(raw) => SassValue::Color(parse_hex(raw)),
        Literal::Url(raw) => SassValue::Str {
            text: raw.clone(),
            quoted: false,
        },
        Literal::Bool(b) => SassValue::Bool(*b),
        Literal::Null => SassValue::Null,
        Literal::Ident(name) => SassValue::Str {
            text: name.clone(),
            quoted: false,
        },
    }
}

/// Parse an `#rgb` / `#rrggbb` hex color into RGBA (fallback: black on malformed).
fn parse_hex(raw: &str) -> Color {
    let hex = raw.trim_start_matches('#');
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        3 if !hex.is_empty() => {
            let chars: Vec<char> = hex.chars().collect();
            let expand = |c: char| byte(&format!("{c}{c}")).unwrap_or(0) as f64;
            Color {
                r: expand(chars[0]),
                g: expand(chars[1]),
                b: expand(chars[2]),
                a: 1.0,
                hex: true,
            }
        }
        6 => {
            let at = |i: usize| byte(&hex[i..i + 2]).unwrap_or(0) as f64;
            Color {
                r: at(0),
                g: at(2),
                b: at(4),
                a: 1.0,
                hex: true,
            }
        }
        8 => {
            let at = |i: usize| byte(&hex[i..i + 2]).unwrap_or(0) as f64;
            Color {
                r: at(0),
                g: at(2),
                b: at(4),
                a: at(6) / 255.0,
                hex: true,
            }
        }
        _ => Color::rgb(0, 0, 0),
    }
}

fn err(kind: ErrorKind, message: impl Into<String>) -> SassError {
    SassError::new(kind, message)
}

/// The CSS source text of a binary operator, preserved literally when rendering
/// inside a `calc`/`var` passthrough (Sass math is disabled in that context).
fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div | BinOp::IntDiv => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::Assign => "=",
    }
}

/// Whether a binary operator is rendered with surrounding spaces inside a
/// plain-CSS passthrough (arithmetic / logic) or tight (assignment / comparison).
fn spaced_op(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::IntDiv
            | BinOp::Rem
            | BinOp::And
            | BinOp::Or
    )
}
