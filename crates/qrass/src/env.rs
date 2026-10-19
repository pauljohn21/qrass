//! Lexical scope chain for eval (task 5.2). Scopes live in an owned arena
//! ([`Env`]); a scope is addressed by index, and each records its parent index —
//! no `Arc<Mutex<_>>`, no reference cycles. The `Env` value is moved by value
//! through the `scan_map` eval fold, entering/leaving scopes as the traversal
//! descends rulesets and mixin bodies.
//!
//! Variable rules follow modern module semantics (design D3):
//! - a plain assignment updates the innermost **local** (non-global) scope that
//!   already declares the name (so loop/mixin accumulators persist), shadowing only
//!   when the name is new or lives solely in the global scope;
//! - `!global` writes through to the root (module-global) scope;
//! - `!default` binds only when the name resolves to nothing across the chain.

use std::collections::HashMap;

use crate::value::SassValue;

/// Handle to a scope in the arena.
pub type ScopeId = usize;

#[derive(Clone, Debug)]
struct Scope {
    vars: HashMap<String, SassValue>,
    parent: Option<ScopeId>,
}

/// An owned scope arena. The root scope (id 0) is the module-global scope.
#[derive(Clone, Debug)]
pub struct Env {
    scopes: Vec<Scope>,
    current: ScopeId,
    /// Module namespaces bound by `@use … as ns`: `ns -> {var -> value}`.
    /// `as *` merges into the global scope instead of here.
    namespaces: HashMap<String, HashMap<String, SassValue>>,
}

/// Flags that modify an assignment.
#[derive(Clone, Copy, Debug, Default)]
pub struct AssignFlags {
    pub global: bool,
    pub default: bool,
}

impl Env {
    /// A fresh environment with only the global scope.
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope {
                vars: HashMap::new(),
                parent: None,
            }],
            current: 0,
            namespaces: HashMap::new(),
        }
    }

    /// Enter a child scope whose parent is the current one; returns the id to
    /// restore on [`Env::leave`].
    pub fn enter(&mut self) -> ScopeId {
        let id = self.scopes.len();
        self.scopes.push(Scope {
            vars: HashMap::new(),
            parent: Some(self.current),
        });
        self.current = id;
        id
    }

    /// Leave the current scope, restoring its parent. The root is never popped.
    pub fn leave(&mut self) {
        if let Some(parent) = self.scopes[self.current].parent {
            self.current = parent;
        }
    }

    /// The scope that `!global` bindings write to.
    pub fn global_scope(&self) -> ScopeId {
        0
    }

    /// The active scope id. Captured at an `@include` site so `@content` can be
    /// run back in that lexical scope (task 7.1).
    pub fn current_scope(&self) -> ScopeId {
        self.current
    }

    /// Make `id` the active scope, returning the previous one. Lets `@content`
    /// suspend the mixin-body scope and execute in the include site's scope.
    pub fn set_scope(&mut self, id: ScopeId) -> ScopeId {
        std::mem::replace(&mut self.current, id)
    }

    /// Resolve `name` from the current scope up the parent chain.
    pub fn get(&self, name: &str) -> Option<&SassValue> {
        let mut cur = Some(self.current);
        while let Some(id) = cur {
            if let Some(v) = self.scopes[id].vars.get(name) {
                return Some(v);
            }
            cur = self.scopes[id].parent;
        }
        None
    }

    /// True if `name` is bound in the current scope only (no parent search).
    pub fn defined_here(&self, name: &str) -> bool {
        self.scopes[self.current].vars.contains_key(name)
    }

    /// True if `name` is bound in the module-global (root) scope — `global-variable-exists`.
    pub fn defined_global(&self, name: &str) -> bool {
        self.scopes[0].vars.contains_key(name)
    }

    /// Bind `name` per [`AssignFlags`]. Returns whether a binding was written.
    pub fn define(&mut self, name: &str, value: SassValue, flags: AssignFlags) -> bool {
        if flags.default && self.get(name).is_some() {
            return false; // already defined anywhere → `!default` is a no-op
        }
        let target = if flags.global {
            self.global_scope()
        } else {
            self.current
        };
        self.scopes[target].vars.insert(name.to_string(), value);
        true
    }

    /// Reassign a plain (non-`!global`) assignment following golden scoping: it
    /// updates the existing binding in the innermost *local* (non-global) scope that
    /// already declares it — so a loop/mixin accumulator like `$n: $n + …` persists —
    /// but a name found only in the module-global scope is *shadowed* locally instead
    /// of mutated (that requires `!global`). When declared nowhere, it binds here.
    pub fn set_existing_or_local(&mut self, name: &str, value: SassValue) {
        let mut cur = Some(self.current);
        while let Some(id) = cur {
            if id == 0 {
                break; // never write through to the global scope from a plain assignment
            }
            if self.scopes[id].vars.contains_key(name) {
                self.scopes[id].vars.insert(name.to_string(), value);
                return;
            }
            cur = self.scopes[id].parent;
        }
        self.scopes[self.current]
            .vars
            .insert(name.to_string(), value);
    }

    /// A snapshot of the global scope's bindings (used to publish a module's
    /// exported variables after instantiation, task group 6).
    pub fn global_bindings(&self) -> Vec<(String, SassValue)> {
        self.scopes[0]
            .vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Resolve a namespaced member `ns.$name` (task 6.2).
    pub fn get_member(&self, namespace: &str, name: &str) -> Option<&SassValue> {
        self.namespaces.get(namespace).and_then(|m| m.get(name))
    }

    /// True when `ns` names a module some `@use` has loaded here. A missing
    /// namespace is reported as its own error rather than as an undefined
    /// member (`directives/forward/error/with.hrx`
    /// `invalid_expression/module_loaded_later`).
    pub fn has_namespace(&self, namespace: &str) -> bool {
        self.namespaces.contains_key(namespace)
    }

    /// The whole variable map published under `ns` (meta.module-variables).
    pub fn namespace_vars(&self, namespace: &str) -> Option<&HashMap<String, SassValue>> {
        self.namespaces.get(namespace)
    }

    /// Publish a module's exported variables under `ns`.
    pub fn bind_namespace(&mut self, namespace: &str, vars: Vec<(String, SassValue)>) {
        self.namespaces
            .insert(namespace.to_string(), vars.into_iter().collect());
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
