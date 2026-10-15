//! Built-in function library (task group 8, design D6).
//!
//! Two dispatch tables share one implementation per function:
//! * `namespaces` maps a module (`"map"`, `"math"`, …) to its functions, reached
//!   as `map.get(...)`;
//! * `global` holds bare/legacy names — both plain globals (`nth`, `type-of`)
//!   and the hyphen-joined alias for every namespaced function (`map-get`), so
//!   `map.get` and `map-get` resolve to the *same* entry.
//!
//! A builtin is a pure `fn(&Call) -> Result<SassValue, SassError>`: it captures
//! no environment (variable visibility is the call site's job, per D0 guardrail
//! 2). The tables are built once and read through a process-wide `OnceLock` —
//! immutable after init, so this is read-only sharing, not shared mutability.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

pub(crate) mod color;
pub(crate) mod global;
pub(crate) mod list;
pub(crate) mod map;
pub(crate) mod math;
pub(crate) mod meta;
pub(crate) mod selector;
pub(crate) mod string;

/// Result alias for builtin fallibility (errors ride the value channel).
pub(crate) type Or<T> = Result<T, SassError>;

/// A resolved argument list: evaluated positional values plus a named-argument
/// map keyed without the leading `$` (`$lightness: 10%` → `"lightness"`).
pub(crate) struct Call<'a> {
    pub positional: &'a [SassValue],
    pub named: &'a HashMap<String, SassValue>,
}

type BuiltinFn = fn(&Call) -> Or<SassValue>;

/// One registered function: its implementation plus positional arity bounds
/// (`max == None` means variadic, e.g. `math.min($numbers...)`).
#[derive(Clone, Copy)]
struct Builtin {
    min: usize,
    max: Option<usize>,
    call: BuiltinFn,
}

/// The double dispatch tables (D6).
#[derive(Default)]
pub(crate) struct Registry {
    namespaces: HashMap<&'static str, HashMap<String, Builtin>>,
    global: HashMap<String, Builtin>,
}

impl Registry {
    /// Register a global/legacy function (also reachable by its bare name).
    fn def(&mut self, name: &str, min: usize, max: Option<usize>, call: BuiltinFn) {
        self.global
            .insert(name.to_string(), Builtin { min, max, call });
    }

    /// Register a namespaced function `ns.name`, additionally exposing the
    /// legacy hyphen alias `ns-name` in the global table (both → one impl).
    fn ns(
        &mut self,
        namespace: &'static str,
        name: &str,
        min: usize,
        max: Option<usize>,
        call: BuiltinFn,
    ) {
        let b = Builtin { min, max, call };
        self.namespaces
            .entry(namespace)
            .or_default()
            .insert(name.to_string(), b);
        self.global.insert(format!("{namespace}-{name}"), b);
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// The process-wide built once, read thereafter.
fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let mut r = Registry::default();
        map::register(&mut r);
        math::register(&mut r);
        color::register(&mut r);
        string::register(&mut r);
        list::register(&mut r);
        selector::register(&mut r);
        meta::register(&mut r);
        global::register(&mut r);
        r
    })
}

/// Look up a builtin by optional namespace and name. `map.get` → namespace
/// `"map"`/name `"get"`; a bare call → namespace `None`. Returns `None` when no
/// such builtin exists (so the caller raises "Undefined function").
fn lookup(namespace: Option<&str>, name: &str) -> Option<&'static Builtin> {
    match namespace {
        Some(ns) => registry().namespaces.get(ns)?.get(name),
        None => registry().global.get(name),
    }
}

/// True when a builtin is registered under `namespace`/`name` — used by
/// `function-exists` without leaking the private `Builtin` type.
pub(crate) fn contains(namespace: Option<&str>, name: &str) -> bool {
    lookup(namespace, name).is_some()
}

/// Built-in module *constants* (task 8.2): `math.$pi`, `math.$e`, … resolved when
/// a namespaced member lookup misses. These are the variables a `@use "sass:math"`
/// exposes without a source file; only the `math` module defines any today.
pub(crate) fn module_const(namespace: &str, name: &str) -> Option<SassValue> {
    if namespace != "math" {
        return None;
    }
    let value = match name {
        "pi" => std::f64::consts::PI,
        "e" => std::f64::consts::E,
        "epsilon" => f64::EPSILON,
        "max-safe-integer" => 9_007_199_254_740_991.0,
        "min-safe-integer" => -9_007_199_254_740_991.0,
        _ => return None,
    };
    Some(SassValue::Number { value, unit: None })
}

/// Invoke a builtin if registered, after an arity check. `Ok(...)` carries the
/// call result (or an arity/eval error); `None` means "not a builtin" so
/// `eval_call` can fall through to the undefined-function error.
pub(crate) fn call(
    namespace: Option<&str>,
    name: &str,
    positional: &[SassValue],
    named: &HashMap<String, SassValue>,
) -> Option<Or<SassValue>> {
    let display = match namespace {
        Some(ns) => format!("{ns}.{name}"),
        None => name.to_string(),
    };
    let b = lookup(namespace, name)?;
    if positional.len() < b.min {
        return Some(Err(arity(&display, positional.len(), b.min, b.max)));
    }
    if let Some(max) = b.max {
        if positional.len() > max {
            return Some(Err(arity(&display, positional.len(), b.min, b.max)));
        }
    }
    Some((b.call)(&Call { positional, named }))
}

/// Build the sass-shaped "wrong number of arguments" message.
fn arity(name: &str, given: usize, min: usize, max: Option<usize>) -> SassError {
    let expected = match max {
        None => format!("{min} or more"),
        Some(max) if max == min => format!("{max}"),
        Some(max) => format!("{min} to {max}"),
    };
    SassError::new(
        ErrorKind::Builtins,
        format!("wrong number of arguments ({given} for {expected}) to \"{name}\""),
    )
}

// ─── Shared argument helpers (used by the domain modules) ──────────────────────

impl<'a> Call<'a> {
    /// A named argument (by `$`-less key) if present, else the positional
    /// fallback at `pos`, else `default`.
    pub(crate) fn opt(&self, key: &str, pos: Option<usize>) -> Option<&'a SassValue> {
        if let Some(v) = self.named.get(key) {
            return Some(v);
        }
        pos.and_then(|p| self.positional.get(p))
    }
}

/// Report a value of the wrong type for `fn_name`'s `$what` argument.
pub(crate) fn type_err(fn_name: &str, what: &str, expected: &str, v: &SassValue) -> SassError {
    SassError::new(
        ErrorKind::Builtins,
        format!(
            "\"{fn_name}\": ${what} must be {expected} (given {})",
            v.type_name()
        ),
    )
}
