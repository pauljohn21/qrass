//! Sass `math.*` builtins (plus the bare global aliases Sass still exposes). Unit algebra
//! for `-`/`+`/`*`/`/` is handled by `arith`; these are the named entry points.

use super::{map_num, reduce_vals, unitless};
use crate::expr::{Op, arith, compare, num, Cmp};
use crate::value::Value;

/// `Some` when this domain owns `name`.
pub fn call(name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match (name, args) {
        ("math.div" | "divide", [a, b]) => Some(arith(Op::Div, a.clone(), b.clone())),
        ("abs" | "math.abs", [a]) => Some(num(a).map(|(n, u)| Value::Number(n.abs(), u))),
        ("math.round" | "round", [a]) => Some(map_num(a, |n| (n + 0.5).floor())),
        ("math.ceil" | "ceil", [a]) => Some(map_num(a, f64::ceil)),
        ("math.floor" | "floor", [a]) => Some(map_num(a, f64::floor)),
        ("math.sqrt", [a]) => Some(unitless(a).map(|n| Value::Number(n.sqrt(), String::new()))),
        ("math.log", [a]) => Some(unitless(a).map(|n| Value::Number(n.ln(), String::new()))),
        ("math.pow", [x, y]) => {
            Some(match (unitless(x), unitless(y)) {
                (Ok(a), Ok(b)) => Ok(Value::Number(a.powf(b), String::new())),
                (Err(e), _) | (_, Err(e)) => Err(e),
            })
        }
        ("min" | "math.min", vs) => Some(reduce_vals(vs, least)),
        ("max" | "math.max", vs) => Some(reduce_vals(vs, greatest)),
        _ => None,
    }
}

fn is_lt(a: &Value, b: &Value) -> bool {
    matches!(compare(a.clone(), Cmp::Lt, b.clone()), Ok(Value::Bool(true)))
}
fn least(a: Value, b: Value) -> Value {
    if is_lt(&a, &b) { a } else { b }
}
fn greatest(a: Value, b: Value) -> Value {
    if is_lt(&a, &b) { b } else { a }
}
