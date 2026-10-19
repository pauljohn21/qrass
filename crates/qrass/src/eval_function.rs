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
use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep, Literal};
use crate::env::AssignFlags;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::eval_mixin::RawArgs;
use super::{Ev, EvalState};
use crate::eval::eval_control::{each_elements, int_bound};

/// How one `hsl`/`hwb` argument slot reads for the modern single-argument form.
enum Slot {
    /// a number or `none` — a real channel value
    Number,
    /// symbolic CSS construct (`var()`/`calc()`, a `/` product, a relative
    /// `from` anchor) — the whole call stays verbatim CSS
    Verbatim,
    /// a concrete non-channel value (quoted string, bare word, color, …) —
    /// dart raises a `$channels:` type error
    Concrete,
}

fn slot_kind(e: &Expr) -> Slot {
    match e {
        Expr::Literal(Literal::Number { .. }) => Slot::Number,
        Expr::Literal(Literal::Ident(w)) if w == "none" => Slot::Number,
        Expr::Literal(Literal::Ident(w)) if w.eq_ignore_ascii_case("from") => Slot::Verbatim,
        Expr::FunctionCall(_) => Slot::Verbatim,
        Expr::Binary {
            op, left, right, ..
        } => {
            if matches!(op, BinOp::Div | BinOp::Mul) {
                // `a / b` inside a slot is kept literal by the CSS layer.
                Slot::Verbatim
            } else {
                match (slot_kind(left), slot_kind(right)) {
                    (Slot::Verbatim, _) | (_, Slot::Verbatim) => Slot::Verbatim,
                    (Slot::Number, Slot::Number) => Slot::Number,
                    _ => Slot::Concrete,
                }
            }
        }
        _ => Slot::Concrete,
    }
}

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

        // Plain-CSS passthrough: a bare call that is neither a known builtin nor a
        // meta form is emitted verbatim (the goldens keep unknown calls as plain CSS),
        // preserving calc/var operators literally while resolving interpolation and
        // nested Sass calls. Namespaced unknowns still error below.
        if call.namespace.is_none()
            && !crate::builtins::contains(None, &call.name)
            && !is_meta_name(&call.name)
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
        Err(SassError::new(
            ErrorKind::UndefinedFunction,
            format!("Undefined function \"{key}\""),
        ))
    }

    /// Handle the modern CSS `rgb(r g b / a)` / `rgba(r g b / a)` space-separated
    /// single-argument form. Returns `Ok(None)` for every other shape so the caller
    /// falls through to ordinary builtin dispatch / passthrough unchanged.
    fn eval_space_color(&mut self, call: &FunctionCall) -> Ev<Option<SassValue>> {
        let rgb_family =
            call.namespace.is_none() && matches!(call.name.as_str(), "rgb" | "rgba");
        // The modern `hsl`/`hsla`/`hwb` single-argument spelling — bare or under
        // the `color` namespace — is also rebuilt here (round-64, hwb/one_arg
        // goldens): split channels route to the legacy builtins (which produce
        // the goldens' `hsl(...)` serialization), anything else falls back to a
        // verbatim slash-preserving passthrough below `eval_hsl_family`.
        let hsl_family = matches!(call.name.as_str(), "hsl" | "hsla" | "hwb")
            && matches!(call.namespace.as_deref(), None | Some("color"));
        if !rgb_family && !hsl_family {
            return Ok(None);
        }
        let Some(Arg::Positional(Expr::List {
            items,
            sep: ListSep::Space,
            ..
        })) = call.args.first()
        else {
            return Ok(None);
        };
        if hsl_family {
            return self.eval_hsl_family(call, items);
        }
        let mut positional: Vec<SassValue> = Vec::new();
        let mut alpha: Option<SassValue> = None;
        for item in items {
            if let Expr::Binary {
                op: BinOp::Div,
                left,
                right,
                ..
            } = item
            {
                // The `/` separates the channels from the alpha; a percentage alpha
                // is normalized to `0..=1` so the builtin's `alpha_arg` accepts it.
                positional.push(self.eval_expr(left)?);
                let a = self.eval_expr(right)?;
                alpha = Some(match a {
                    SassValue::Number {
                        value,
                        unit: Some(ref u),
                    } if u == "%" => SassValue::Number {
                        value: value / 100.0,
                        unit: None,
                    },
                    other => other,
                });
            } else {
                positional.push(self.eval_expr(item)?);
            }
        }
        if let Some(a) = alpha {
            positional.push(a);
        }
        let named = std::collections::HashMap::new();
        let result = crate::builtins::call(None, &call.name, &positional, &named)
            .ok_or_else(|| {
                SassError::new(
                    ErrorKind::UndefinedFunction,
                    format!("Undefined function \"{}\"", call.name),
                )
            })?;
        result.map_err(|e| e.note(format!("during color `{}` in module {}", call.name, self.name)))
            .map(Some)
    }

    /// The modern single-argument `hsl`/`hsla`/`hwb` spelling. Channels that are
    /// all number/`none` leaves (exactly three, alpha optional) are split and
    /// dispatched to the legacy builtins — hwb additionally enforces dart's
    /// strict `%` unit on whiteness/blackness. Every other shape (symbolic
    /// `var()`/`calc()`, a relative-color `from` anchor, a wrong count) is
    /// rendered verbatim with the alpha slash kept literal (`hwb(0 30% var(--c)/0.5)`).
    fn eval_hsl_family(&mut self, call: &FunctionCall, items: &[Expr]) -> Ev<Option<SassValue>> {
        let mut channels: Vec<&Expr> = Vec::new();
        let mut alpha: Option<&Expr> = None;
        for item in items {
            if let Expr::Binary {
                op: BinOp::Div,
                left,
                right,
                ..
            } = item
            {
                channels.push(left);
                alpha = Some(right);
            } else {
                channels.push(item);
            }
        }
        // Dart's `hsl`/`hwb` single-argument form distinguishes three ways a
        // slot can fail: a relative-color `from` anchor or a symbolic
        // `var()`/`calc()` construct keeps the whole call verbatim CSS, while a
        // *concrete* non-number (quoted string, bare word, color, …) is a
        // `$channels:` type error naming the first offending channel. Only
        // numbers and `none` are channels at all.
        let slot_kind = |e: &Expr| -> Slot { slot_kind(e) };
        let channel_names: &[&str] = if call.name == "hwb" {
            &["hue", "whiteness", "blackness"]
        } else {
            &["hue", "saturation", "lightness"]
        };
        // Scan every channel slot (and the alpha): a `var()`/relative anchor
        // anywhere makes the whole call verbatim; otherwise the first concrete
        // non-number is dart's `$channels:` type error; a clean 3-channel list
        // goes on to strict units and the legacy dispatch.
        let mut first_concrete: Option<(usize, &Expr)> = None;
        let mut verbatim = false;
        for (i, e) in channels.iter().enumerate() {
            match slot_kind(e) {
                Slot::Number => {}
                Slot::Verbatim => verbatim = true,
                Slot::Concrete => {
                    if i < 3 && first_concrete.is_none() {
                        first_concrete = Some((i, e));
                    }
                }
            }
        }
        if let Some(a) = alpha
            && matches!(slot_kind(a), Slot::Verbatim)
        {
            verbatim = true;
        }
        if !verbatim {
            if let Some((i, e)) = first_concrete {
                let v = self.eval_expr(e)?;
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!(
                        "$channels: Expected {} channel to be a number, was {}.",
                        channel_names[i],
                        v.to_css()
                    ),
                ));
            }
            if channels.len() == 3 {
            // `color.hwb(0 30 40%)`: hwb's w/b channels only accept `%` (or none).
            if call.name == "hwb" {
                for (i, key) in [(1usize, "whiteness"), (2, "blackness")] {
                    if let Expr::Literal(Literal::Number { value, unit }) = channels[i]
                        && unit.as_deref() != Some("%")
                    {
                        let shown = format!("{value}{}", unit.as_deref().unwrap_or(""));
                        return Err(SassError::new(
                            ErrorKind::Builtins,
                            format!("${key}: Expected {shown} to have unit \"%\"."),
                        ));
                    }
                }
            }
            let mut positional: Vec<SassValue> = Vec::new();
            for e in &channels {
                positional.push(self.eval_expr(e)?);
            }
            if let Some(a) = alpha {
                positional.push(match self.eval_expr(a)? {
                    SassValue::Number {
                        value,
                        unit: Some(u),
                    } if u == "%" => SassValue::Number {
                        value: value / 100.0,
                        unit: None,
                    },
                    other => other,
                });
            }
            let named = std::collections::HashMap::new();
            // A base name with no global builtin (e.g. bare `hwb`, which only
            // exists namespaced in dart) falls through to the Color4 literal /
            // passthrough layers rather than erroring.
            let Some(result) =
                crate::builtins::call(call.namespace.as_deref(), &call.name, &positional, &named)
            else {
                return Ok(None);
            };
            return result
                .map_err(|e| e.note(format!("during color `{}` in module {}", call.name, self.name)))
                .map(Some);
            }
        }
        let mut parts = Vec::with_capacity(items.len());
        for item in items {
            if let Expr::Binary {
                op: BinOp::Div,
                left,
                right,
                ..
            } = item
            {
                parts.push(format!("{}/{}", self.render_css(left)?, self.render_css(right)?));
            } else {
                parts.push(self.render_css(item)?);
            }
        }
        Ok(Some(SassValue::Str {
            text: format!("{}({})", call.name, parts.join(" ")),
            quoted: false,
        }))
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
                SassValue::Bool(
                    self.functions.contains_key(&f) || crate::builtins::contains(None, &f),
                )
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
