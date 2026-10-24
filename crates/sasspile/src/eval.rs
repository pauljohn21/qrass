//! Variable binding & substitution as an eval stage on the stream: `scan_map`
//! carries an owned `Env` (a `HashMap`) rule-to-rule, binding `$name: value`
//! statements (recorded, never emitted) and substituting `$name` references in later
//! selectors and declaration values. An unknown `$name` is left verbatim, so
//! plain-CSS tokens containing `$` never regress. Block-lexical scoping is a later
//! increment; this is document-order visibility (globals first, then in-tree order).

use std::collections::HashMap;

use crate::ast::Rule;
use crate::expr::{Env as ValueEnv, Expr, parse_expr, value_to_css};
use crate::value::Value;

#[derive(Default)]
pub(crate) struct Env {
    vars: HashMap<String, String>,
}

impl Env {
    /// Seed from top-level `$var:` bindings in document order (a later binding may
    /// reference an earlier one).
    pub(crate) fn from_globals(globals: &[(String, String)]) -> Env {
        globals.iter().fold(Env::default(), |mut env, (prop, value)| {
            if let Some(name) = prop.strip_prefix('$') {
                let resolved = env.subst(value);
                env.vars.insert(name.to_string(), resolved);
            }
            env
        })
    }

    /// Resolve a sibling list, folding consecutive `@if` / `@else if` / `@else`
    /// directive rules into the single branch whose condition first holds (lazy:
    /// later branches are skipped once one is taken). Non-conditional rules resolve
    /// normally.
    pub(crate) fn resolve_all(&mut self, rules: Vec<Rule>) -> Vec<Rule> {
        struct St {
            out: Vec<Rule>,
            group: Option<(bool, Vec<Rule>)>,
        }
        let flush = |st: &mut St, env: &mut Self| {
            if let Some((_, branch)) = st.group.take() {
                st.out.extend(env.resolve_all(branch));
            }
        };
        let st = rules.into_iter().fold(
            St {
                out: Vec::new(),
                group: None,
            },
            |mut st, rule| {
                match rule.directive.as_deref() {
                    Some(d) if d.starts_with("@if ") => {
                        flush(&mut st, self);
                        let taken = self.truthy(&d[4..]);
                        st.group = Some((taken, if taken { rule.children } else { Vec::new() }));
                    }
                    Some(d) if d.starts_with("@else if ") => {
                        if let Some(g) = st.group.as_mut()
                            && !g.0
                            && self.truthy(&d[9..])
                        {
                            g.0 = true;
                            g.1 = rule.children;
                        }
                    }
                    Some("@else") => {
                        if let Some(g) = st.group.as_mut()
                            && !g.0
                        {
                            g.0 = true;
                            g.1 = rule.children;
                        }
                    }
                    Some(d) if d.starts_with("@each ") => {
                        flush(&mut st, self);
                        self.expand(&d[6..], rule.children, true, &mut st.out);
                    }
                    Some(d) if d.starts_with("@for ") => {
                        flush(&mut st, self);
                        self.expand(&d[5..], rule.children, false, &mut st.out);
                    }
                    _ => {
                        flush(&mut st, self);
                        st.out.push(self.resolve(rule));
                    }
                }
                st
            },
        );
        let mut st = st;
        flush(&mut st, self);
        st.out
    }

    /// Sass truthiness of a condition string: only `false` / empty (`null`) are falsy.
    fn truthy(&mut self, cond: &str) -> bool {
        let rendered = self.render(cond).unwrap_or_else(|| cond.to_string());
        let t = rendered.trim();
        !t.is_empty() && t != "false"
    }

    /// Expand one `@each`/`@for` body per binding, then restore the loop variable's
    /// prior binding (block scoping for the loop var only).
    fn expand(&mut self, spec: &str, body: Vec<Rule>, each: bool, out: &mut Vec<Rule>) {
        let bindings = if each { self.each_bindings(spec) } else { self.for_bindings(spec) };
        let key = bindings.first().map(|(k, _)| k.clone());
        let prev = key.as_ref().and_then(|k| self.vars.get(k).cloned());
        bindings.into_iter().for_each(|(k, v)| {
            self.vars.insert(k, v);
            out.extend(self.resolve_all(body.clone()));
        });
        match (key, prev) {
            (Some(k), Some(p)) => self.vars.insert(k, p),
            (Some(k), None) => self.vars.remove(&k),
            _ => None,
        };
    }

    /// `@each $v in <list>` → one binding per item. Single variable only; the list is
    /// rendered (so `$list`/`#{}` resolve) then split on commas when present, else on
    /// whitespace.
    fn each_bindings(&self, spec: &str) -> Vec<(String, String)> {
        let Some((var_part, list_part)) = spec.split_once(" in ") else {
            return Vec::new();
        };
        let var = var_part.trim().trim_start_matches('$');
        if var.is_empty() || var.contains(',') {
            return Vec::new();
        }
        let list = self.render(list_part.trim()).unwrap_or_default();
        let items: Vec<String> = if list.contains(',') {
            list.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
        } else {
            list.split_whitespace().map(String::from).collect()
        };
        items.into_iter().map(|it| (var.to_string(), it)).collect()
    }

    /// `@for $i from <a> to|through <b>` → numeric range bindings.
    fn for_bindings(&self, spec: &str) -> Vec<(String, String)> {
        let Some((var_part, rest)) = spec.split_once(" from ") else {
            return Vec::new();
        };
        let var = var_part.trim().trim_start_matches('$');
        if var.is_empty() {
            return Vec::new();
        }
        let rendered = self.render(rest.trim()).unwrap_or_else(|| rest.trim().to_string());
        let toks: Vec<&str> = rendered.split_whitespace().collect();
        let num = |s: &str| s.trim().parse::<i64>().ok();
        let (Some(a), Some(b)) = (toks.first().copied().and_then(num), toks.get(2).copied().and_then(num))
        else {
            return Vec::new();
        };
        let end = match toks.get(1).copied() {
            Some("to") => b,
            Some("through") => b + 1,
            _ => return Vec::new(),
        };
        (a..end).map(|n| (var.to_string(), n.to_string())).collect()
    }

    /// Resolve one rule: bind its `$prop:` declarations, canonicalise ordinary
    /// declaration values and its selector (an at-rule/directive block keeps its
    /// inherited selector), and recurse children as a sibling list.
    pub(crate) fn resolve(&mut self, rule: Rule) -> Rule {
        let directive = rule.directive.clone();
        let selector = if directive.is_some() {
            rule.selector.clone()
        } else {
            self.render(&rule.selector)
                .unwrap_or_else(|| rule.selector.clone())
        };
        let mut env_rule = Rule {
            selector,
            decls: Vec::new(),
            children: Vec::new(),
            directive,
        };
        rule.decls.into_iter().for_each(|(prop, value)| {
            tracing::debug!(prop = %prop, raw = %value, "decl");
            if let Some(name) = prop.strip_prefix('$') {
                if let Some(v) = self.render(&value) {
                    self.vars.insert(name.to_string(), v);
                }
            } else if let Some(v) = self.render(&value) {
                env_rule.decls.push((prop, v));
            }
        });
        env_rule.children = self.resolve_all(rule.children);
        env_rule
    }

    /// Canonicalise a value/selector: `#{ … }` via `value_to_css`; a pure numeric
    /// `+`/`-` arithmetic (no `/ * %`, which are ambiguous in CSS values like
    /// `font: 12px/1.5` or `grid: 1 / 2`) via `parse_expr` + eval; everything else keeps
    /// the existing `$var` substitution unchanged.
    fn render(&self, text: &str) -> Option<String> {
        if text.contains("#{") {
            return value_to_css(text, &self.value_env());
        }
        if text.contains(['/', '*', '%']) {
            return Some(self.subst(text));
        }
        let expr = parse_expr(text);
        if matches!(expr, Expr::Bin(..) | Expr::Func { .. })
            && let Ok(v) = expr.eval(&self.value_env())
        {
            return Some(v.to_css());
        }
        Some(self.subst(text))
    }

    fn value_env(&self) -> ValueEnv {
        self.vars
            .iter()
            .map(|(k, v)| {
                let value = Value::parse_number(v).unwrap_or_else(|| Value::Raw(v.clone()));
                (k.clone(), value)
            })
            .collect()
    }

    /// Replace each `$ident` with its bound value (env keyed WITHOUT the `$`); leave
    /// it intact if undefined.
    fn subst(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' {
                let mut j = i + 1;
                while j < chars.len()
                    && (chars[j].is_ascii_alphanumeric() || chars[j] == '-' || chars[j] == '_')
                {
                    j += 1;
                }
                if j > i + 1 {
                    let name: String = chars[i + 1..j].iter().collect();
                    if let Some(value) = self.vars.get(&name) {
                        out.push_str(value);
                        i = j;
                        continue;
                    }
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }
}
