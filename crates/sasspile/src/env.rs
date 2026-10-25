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

use crate::SassError;
use crate::value::Value;

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

    /// Look a variable up; an unknown name is the Sass error, and the caller decides whether
    /// that fails the stream or leaves the declaration value untouched.
    pub fn get(&self, name: &str) -> Result<Value, SassError> {
        self.vars.get(name).cloned().ok_or_else(|| {
            SassError::new(format!("Undefined variable: \"${name}\"."))
        })
    }
}
