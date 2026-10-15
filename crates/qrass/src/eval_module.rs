//! Module loading for [`EvalState`] (tasks 6.2 / 6.3): `@use` and `@forward`.
//!
//! Both canonicalize a URL against the importer's directory and instantiate each
//! canonical path exactly once in its own global scope, publishing the module's
//! CSS at that first load. `@use` binds the exports under a namespace; `@forward`
//! re-exports them (with `show`/`hide`/`as prefix*`) into the current module's
//! export set. `@use … with` / `@forward … with` configures the target's
//! `!default` variables; a supplied-but-unconsumed name is an error (design D3).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ast::{Forward, Use, UseAlias};
use crate::env::AssignFlags;
use crate::error::{ErrorKind, SassError};
use crate::loader::{SourceKind, canonicalize_with};
use crate::module::{Module, default_namespace, export_vars};
use crate::value::SassValue;

use super::{Ev, EvalState};

impl EvalState {
    /// Resolve and load one `@use`, emitting the module's CSS (once) and binding
    /// its namespace into the current scope.
    pub(super) fn handle_use(&mut self, u: &Use, out: &mut Vec<String>) -> Ev<()> {
        let config = self.eval_config(&u.config)?;
        let (module, _fresh) = self.load_module(&u.url, config, out)?;
        self.bind_alias(u, &module, &u.url);
        Ok(())
    }

    /// Resolve and load one `@forward`, re-exporting the target's members into
    /// the current module's export buffer.
    pub(super) fn handle_forward(&mut self, f: &Forward, out: &mut Vec<String>) -> Ev<()> {
        let config = self.eval_config(&f.config)?;
        let (module, _fresh) = self.load_module(&f.url, config, out)?;
        let members = filter_forwarded(module.vars.clone(), &f.shown, &f.hidden, &f.prefix);
        if let Some(buf) = self.forward_buf.last_mut() {
            buf.extend(members);
        }
        Ok(())
    }

    /// Evaluate `with (…)` configuration in the *importer's* environment.
    fn eval_config(
        &mut self,
        config: &[(String, crate::ast_expr::Expr)],
    ) -> Ev<Vec<(String, SassValue)>> {
        let mut out = Vec::with_capacity(config.len());
        for (n, e) in config {
            let v = self.eval_expr(e)?;
            out.push((n.clone(), v));
        }
        Ok(out)
    }

    /// Canonicalize, cycle-check, and load a module (cached ⇒ no re-emit).
    /// Returns `(module, was_freshly_instantiated)`.
    fn load_module(
        &mut self,
        url: &str,
        config: Vec<(String, SassValue)>,
        out: &mut Vec<String>,
    ) -> Ev<(Module, bool)> {
        // Built-in modules (`sass:math`, `sass:color`, …) have no source file: their
        // functions resolve through the builtin registry by namespace at call time
        // and their constants (`math.$pi`, …) through the `module_const` fallback, so
        // `@use`/`@forward` of a `sass:` URL is a no-op that binds nothing.
        if url.starts_with("sass:") {
            return Ok((Module { css: Vec::new(), vars: Vec::new() }, false));
        }
        let base = parent_dir(&self.name);
        let resolved = canonicalize_with(url, &base, &self.load_paths)?;
        let key = resolved.path.display().to_string();

        if self.stack.contains(&key) {
            let mut chain = self.stack.clone();
            chain.push(key);
            return Err(SassError::new(
                ErrorKind::Cycle,
                "It is a circular dependency: @use/@forward loops back on itself",
            )
            .note(chain.join(" -> ")));
        }

        if let Some(module) = self.modules.get(&key) {
            return Ok((module.clone(), false)); // cached: no CSS, no re-run
        }

        let module = if resolved.kind == SourceKind::Css {
            let css = read_to_string(&resolved.path, &key)?;
            Module {
                css: vec![css],
                vars: Vec::new(),
            }
        } else {
            let content = read_to_string(&resolved.path, &key)?;
            self.instantiate(&key, &content, config)?
        };

        self.modules.insert(key, module.clone());
        out.extend(module.css.iter().cloned());
        Ok((module, true))
    }

    /// Bind a module's exports under the `@use` alias.
    fn bind_alias(&mut self, u: &Use, module: &Module, url: &str) {
        match &u.alias {
            UseAlias::Wildcard => {
                for (name, value) in &module.vars {
                    self.env.define(name, value.clone(), AssignFlags::default());
                }
            }
            UseAlias::Named(ns) => self.env.bind_namespace(ns, module.vars.clone()),
            UseAlias::Default => {
                let ns = default_namespace(url);
                if !ns.is_empty() {
                    self.env.bind_namespace(&ns, module.vars.clone());
                }
            }
        }
    }

    /// Run a module body in a fresh global scope, returning its CSS + exports.
    /// Nested `@use`/`@forward` share `self.modules`/`self.stack`, so a diamond
    /// loads once. Configuration is pre-bound so the body's `!default` keeps it.
    fn instantiate(
        &mut self,
        key: &str,
        content: &str,
        config: Vec<(String, SassValue)>,
    ) -> Ev<Module> {
        let program =
            crate::lexer::lex(0, key.to_string(), content.to_string()).and_then(|tokens| {
                crate::parser::parse(0, key.to_string(), content.to_string(), tokens)
            })?;

        let outer_env = std::mem::replace(&mut self.env, Default::default());
        let outer_owner = std::mem::replace(&mut self.env_owner, key.to_string());
        let outer_file = std::mem::replace(&mut self.file, 0);
        let outer_name = std::mem::replace(&mut self.name, key.to_string());
        let outer_content = std::mem::replace(&mut self.content, content.to_string());
        self.stack.push(key.to_string());
        self.forward_buf.push(Vec::new());
        let pending: HashSet<String> = config.iter().map(|(n, _)| n.clone()).collect();
        self.pending_config.push(pending);

        for (name, value) in config {
            self.env.define(&name, value, AssignFlags::default());
        }

        let ran = (|st: &mut EvalState| -> Ev<Module> {
            let mut css = Vec::new();
            for stmt in &program {
                st.eval_top_level(stmt, &mut css)?;
            }
            if let Some(unconsumed) = st.pending_config.last() {
                if !unconsumed.is_empty() {
                    let names = unconsumed.iter().cloned().collect::<Vec<_>>().join(", ");
                    return Err(SassError::new(
                        ErrorKind::Config,
                        format!(
                            "Module was configured with ${names} but did not declare them !default"
                        ),
                    ));
                }
            }
            let forwarded = st.forward_buf.last().cloned().unwrap_or_default();
            let mut vars = export_vars(st.env.global_bindings());
            vars.extend(forwarded);
            Ok(Module { css, vars })
        })(self);

        self.pending_config.pop();
        self.forward_buf.pop();
        self.stack.pop();
        // Persist this module's now-live global env so cross-module closures defined
        // in it share (and mutate) it on later calls, rather than a definition clone.
        self.module_envs.insert(key.to_string(), self.env.clone());
        self.env = outer_env;
        self.env_owner = outer_owner;
        self.file = outer_file;
        self.name = outer_name;
        self.content = outer_content;

        ran
    }

    /// Record a `!default` binding as consuming pending configuration.
    pub(super) fn mark_default_used(&mut self, name: &str) {
        if let Some(top) = self.pending_config.last_mut() {
            top.remove(name);
        }
    }
}

/// Apply `@forward` `show`/`hide`/`as prefix*` to a module's public members.
fn filter_forwarded(
    members: Vec<(String, SassValue)>,
    shown: &[String],
    hidden: &[String],
    prefix: &str,
) -> Vec<(String, SassValue)> {
    let norm = |s: &str| s.trim_start_matches(['$', '-']).to_string();
    let mut out = members;
    if !shown.is_empty() {
        let keep: Vec<String> = shown.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| keep.contains(n));
    }
    if !hidden.is_empty() {
        let drop: Vec<String> = hidden.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| !drop.contains(n));
    }
    if !prefix.is_empty() {
        for (name, _) in &mut out {
            *name = format!("{prefix}{name}");
        }
    }
    out
}

/// Directory containing `path` (empty for a bare relative name).
fn parent_dir(path: &str) -> PathBuf {
    Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default()
}

/// Read a resolved module from disk, mapping IO failure to a load error.
fn read_to_string(path: &Path, name: &str) -> Result<String, SassError> {
    std::fs::read_to_string(path).map_err(|err| SassError::io(err, name))
}
