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
pub(crate) mod color_gamut;
pub(crate) mod color_legacy;
pub(crate) mod color_ops;
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
/// (`max == None` means variadic, e.g. `math.min($numbers...)`) and the ordered
/// parameter names. `params` lets the dispatcher fold named arguments into the
/// positional slot each name declares (`math.clamp($min: 0, …)`); an empty
/// `params` means the function is invoked positionally only.
#[derive(Clone, Copy)]
struct Builtin {
    min: usize,
    max: Option<usize>,
    call: BuiltinFn,
    params: &'static [&'static str],
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
            .insert(name.to_string(), Builtin { min, max, call, params: &[] });
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
        let b = Builtin { min, max, call, params: &[] };
        self.namespaces
            .entry(namespace)
            .or_default()
            .insert(name.to_string(), b);
        self.global.insert(format!("{namespace}-{name}"), b);
    }

    /// Like [`Registry::ns`], declaring the parameter names for named-argument
    /// folding on both the `ns.name` and `ns-name` entries.
    fn nsp(
        &mut self,
        namespace: &'static str,
        name: &str,
        min: usize,
        max: Option<usize>,
        params: &'static [&'static str],
        call: BuiltinFn,
    ) {
        let b = Builtin { min, max, call, params };
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
        color_legacy::register(&mut r);
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
    // Fold named arguments into their declared positional slots when the
    // function declares parameter names. This is skipped entirely when no named
    // arguments are passed (or the function is positional-only), so the common
    // path — and its arity semantics — is unchanged.
    let folded: Vec<SassValue> = if !b.params.is_empty() && !named.is_empty() {
        match fold_named(b, positional, named, &display) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        }
    } else {
        Vec::new()
    };
    let positional: &[SassValue] = if folded.is_empty() { positional } else { &folded };
    if positional.len() < b.min {
        return Some(Err(arity(
            &display,
            positional.len(),
            b.min,
            b.max,
            b.params,
        )));
    }
    if let Some(max) = b.max
        && positional.len() > max {
            return Some(Err(arity(
                &display,
                positional.len(),
                b.min,
                b.max,
                b.params,
            )));
        }
    Some((b.call)(&Call { positional, named }))
}

/// Fold named arguments into the positional slot each declared parameter name
/// occupies, so a call like `math.clamp($min: 0, $number: 5, $max: 10)` reaches
/// the implementation through `positional[0..3]` in *declaration* order.
///
/// Leading positional arguments fill slots first; a named argument that targets
/// an already-filled slot (or an unknown parameter) is a call error. Folding
/// stops at the first empty slot — a missing argument — leaving the downstream
/// arity check to report the shortfall.
fn fold_named(
    b: &Builtin,
    positional: &[SassValue],
    named: &HashMap<String, SassValue>,
    display: &str,
) -> Or<Vec<SassValue>> {
    let np = b.params.len();
    let mut slots: Vec<Option<SassValue>> = vec![None; np];
    // Arguments past the declared parameters are the variadic tail (only
    // reachable when `max == None`; the arity check catches the non-variadic
    // over-application later).
    let mut tail: Vec<SassValue> = Vec::new();
    for (i, v) in positional.iter().enumerate() {
        if i < np {
            slots[i] = Some(v.clone());
        } else {
            tail.push(v.clone());
        }
    }
    for (key, val) in named {
        match b.params.iter().position(|p| *p == key) {
            Some(idx) => {
                if slots[idx].is_some() {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        format!("Argument ${key} was passed both by position and by name."),
                    ));
                }
                slots[idx] = Some(val.clone());
            }
            None => {
                // A variadic function (`max == None`) accepts named arguments
                // beyond its declared parameters — the golden's `@kwargs...` channel
                // setters, e.g. `color.scale($lightness: 10%)`. Leave those for
                // the implementation to read from `Call::named`; only a
                // fixed-arity function rejects an unknown argument name.
                if b.max.is_none() {
                    continue;
                }
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("\"{display}\": No such argument \"${key}\""),
                ));
            }
        }
    }
    let mut out = Vec::with_capacity(np + tail.len());
    // Keep positional slots index-aligned for implementations that read by
    // index: fill interior gaps left by a named argument that targets a later
    // slot (e.g. `join($list1, $list2, $bracketed: false)` skips `$separator`)
    // with `Null` rather than truncating and dropping the trailing value.
    if let Some(last) = slots.iter().rposition(|s| s.is_some()) {
        for slot in slots.into_iter().take(last + 1) {
            out.push(slot.unwrap_or(SassValue::Null));
        }
    }
    out.extend(tail);
    Ok(out)
}

/// Build the sass-shaped arity error. When the shortfall/overflow names a
/// declared parameter it mirrors dart-sass exactly (`Missing argument $x.` /
/// `Only N arguments allowed, but M were passed.`); a positional-only builtin
/// (no parameter names) keeps the generic `(given for expected)` wording.
fn arity(
    name: &str,
    given: usize,
    min: usize,
    max: Option<usize>,
    params: &[&'static str],
) -> SassError {
    if given < min
        && let Some(missing) = params.get(given) {
            return SassError::new(
                ErrorKind::Builtins,
                format!("Missing argument ${missing}."),
            );
        }
    if let Some(max) = max
        && given > max {
            let arg_word = if max == 1 { "argument" } else { "arguments" };
            let pass_word = if given == 1 { "was" } else { "were" };
            return SassError::new(
                ErrorKind::Builtins,
                format!(
                    "Only {max} {arg_word} allowed, but {given} {pass_word} passed.",
                ),
            );
        }
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

