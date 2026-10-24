//! Variable binding & substitution as an eval stage on the stream: `scan_map`
//! carries an owned `Env` (a `HashMap`) rule-to-rule, binding `$name: value`
//! statements (recorded, never emitted) and substituting `$name` references in later
//! selectors and declaration values. An unknown `$name` is left verbatim, so
//! plain-CSS tokens containing `$` never regress. Block-lexical scoping is a later
//! increment; this is document-order visibility (globals first, then in-tree order).

use std::collections::HashMap;

use crate::ast::Rule;
use crate::expr::{Env as ValueEnv, value_to_css};
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
            if prop.starts_with('$') {
                let resolved = env.subst(value);
                env.vars.insert(prop.clone(), resolved);
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
            if prop.starts_with('$') {
                if let Some(v) = self.render(&value) {
                    self.vars.insert(prop, v);
                }
            } else if let Some(v) = self.render(&value) {
                env_rule.decls.push((prop, v));
            }
        });
        env_rule.children = self.resolve_all(rule.children);
        env_rule
    }

    /// Canonicalise a value/selector: only strings containing `#{ … }` are routed
    /// through `value_to_css` (which returns `None` on an undefined interpolation);
    /// everything else keeps the existing `$var` substitution, so already-correct
    /// values are not re-processed.
    fn render(&self, text: &str) -> Option<String> {
        if text.contains("#{") {
            value_to_css(text, &self.value_env())
        } else {
            Some(self.subst(text))
        }
    }

    fn value_env(&self) -> ValueEnv {
        self.vars
            .iter()
            .map(|(k, v)| (k.clone(), Value::Raw(v.clone())))
            .collect()
    }

    /// Replace each `$ident` with its bound value; leave it intact if undefined.
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
                    let name: String = chars[i..j].iter().collect();
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
