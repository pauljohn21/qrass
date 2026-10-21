//! Expression evaluation (task 5.3 + 7.2): fold an owned [`Expr`] against the
//! live [`Env`] into an owned [`SassValue`]. Tree-walking lives on [`EvalState`]
//! (a method) so a `FunctionCall` can enter a scope, run a `@function` body and
//! capture `@return` — arithmetic/comparison helpers stay pure over values.

use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep, Literal, UnaryOp};
use crate::error::{ErrorKind, SassError};
use crate::token::{Piece, Quote};
use crate::value::{Color, SassValue};

use super::eval_arith::{arith, compare, negate, values_equal};
use super::eval_function::is_meta_name;

use crate::ast::Stmt;

use super::{Ev, EvalState};

pub(crate) type Ee<T> = Result<T, SassError>;

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
            Expr::Variable(name, _) => {
                // A name two different origin files both export through
                // `as *` is rejected at the reference, not silently last-won
                // (`directives/use/error/member` `variable/*`; same values
                // still conflict, one upstream forwarded twice does not).
                if self.wildcard_conflicts.0.iter().any(|c| c == name) {
                    return Err(err(
                        ErrorKind::UndefinedVariable,
                        "This variable is available from multiple global modules.",
                    ));
                }
                // Live module-var cell of the *defining* module wins over the
                // snapshot env (namespaced writes mutate it, M4 slice 3).
                if let Some(v) = self.member_store.get(&(self.env_owner.clone(), name.clone())) {
                    return Ok(v.clone());
                }
                self.env.get(name).cloned().ok_or_else(|| {
                    // Every golden spells this message bare — `Error: Undefined
                    // variable.` — the name being carried by the annotated source
                    // line rather than the sentence.
                    err(ErrorKind::UndefinedVariable, "Undefined variable.")
                })
            }
            Expr::Member {
                namespace, name, ..
            } => {
                let bound = self.env.has_namespace(namespace);
                // Live cell first: a namespaced write elsewhere mutated the
                // defining module's copy (M4 slice 3).
                let live = self
                    .ns_origins
                    .get(&(namespace.clone(), name.clone()))
                    .and_then(|o| self.member_store.get(&(o.clone(), name.clone())).cloned());
                live
                    .or_else(|| self.env.get_member(namespace, name).cloned())
                    .or_else(|| crate::builtins::module_const(namespace, name))
                    .ok_or_else(|| {
                        // Like the plain-variable case above the name lives in the
                        // annotated line, but a member read off a namespace no
                        // `@use` bound is called out as such.
                        err(
                            ErrorKind::UndefinedVariable,
                            if bound {
                                "Undefined variable.".to_string()
                            } else {
                                format!("There is no module with namespace \"{namespace}\".")
                            },
                        )
                    })
            }
            Expr::Paren(inner, _) => self.eval_expr(inner),
            Expr::List { items, sep, bracketed, .. } => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.eval_expr(item)?);
                }
                Ok(SassValue::List {
                    items: out,
                    sep: *sep,
                    bracketed: *bracketed,
                    keywords: None,
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
                    quoted: quote.is_some(),
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
            Expr::IfCall(call) => self.eval_if(call),
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
                // Inside a passthrough call the goldens keep `+`/`-` (and any
                // operand touching a `var()`/function) as literal CSS, but it
                // *does* run Sass arithmetic for `*`/`/` when both sides are
                // plain Sass numbers — e.g. `calc(… - $border-width * 2)` →
                // `calc(… - 2px)`. Detect that narrow case and fold it.
                if matches!(op, BinOp::Mul | BinOp::Div)
                    && let (Some(lv), Some(rv)) = (self.try_number(left)?, self.try_number(right)?)
                    && let (SassValue::Number { .. }, SassValue::Number { .. }) = (&lv, &rv)
                {
                    return Ok(arith(*op, lv, rv)?.to_css());
                }
                let l = self.render_css(left)?;
                let r = self.render_css(right)?;
                let sym = op_symbol(*op);
                // Sass math keeps the goldens' spaced form (`100% - 32px`); an
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
            Expr::List { items, sep, bracketed, .. } => {
                let join = match sep {
                    ListSep::Comma => ", ",
                    ListSep::Space => " ",
                    ListSep::Slash => " / ",
                };
                let parts = items
                    .iter()
                    .map(|i| self.render_css(i))
                    .collect::<Ev<Vec<_>>>()?;
                let inner = parts.join(join);
                Ok(if *bracketed {
                    format!("[{inner}]")
                } else {
                    inner
                })
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
        // A `url($var)` argument makes dart parse the call as an *ordinary* Sass
        // function (resolving the variable and keeping the name verbatim, e.g.
        // `-E-URL($a)` → `-E-URL(b)`) rather than a raw url. A literal or
        // `#{…}`-interpolated argument keeps the special `url` form.
        let url_is_call = call.args.iter().any(|arg| {
            matches!(arg, Arg::Positional(Expr::Variable(..)) | Arg::Splat(Expr::Variable(..)))
        });
        let mut parts = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            let text = match arg {
                Arg::Positional(e) | Arg::Splat(e) => self.render_css(e)?,
                Arg::Named { name, value } => format!("({name}: {})", self.render_css(value)?),
            };
            parts.push(text);
        }
        Ok(format!(
            "{}({})",
            normalize_special_name(&call.name, url_is_call),
            parts.join(", ")
        ))
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
                None => Err(err(ErrorKind::UndefinedVariable, "Undefined variable.")),
            },
            _ => Ok(None),
        }
    }
}

// ─── Literals & interpolation helpers ────────────────────────────────────────

/// dart lowercases the emitted name of every *special* plain-CSS function
/// (`calc`/`url`/`element`/`expression`/`progid:…`, optionally vendor-prefixed
/// and in any case) and, uniquely for `url`, also strips the vendor prefix
/// (`-C-URL(…)` → `url(…)`). `url` is skipped (name kept verbatim) when
/// `url_is_call` marks a variable argument, which makes dart treat it as an
/// ordinary function. Any other passthrough call name is left verbatim, so
/// non-special unknown functions keep their source spelling.
fn normalize_special_name(name: &str, url_is_call: bool) -> String {
    let lower = name.to_ascii_lowercase();
    // A leading vendor prefix is `[-_a-zA-Z0-9]+-`; the base word follows the
    // last `-` of that run. Names without a vendor dash use the whole string.
    let (prefix_len, base) = match lower.rfind('-') {
        Some(dash) if dash + 1 < lower.len() => (dash + 1, &lower[dash + 1..]),
        _ => (0, lower.as_str()),
    };
    // Only treat it as a known special function when the prefix is a clean
    // vendor run (all word/dash chars) so e.g. `data-url` is not mis-normalized.
    let vendor_ok = lower[..prefix_len]
        .bytes()
        .all(|b| b == b'-' || b == b'_' || (b as char).is_ascii_alphanumeric());
    if !vendor_ok {
        return name.to_string();
    }
    if base == "url" {
        if url_is_call {
            name.to_string()
        } else {
            "url".to_string()
        }
    } else if matches!(base, "calc" | "element" | "expression") || base.starts_with("progid:") {
        lower
    } else {
        name.to_string()
    }
}

/// Serialize a value appearing as an argument inside a plain-CSS passthrough
/// call (`calc`/`var`/…). A Sass `null` renders as the empty string (the goldens
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
        Literal::NamedColor { hex, name } => SassValue::Color(Color::named(hex, name)),
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
                c4: None,
                name: None,
                legacy: None,
                legacy_hsl: None,
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
                c4: None,
                name: None,
                legacy: None,
                legacy_hsl: None,
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
                c4: None,
                name: None,
                legacy: None,
                legacy_hsl: None,
            }
        }
        _ => Color::rgb(0, 0, 0),
    }
}

pub(crate) fn err(kind: ErrorKind, message: impl Into<String>) -> SassError {
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
