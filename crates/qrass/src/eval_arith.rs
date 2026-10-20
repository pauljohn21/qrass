//! Numeric arithmetic with unit conversion (task 5.3): pure helpers over owned
//! [`SassValue`]s, split out of [`eval_expr`] so the tree-walking methods stay
//! focused on scope entry and `@return` capture.

use crate::ast_expr::BinOp;
use crate::error::ErrorKind;
use crate::value::{SassValue, convert_number, number_to_css};

use super::eval_expr::{Ee, err};

pub(crate) fn arith(op: BinOp, l: SassValue, r: SassValue) -> Ee<SassValue> {
    // A callable operand is not a CSS value at all: the goldens spell the
    // *left* value's serialization (`values/mixins.hrx` addition, and
    // `get_function/error` division both read `get-*("…") isn't a valid CSS
    // value.`), falling through to the existing wording otherwise.
    if matches!(l, SassValue::Function(_) | SassValue::Mixin(_)) {
        return Err(err(ErrorKind::Eval, format!("{} isn't a valid CSS value.", l.to_css())));
    }
    if matches!(r, SassValue::Function(_) | SassValue::Mixin(_)) {
        return Err(err(ErrorKind::Eval, format!("{} isn't a valid CSS value.", l.to_css())));
    }
    let (SassValue::Number { value: a, unit: ua }, SassValue::Number { value: b, unit: ub }) =
        (l, r)
    else {
        return Err(err(ErrorKind::Eval, "arithmetic operands must be numbers"));
    };
    let ua = ua.as_deref();
    let ub = ub.as_deref();
    let (lhs, rhs) = (number_to_css(a, ua), number_to_css(b, ub));
    let value = match op {
        BinOp::Add => a + convert(b, ub, ua, &lhs, &rhs)?,
        BinOp::Sub => a - convert(b, ub, ua, &lhs, &rhs)?,
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
/// `lhs`/`rhs` are the two operands as the goldens spell them in the failure
/// sentence — `Error: 1px and 1em have incompatible units.` names the operands in
/// source order, not the unit names (`invalid_expression/error` of
/// `directives/forward/error/with.hrx`).
fn convert(value: f64, from: Option<&str>, to: Option<&str>, lhs: &str, rhs: &str) -> Ee<f64> {
    if from == to || from.is_none() || to.is_none() {
        return Ok(value);
    }
    convert_number(value, from, to).ok_or_else(|| {
        err(
            ErrorKind::Eval,
            format!("{lhs} and {rhs} have incompatible units."),
        )
    })
}

pub(crate) fn compare(op: BinOp, l: SassValue, r: SassValue) -> Ee<SassValue> {
    let (ln, rn) = (l.as_number(), r.as_number());
    if let (Some((a, ua)), Some((b, ub))) = (ln, rn) {
        let (lhs, rhs) = (number_to_css(a, ua), number_to_css(b, ub));
        let b = convert(b, ub, ua, &lhs, &rhs)?;
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

pub(crate) fn values_equal(l: &SassValue, r: &SassValue) -> bool {
    if let (SassValue::Number { value: a, unit: ua }, SassValue::Number { value: b, unit: ub }) =
        (l, r)
    {
        return match convert(
            *b,
            ub.as_deref(),
            ua.as_deref(),
            &number_to_css(*a, ua.as_deref()),
            &number_to_css(*b, ub.as_deref()),
        ) {
            Ok(b2) => a == &b2,
            Err(_) => false,
        };
    }
    l == r
}

pub(crate) fn negate(v: SassValue) -> Ee<SassValue> {
    match v {
        SassValue::Number { value, unit } => Ok(SassValue::Number {
            value: -value,
            unit,
        }),
        _ => Err(err(ErrorKind::Eval, "cannot negate a non-number")),
    }
}
