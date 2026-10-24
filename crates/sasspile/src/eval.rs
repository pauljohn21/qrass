//! Variable binding & substitution as an eval stage on the stream: `scan_map`
//! carries an owned `Env` (a `HashMap`) rule-to-rule, binding `$name: value`
//! statements (recorded, never emitted) and substituting `$name` references in later
//! selectors and declaration values. An unknown `$name` is left verbatim, so
//! plain-CSS tokens containing `$` never regress. Block-lexical scoping is a later
//! increment; this is document-order visibility (globals first, then in-tree order).

use std::collections::HashMap;

use crate::ast::Rule;

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

    /// Resolve one rule: bind its `$prop:` declarations, substitute ordinary
    /// declaration values and its selector, and recurse children on the same env.
    pub(crate) fn resolve(&mut self, rule: Rule) -> Rule {
        let mut env_rule = Rule {
            selector: self.subst(&rule.selector),
            decls: Vec::new(),
            children: Vec::new(),
        };
        rule.decls.into_iter().for_each(|(prop, value)| {
            tracing::debug!(prop = %prop, raw = %value, "decl");
            if prop.starts_with('$') {
                let v = self.subst(&value);
                self.vars.insert(prop, v);
            } else {
                env_rule.decls.push((prop, self.subst(&value)));
            }
        });
        env_rule.children = rule.children.into_iter().map(|c| self.resolve(c)).collect();
        env_rule
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
