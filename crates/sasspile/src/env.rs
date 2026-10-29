//! Env — the **owned** lexical environment the `Exec` kernel carries. It is one half of
//! [`crate::exec::State`] (`{ scopes, env }`), which lives inside the kernel's observer and moves forward
//! through `&mut self`; it is never shared mutable state behind a handle, and expression evaluation only
//! ever borrows it.
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
    /// aborts the stream or the declaration keeps its verbatim text.
    ///
    /// The message is the corpus's **oracle header**, `Error: Undefined variable.`
    /// (`directives/use/error/member/missing`, `css/comment.hrx::error/loud/interpolation/failure` — all
    /// 23 fixtures that name this error spell it without the variable). The name is not part of the
    /// header: in Sass it is read off the *source span* under the header, which P7 7.2 adds. Literal
    /// contexts must not look here at all (P0′.3b).
    pub fn get(&self, name: &str) -> Result<Value, SassError> {
        self.vars
            .get(name)
            .cloned()
            .ok_or_else(|| SassError::typed(ErrorKind::UndefinedVariable, "Undefined variable."))
    }
}
