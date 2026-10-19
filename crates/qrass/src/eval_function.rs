//! User-defined functions for [`EvalState`] (task 7.2): `@function` registration,
//! `@return` capture and call dispatch (`Expr::FunctionCall`).
//!
//! A function body runs in a fresh child of the definition-time global scope with
//! its parameters bound; statements are evaluated for their bindings (not CSS)
//! until a `@return` yields the value. Bodies may use `@if`/`@each`/`@for`/`@while`
//! and recurse (calling other functions via [`EvalState::eval_expr`]). A missing
//! `@return` yields `null`, matching the sass spec. Builtins are dispatched here
//! too once the registry lands (task group 8).

use crate::ast::{Block, For, FunctionDecl, If, Return, Stmt, While};
use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall};
use crate::env::AssignFlags;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::eval_mixin::RawArgs;
use super::{Ev, EvalState};
use crate::eval::eval_control::{each_elements, int_bound};


impl EvalState {
    /// Register a `@function` definition (last definition wins for the name),
    /// snapshotting the defining module's environment so the body resolves its
    /// free variables / `ns.$var` bindings lexically (golden scoping).
    pub(super) fn define_function(&mut self, f: &FunctionDecl) {
        self.function_env.insert(f.name.clone(), self.env.clone());
        self.function_module.insert(f.name.clone(), self.name.clone());
        self.functions.insert(f.name.clone(), f.clone());
    }

    /// Dispatch a call. Resolution order: user-defined function (which shadows
    /// a builtin of the same name) → environment-dependent meta forms → the pure
    /// builtin registry (task 8, D6) → `Undefined function`. Arguments evaluate
    /// in the caller's scope; a user function body runs in a fresh child of the
    /// *global* frame (lexical capture of the definition, never the call-site
    /// locals).
    pub(crate) fn eval_call(&mut self, call: &FunctionCall) -> Ev<SassValue> {
        let key = match &call.namespace {
            Some(ns) => format!("{ns}.{}", call.name),
            None => call.name.clone(),
        };
        if let Some(func) = self.functions.get(&key).cloned() {
            // Arguments evaluate in the caller's scope…
            let raw = self.collect_args(&call.args)?;
            // …but the body runs in its *definition-time* module env snapshot, entered
            // as a child so parameter defaults and free lookups resolve lexically
            // against that module (never the call-site locals). The snapshot also
            // fixes `env_owner` to the defining module so a nested `@include` in the
            // body dispatches closures against the right module.
            let def_env = self
                .function_env
                .get(&key)
                .cloned()
                .unwrap_or_else(|| self.env.clone());
            let def_owner = self
                .function_module
                .get(&key)
                .cloned()
                .unwrap_or_else(|| self.env_owner.clone());
            let saved_env = std::mem::replace(&mut self.env, def_env);
            let saved_owner = std::mem::replace(&mut self.env_owner, def_owner);
            self.env.enter();
            self.bind_params(&func.params, raw)?;
            let value = match self.eval_fn_body(&func.body)? {
                Some(v) => v,
                None => SassValue::Null,
            };
            self.env.leave();
            self.env = saved_env;
            self.env_owner = saved_owner;
            return Ok(value);
        }

        // Modern CSS color syntax (`rgb(r g b / a)`) is a single space-separated
        // argument; the ordinary value parser reads its `/` as division and would
        // evaluate `0 / 15%` to `0`, destroying the alpha, so it is rebuilt from the
        // raw argument AST here — before argument evaluation runs — and routed to the
        // `rgb`/`rgba` builtin as comma positional arguments.
        if let Some(v) = self.eval_space_color(call)? {
            return Ok(v);
        }

        // CSS Color 4 literals (`color(a98-rgb 0.2 0.5 0.7)`, `lab(...)`, ...).
        if let Some(v) = self.eval_color4_literal(call)? {
            return Ok(v);
        }

        // The bare `invert()` is both a CSS filter function and — given Sass
        // values — the `color.invert` builtin; dart splits on argument kind.
        if call.namespace.is_none() && call.name == "invert" {
            return self.eval_plain_invert(call);
        }

        // Plain-CSS passthrough: a bare call that is neither a known builtin nor a
        // meta form is emitted verbatim (the goldens keep unknown calls as plain CSS),
        // preserving calc/var operators literally while resolving interpolation and
        // nested Sass calls. Namespaced unknowns still error below.
        if call.namespace.is_none()
            && !crate::builtins::contains(None, &call.name)
            && !is_meta_name(&call.name)
            // A CSS-filter call like `alpha(c=d)` passes through verbatim even
            // though `c=d` is not an evaluable Sass expression (alpha.hrx
            // `filter/one_arg` + `filter/multi_args` goldens).
            || (call.namespace.is_none()
                && call.args.iter().any(|a| matches!(
                    a,
                    Arg::Positional(Expr::Binary {
                        op: BinOp::Assign,
                        ..
                    })
                )))
        {
            let text = self.render_css_call(call)?;
            return Ok(SassValue::Str {
                text,
                quoted: false,
            });
        }

        let raw = self.collect_args(&call.args)?;
        // Env-dependent meta forms need the live scope / registries, so they are
        // resolved here rather than in the pure builtin table.
        if let Some(v) = self.try_env_meta(call.namespace.as_deref(), &call.name, &raw)? {
            return Ok(v);
        }
        // Pure builtins: `map.get`/`map-get` → the shared double registry.
        // CSS overloads first (`modules/color/css_overloads` goldens): under
        // the color namespace, arguments that do not fit the Sass signature
        // (a lone non-color value, or non-parameter named pairs like `c=d`)
        // re-emit the plain spelling under the base name — before any arity
        // check could claim the call.
        if call.namespace.as_deref() == Some("color")
            && matches!(
                call.name.as_str(),
                "grayscale" | "invert" | "alpha" | "opacity"
            )
            && let Some(v) = super::eval_color_space::css_color_overload(&raw, &call.name)
        {
            return Ok(v);
        }
        if let Some(result) = crate::builtins::call(
            call.namespace.as_deref(),
            &call.name,
            raw.positional(),
            raw.named(),
        ) {
            return result.map_err(|e| {
                e.note(format!("during builtin `{key}` in module {}", self.name))
            });
        }
        // dart's wording for a *module*-level miss (modules/color/error
        // goldens): the hyphenated legacy spellings name the function and its
        // home module; the `*-color` compat spellings report the bare
        // `Undefined function.`.
        if call.namespace.as_deref() == Some("color") {
            if matches!(
                call.name.as_str(),
                "adjust-color" | "scale-color" | "change-color"
            ) {
                return Err(SassError::new(
                    ErrorKind::UndefinedFunction,
                    "Undefined function.",
                ));
            }
            if matches!(
                call.name.as_str(),
                "adjust-hue"
                    | "lighten"
                    | "darken"
                    | "saturate"
                    | "desaturate"
                    | "opacify"
                    | "fade-in"
                    | "transparentize"
                    | "fade-out"
            ) {
                return Err(SassError::new(
                    ErrorKind::UndefinedFunction,
                    format!(
                        "The function {}() isn't in the sass:color module.",
                        call.name
                    ),
                ));
            }
        }
        Err(SassError::new(
            ErrorKind::UndefinedFunction,
            format!("Undefined function \"{key}\""),
        ))
    }

    /// Resolve the environment-dependent `meta` predicates that cannot live in
    /// the pure builtin registry: `variable-exists`, `global-variable-exists`,
    /// `mixin-exists`, `function-exists`, `content-exists`. Returns `Ok(None)` for
    /// any other name so the caller falls through to the pure table / undefined.
    fn try_env_meta(
        &mut self,
        namespace: Option<&str>,
        name: &str,
        raw: &RawArgs,
    ) -> Ev<Option<SassValue>> {
        if !matches!(namespace, None | Some("meta")) {
            return Ok(None);
        }
        let first = || match raw.positional().first() {
            Some(SassValue::Str { text, .. }) => text.clone(),
            Some(other) => other.to_css(),
            None => String::new(),
        };
        let found = match name {
            "variable-exists" => SassValue::Bool(self.env.get(&first()).is_some()),
            "global-variable-exists" => SassValue::Bool(self.env.defined_global(&first())),
            "mixin-exists" => SassValue::Bool(self.mixins.contains_key(&first())),
            "function-exists" => {
                let f = first();
                // Sass treats `-` and `_` as interchangeable in identifiers
                // (function_exists `dash_insensitive` goldens: defining `a_b`
                // makes `function-exists(a-b)` true).
                let dashed = f.replace('_', "-");
                let underscored = f.replace('-', "_");
                // `meta.function-exists($name, $module)` searches that
                // module's own functions (`different_module` goldens:
                // `("rgb", "list")` is false, `("red", "color")` true).
                let module = raw
                    .positional()
                    .get(1)
                    .map(|v| v.to_css())
                    .or_else(|| raw.named().get("module").map(|v| v.to_css()));
                if let Some(ns) = module {
                    let in_module = [dashed.as_str(), underscored.as_str()]
                        .iter()
                        .any(|k| crate::builtins::contains(Some(&ns), k));
                    return Ok(Some(SassValue::Bool(in_module)));
                }
                let user_defined = [f.as_str(), dashed.as_str(), underscored.as_str()]
                    .iter()
                    .any(|k| self.functions.contains_key(*k));
                let builtin = [dashed.as_str(), underscored.as_str()]
                    .iter()
                    .any(|k| crate::builtins::contains(None, k));
                SassValue::Bool(user_defined || builtin)
            }
            "content-exists" => SassValue::Bool(!self.content_stack.is_empty()),
            _ => return Ok(None),
        };
        Ok(Some(found))
    }

    /// Run a function body, returning the first `@return` value if any.
    fn eval_fn_body(&mut self, block: &Block) -> Ev<Option<SassValue>> {
        for s in block {
            if let Some(v) = self.eval_fn_stmt(s)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// Evaluate one statement inside a function body for its bindings / return.
    fn eval_fn_stmt(&mut self, s: &Stmt) -> Ev<Option<SassValue>> {
        match s {
            Stmt::Return(r) => self.eval_return(r),
            Stmt::Variable(v) => {
                self.bind_variable(v, false)?;
                Ok(None)
            }
            Stmt::If(i) => self.eval_fn_if(i),
            Stmt::Each(e) => {
                let value = self.eval_expr(&e.list)?;
                for binding in each_elements(&value, e.vars.len()) {
                    self.env.enter();
                    for (var, val) in e.vars.iter().zip(binding) {
                        self.env.define(var, val, AssignFlags::default());
                    }
                    let r = self.eval_fn_body(&e.body);
                    self.env.leave();
                    if let Some(v) = r? {
                        return Ok(Some(v));
                    }
                }
                Ok(None)
            }
            Stmt::For(f) => self.eval_fn_for(f),
            Stmt::While(w) => self.eval_fn_while(w),
            // `@include` runs for side effects (e.g. `!global` writes); no CSS.
            Stmt::Include(inc) => {
                self.run_include(inc, &[], &mut Vec::new(), &mut Vec::new())?;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn eval_return(&mut self, r: &Return) -> Ev<Option<SassValue>> {
        let value = match &r.expr {
            Some(e) => self.eval_expr(e)?,
            None => SassValue::Null,
        };
        Ok(Some(value))
    }

    fn eval_fn_if(&mut self, i: &If) -> Ev<Option<SassValue>> {
        for clause in &i.clauses {
            let take = match &clause.cond {
                None => true,
                Some(c) => self.eval_expr(c)?.is_truthy(),
            };
            if take {
                self.env.enter();
                let r = self.eval_fn_body(&clause.body);
                self.env.leave();
                return r;
            }
        }
        Ok(None)
    }

    fn eval_fn_for(&mut self, f: &For) -> Ev<Option<SassValue>> {
        let from = int_bound(self.eval_expr(&f.from)?)?;
        let to = int_bound(self.eval_expr(&f.to)?)?;
        let last = if f.inclusive {
            to
        } else {
            to + if from <= to { -1 } else { 1 }
        };
        let step = if from <= last { 1 } else { -1 };
        let mut i = from;
        while (step > 0 && i <= last) || (step < 0 && i >= last) {
            self.env.enter();
            self.env.define(
                &f.var,
                SassValue::Number {
                    value: i as f64,
                    unit: None,
                },
                AssignFlags::default(),
            );
            let r = self.eval_fn_body(&f.body);
            self.env.leave();
            if let Some(v) = r? {
                return Ok(Some(v));
            }
            i += step;
        }
        Ok(None)
    }

    fn eval_fn_while(&mut self, w: &While) -> Ev<Option<SassValue>> {
        let mut guard = 0u32;
        self.env.enter();
        while self.eval_expr(&w.cond)?.is_truthy() {
            guard += 1;
            if guard > 100_000 {
                self.env.leave();
                return Err(SassError::new(
                    ErrorKind::Eval,
                    "@while loop exceeded iteration limit",
                ));
            }
            let r = self.eval_fn_body(&w.body);
            if let Some(v) = r? {
                self.env.leave();
                return Ok(Some(v));
            }
        }
        self.env.leave();
        Ok(None)
    }
}

/// The environment-dependent `meta` predicate names: they resolve against the live
/// scope in [`EvalState::try_env_meta`], so they must never be mistaken for a
/// plain-CSS passthrough call.
pub(crate) fn is_meta_name(name: &str) -> bool {
    matches!(
        name,
        "variable-exists"
            | "global-variable-exists"
            | "mixin-exists"
            | "function-exists"
            | "content-exists"
    )
}
