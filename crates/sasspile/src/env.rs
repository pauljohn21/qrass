//! Env — the **owned** lexical environment carried inside the exec accumulator (design §1:
//! `Acc = { scopes, env, chain_taken, order }`). It is plain data moved through `scan_map`,
//! never shared mutable state behind a handle: the accumulator owns it, and expression
//! evaluation only ever borrows it.
//!
//! P1 keeps one flat layer, which is what a top-level `$var: value` binding needs. Block
//! scoping, `!global`, `!default` and the dynamic scope of `@mixin`/`@function` bodies are
//! `directives`-phase work (tasks 3.6/4.1) and will grow this type into a scope stack; the
//! lookup key is fixed now so those phases cannot drift: a name **without** its `$`.

use std::collections::HashMap;

use crate::value::Value;
use crate::{ErrorKind, SassError};

#[derive(Debug, Clone, Default)]
pub struct Env {
    vars: HashMap<String, Value>,
}

impl Env {
    /// Bind `$name: value` at the current layer (later phases push a layer first). The env is
    /// reached only through the `State` value that the stream carries, so this mutation is of
    /// data already owned by that value — never of state shared behind a handle.
    pub fn bind(&mut self, name: &str, value: Value) {
        self.vars.insert(name.to_string(), value);
    }

    /// Look a variable up. An unbound name is typed as the Sass **undefined variable** error so the
    /// `Exec` phase can decide — by kind, against the `FATAL` set in the crate root — whether it
    /// aborts the stream or the declaration keeps its verbatim text. The corpus wants
    /// `Error: Undefined variable.` (`directives/use/error/member/missing`); literal contexts must not
    /// look here at all (P0′.3b).
    pub fn get(&self, name: &str) -> Result<Value, SassError> {
        self.vars.get(name).cloned().ok_or_else(|| {
            SassError::typed(
                ErrorKind::UndefinedVariable,
                format!("Undefined variable: \"${name}\"."),
            )
        })
    }
}
