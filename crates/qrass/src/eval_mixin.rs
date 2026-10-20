//! Mixin machinery for [`EvalState`] (task 7.1): `@mixin` definition, `@include`
//! invocation with full argument binding (positional / named / default / rest
//! splat) and `@content` replay in the include site's lexical scope.
//!
//! The mixin body is executed in a child of its **defining module's** environment
//! snapshot (lexical scoping, like functions), so it sees the module's private
//! variables and `ns.$var` bindings regardless of the call site. `@content` is the
//! one exception: it swaps back to the captured include-site environment so the
//! block sees the caller's variables. Bodies expand through
//! [`EvalState::collect_stmt`], so declarations they emit fold into the enclosing
//! rule and nested rules render in place.

use std::collections::HashMap;

use crate::ast::{Content, Include, MixinDecl, Param};
use crate::ast_expr::{Arg, BinOp, Expr, ListSep, Literal};
use crate::env::AssignFlags;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::{ContentCtx, Ev, EvalState};

impl EvalState {
    /// Register a `@mixin` definition (last definition wins for the name),
    /// snapshotting the defining module's environment for lexical body resolution.
    pub(super) fn define_mixin(&mut self, m: &MixinDecl) -> Ev<()> {
        let name = self.render_interp(&m.name)?;
        self.mixin_env.insert(name.clone(), self.env.clone());
        self.mixin_module.insert(name.clone(), self.name.clone());
        self.mixins.insert(name, std::rc::Rc::new(m.clone()));
        Ok(())
    }

    /// `@include name(args) [using (…) { … }]`: bind arguments, push the content
    /// block, expand the body, then unwind.
    pub(super) fn run_include(
        &mut self,
        inc: &Include,
        sel_ctx: &[String],
        decls: &mut Vec<crate::css::Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        let name = self.render_interp(&inc.name)?;
        let mixin = match self.mixins.get(&name) {
            Some(m) => m.clone(),
            None => {
                return Err(SassError::new(
                    ErrorKind::Eval,
                    format!("Undefined mixin `{name}`"),
                ));
            }
        };

        let site_scope = self.env.current_scope();
        // Argument expressions evaluate in the caller's scope; defaults resolve
        // later against the mixin body scope so `$h: $w` sees the prior parameter.
        let raw = self.collect_args(&inc.args)?;

        // `@content` must replay in the include site's *whole* environment, so the
        // site arena (and its owning module) is captured up front — a scope id alone
        // is meaningless once the body swaps into the defining module's live env.
        let site_env = self.env.clone();
        let site_owner = self.env_owner.clone();
        self.content_stack.push(ContentCtx {
            block: inc.content.clone().unwrap_or_default(),
            params: inc.content_params.clone(),
            site_env,
            site_scope,
            site_owner,
        });

        // The body runs in its defining module's *live* environment: same-module
        // nested calls share `self.env` directly; cross-module calls park the caller
        // and swap the module's persistent env in so `!global` writes (e.g. BEM
        // `$B`/`$E`) propagate across the whole call tree.
        let def = self
            .mixin_module
            .get(&name)
            .cloned()
            .unwrap_or_else(|| self.env_owner.clone());
        let swapped = self.begin_closure(&def);
        self.env.enter();
        self.bind_params(&mixin.params, raw)?;
        for s in &mixin.body {
            self.collect_stmt(s, sel_ctx, decls, out)?;
        }
        self.env.leave();
        self.end_closure(swapped);
        self.content_stack.pop();
        Ok(())
    }

    /// `@content(args?)`: replay the innermost include's block in the call site's
    /// scope, binding `using (…)` parameters from the passed arguments.
    pub(super) fn run_content(
        &mut self,
        c: &Content,
        sel_ctx: &[String],
        decls: &mut Vec<crate::css::Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        let Some(ctx) = self.content_stack.last().cloned() else {
            return Ok(()); // no `@content` passed → nothing to expand
        };
        if ctx.block.is_empty() {
            return Ok(());
        }

        // Arguments are evaluated in the mixin-body scope (the `@content` site).
        let mut vals: Vec<SassValue> = Vec::new();
        for arg in &c.args {
            match arg {
                Arg::Positional(e) | Arg::Splat(e) => vals.push(self.eval_expr(e)?),
                Arg::Named { value, .. } => vals.push(self.eval_expr(value)?),
            }
        }

        // Commit the mixin body's module env (holding `!global` writes like the BEM
        // `$B`) before switching to the include site to replay `@content`: any nested
        // `@include` back into this module (e.g. `e()` inside `b()`'s content) must
        // read those in-progress globals from `module_envs`, not a stale snapshot.
        self.module_envs.insert(self.env_owner.clone(), self.env.clone());
        let saved = std::mem::replace(&mut self.env, ctx.site_env);
        let saved_owner = std::mem::replace(&mut self.env_owner, ctx.site_owner);
        let prev = self.env.set_scope(ctx.site_scope);
        self.env.enter();
        for (p, v) in ctx.params.iter().zip(vals) {
            self.env.define(p, v, AssignFlags::default());
        }
        for s in &ctx.block {
            self.collect_stmt(s, sel_ctx, decls, out)?;
        }
        self.env.leave();
        self.env.set_scope(prev);
        self.env = saved;
        self.env_owner = saved_owner;
        Ok(())
    }
}

/// Raw call arguments already evaluated in the caller's scope.
pub(crate) struct RawArgs {
    positional: Vec<SassValue>,
    named: HashMap<String, SassValue>,
}

impl RawArgs {
    /// Borrow the positional argument values (for builtin dispatch, task 8).
    pub(crate) fn positional(&self) -> &[SassValue] {
        &self.positional
    }

    /// Borrow the named-argument map, keyed without the leading `$`.
    pub(crate) fn named(&self) -> &HashMap<String, SassValue> {
        &self.named
    }

    /// Assemble parts directly (used by `meta.call` to forward a sub-argument
    /// list whose head — the function reference — has been peeled off).
    pub(crate) fn from_parts(positional: Vec<SassValue>, named: HashMap<String, SassValue>) -> Self {
        Self { positional, named }
    }
}

impl EvalState {
    /// Evaluate each argument expression in the current (caller) scope, expanding
    /// splats into positional values.
    pub(super) fn collect_args(&mut self, args: &[Arg]) -> Ev<RawArgs> {
        let mut positional: Vec<SassValue> = Vec::new();
        let mut named: HashMap<String, SassValue> = HashMap::new();
        for arg in args {
            match arg {
                Arg::Positional(e) => {
                    // CSS-style named argument (`f(c=d)`): the css color
                    // overloads read these as keyword pairs, never as
                    // assignment expressions (css_overloads golden
                    // `color.alpha(c=d)` → `alpha(c=d)`).
                    if let Expr::Binary {
                        op: BinOp::Assign,
                        left,
                        right,
                        ..
                    } = e
                        && let Expr::Literal(Literal::Ident(key)) = left.as_ref()
                    {
                        named.insert(key.clone(), self.eval_expr(right)?);
                        continue;
                    }
                    positional.push(self.eval_expr(e)?);
                }
                Arg::Named { name, value } => {
                    let v = self.eval_expr(value)?;
                    named.insert(name.trim_start_matches('$').to_string(), v);
                }
                Arg::Splat(e) => match self.eval_expr(e)? {
                    SassValue::List { items, .. } => positional.extend(items),
                    // `$named...` where the value is a map feeds the *named* pool
                    // (keys must be strings; a non-string key is a golden error we
                    // surface as a stringified key).
                    SassValue::Map { entries } => {
                        for (k, v) in entries {
                            let name = match k {
                                SassValue::Str { text, .. } => text,
                                other => other.to_css(),
                            };
                            named.insert(name.trim_start_matches('$').to_string(), v);
                        }
                    }
                    other => positional.push(other),
                },
            }
        }
        Ok(RawArgs { positional, named })
    }

    /// Bind a parameter list against [`RawArgs`], defining each binding in the
    /// current (callee) scope so later defaults can read earlier parameters.
    pub(super) fn bind_params(&mut self, params: &[Param], mut raw: RawArgs) -> Ev<()> {
        let mut pi = 0usize;
        // A rest parameter (`$args...`) is bound *after* the loop: it absorbs every
        // leftover positional value and, crucially, every named argument that no
        // earlier parameter consumed. Those named args become the ArgList's keyword
        // part, read back by `meta.keywords` / `keyword-get`. Deferring the define
        // lets ordinary named parameters claim their values from `raw.named` first.
        let mut rest: Option<(String, Vec<SassValue>)> = None;
        for p in params {
            let value = if p.rest {
                let leftover = raw.positional[pi..].to_vec();
                pi = raw.positional.len();
                rest = Some((p.name.clone(), leftover));
                continue;
            } else if let Some(v) = raw.named.remove(&p.name) {
                v
            } else if pi < raw.positional.len() {
                let v = raw.positional[pi].clone();
                pi += 1;
                v
            } else if let Some(default) = &p.default {
                self.eval_expr(default)?
            } else {
                return Err(SassError::new(
                    ErrorKind::Eval,
                    format!("Missing argument `${}`", p.name),
                ));
            };
            self.env.define(&p.name, value, AssignFlags::default());
        }

        match rest {
            Some((name, positional)) => {
                // Unmatched named args fold into the rest ArgList's keyword part,
                // sorted by name for deterministic serialization. Sass treats `_`
                // and `-` in identifiers as interchangeable; the canonical spelling
                // uses dashes, so normalize each key.
                let mut keywords: Vec<(String, SassValue)> = raw
                    .named
                    .into_iter()
                    .map(|(k, v)| (k.replace('_', "-"), v))
                    .collect();
                keywords.sort_by(|a, b| a.0.cmp(&b.0));
                self.env.define(
                    &name,
                    SassValue::List {
                        items: positional,
                        sep: ListSep::Comma,
                        bracketed: false,
                        keywords: Some(keywords),
                    },
                    AssignFlags::default(),
                );
            }
            None => {
                if pi < raw.positional.len() {
                    return Err(SassError::new(
                        ErrorKind::Eval,
                        "Too many positional arguments",
                    ));
                }
                if let Some(extra) = raw.named.keys().min() {
                    return Err(SassError::new(
                        ErrorKind::Eval,
                        format!("Unknown argument `${extra}`"),
                    ));
                }
            }
        }
        Ok(())
    }
}
