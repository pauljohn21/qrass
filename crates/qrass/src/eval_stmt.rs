//! Statement traversal for [`EvalState`] (task 5.3): variable binding, ruleset
//! nesting with `&` resolution, `@media` wrapping and declaration output. Each
//! ruleset becomes one owned CSS chunk holding its *own* declarations (matching
//! expanded output); nested blocks recurse with the resolved selectors as the
//! new parent context. Control flow, mixins and functions are task group 7.

use crate::ast::{Declaration, Ruleset, Stmt, VariableDecl};
use crate::css::{self, Item};
use crate::env::AssignFlags;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::{Ev, EvalState};

impl EvalState {
    /// Evaluate one top-level statement, appending CSS chunks.
    pub(crate) fn eval_top_level(&mut self, stmt: &Stmt, out: &mut Vec<String>) -> Ev<()> {
        // At the sheet root there is no enclosing rule, so any bare declarations
        // fold into a throwaway buffer (invalid CSS) while rulesets flow to `out`.
        self.collect_stmt(stmt, &[], &mut Vec::new(), out)
    }

    /// Fold one statement within the current selector context.
    ///
    /// Declarations are appended to `decls` (the *enclosing rule's* buffer, so
    /// control flow like `@if`/`@each` inline its declarations into the parent
    /// block); whole blocks produced by nested rules / at-rules / modules are
    /// appended to `out` (emitted after the enclosing rule's own block). The body
    /// is expanded eagerly so loop variables are captured per iteration.
    pub(crate) fn collect_stmt(
        &mut self,
        stmt: &Stmt,
        sel_ctx: &[String],
        decls: &mut Vec<Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        match stmt {
            Stmt::Variable(v) => self.bind_variable(v),
            Stmt::Declaration(d) => {
                if let Some((prop, value)) = self.render_declaration(d)? {
                    decls.push(Item::Decl { prop, value });
                }
                Ok(())
            }
            Stmt::Ruleset(rs) => {
                self.flush_decls(sel_ctx, decls, out);
                self.run_ruleset(rs, sel_ctx, out)
            }
            Stmt::Media(m) => {
                self.flush_decls(sel_ctx, decls, out);
                self.run_container("@media", &m.queries, &m.body, sel_ctx, out)
            }
            Stmt::Supports(s) => {
                self.flush_decls(sel_ctx, decls, out);
                self.run_container("@supports", &s.condition, &s.body, sel_ctx, out)
            }
            Stmt::Keyframes(k) => {
                self.flush_decls(sel_ctx, decls, out);
                self.run_keyframes(k, out)
            }
            Stmt::AtRoot(ar) => {
                self.flush_decls(sel_ctx, decls, out);
                self.run_at_root(ar, sel_ctx, out)
            }
            Stmt::Use(u) => self.handle_use(u, out),
            Stmt::Forward(f) => self.handle_forward(f, out),
            Stmt::AtRule(a) if a.name == "import" => self.run_import(a, out),
            Stmt::If(i) => self.run_if(i, sel_ctx, decls, out),
            Stmt::Each(e) => self.run_each(e, sel_ctx, decls, out),
            Stmt::For(f) => self.run_for(f, sel_ctx, decls, out),
            Stmt::While(w) => self.run_while(w, sel_ctx, decls, out),
            Stmt::Mixin(m) => self.define_mixin(m),
            Stmt::Include(inc) => self.run_include(inc, sel_ctx, decls, out),
            Stmt::Content(c) => self.run_content(c, sel_ctx, decls, out),
            Stmt::Extend(e) => self.run_extend(e, sel_ctx),
            Stmt::Function(f) => {
                self.define_function(f);
                Ok(())
            }
            // A loud `/* … */` comment is preserved: inside a rule it interleaves
            // with that rule's declarations; at the sheet root it stands alone.
            // Silent `//` comments are dropped upstream by the lexer.
            Stmt::Comment(c) => {
                if sel_ctx.is_empty() {
                    out.push(css::render_comment(&c.text));
                } else {
                    decls.push(Item::Comment(c.text.clone()));
                }
                Ok(())
            }
            // Constructs still pending (`@extend`, …) and stray declarations at
            // the sheet root produce no output here.
            _ => Ok(()),
        }
    }

    /// `@import`: SCSS module imports are rejected (design D3 — use `@use` /
    /// `@forward`); a plain-CSS import (`…css` literal or `url(…)`) is passed
    /// through verbatim as an at-rule chunk.
    fn run_import(&mut self, a: &crate::ast::AtRule, out: &mut Vec<String>) -> Ev<()> {
        let value = self.render_interp(&a.value)?;
        if looks_like_css_import(&value) {
            out.push(format!("@import {value};"));
            Ok(())
        } else {
            Err(SassError::new(
                ErrorKind::Unsupported,
                "SCSS @import is not supported; use @use or @forward instead",
            ))
        }
    }

    /// Bind a variable declaration honoring sass scoping: `!default` binds only
    /// when the name resolves to nothing; `!global` writes the module-global scope;
    /// a plain assignment *updates the existing binding where it was declared* (so a
    /// loop accumulator like `$n: $n + …` persists across iterations) and only binds
    /// locally when the name is new.
    pub(super) fn bind_variable(&mut self, v: &VariableDecl) -> Ev<()> {
        let value = self.eval_expr(&v.value)?;
        if v.default {
            self.mark_default_used(&v.name);
        }
        if v.global || v.default {
            self.env.define(
                &v.name,
                value,
                AssignFlags {
                    global: v.global,
                    default: v.default,
                },
            );
        } else {
            self.env.set_existing_or_local(&v.name, value);
        }
        Ok(())
    }

    /// Flush a rule's accumulated declarations into a CSS block emitted to `out`,
    /// then clear them — dart-sass does this at every nested-rule / at-rule
    /// boundary so a rule's declarations get split around its children. Only the
    /// concrete (non-placeholder) selectors render; a pure placeholder rule keeps
    /// its declarations for the `@extend` capture instead.
    fn flush_decls(&mut self, sel_ctx: &[String], decls: &mut Vec<Item>, out: &mut Vec<String>) {
        if decls.is_empty() {
            return;
        }
        let concrete: Vec<String> = sel_ctx
            .iter()
            .filter(|s| !contains_placeholder(s))
            .cloned()
            .collect();
        if concrete.is_empty() {
            return;
        }
        out.push(css::render_rule(&concrete, decls));
        decls.clear();
    }

    fn run_ruleset(&mut self, rs: &Ruleset, sel_ctx: &[String], out: &mut Vec<String>) -> Ev<()> {
        let sel_text = self.render_interp(&rs.selectors)?;
        // A ruleset that is a direct child of `@at-root` is lifted: `&` binds to
        // the outer parent but no descendant prefix is added. The flag is consumed
        // here so the body's own nesting then combines normally from `resolved`.
        let resolved = if std::mem::take(&mut self.at_root) {
            combine_at_root(sel_ctx, &sel_text)
        } else {
            combine_selectors(sel_ctx, &sel_text)
        };

        // The block body is a fresh lexical scope: variables declared here bind
        // locally (shadowing outer) and vanish on exit unless `!global`.
        self.env.enter();

        // Split the resolved selectors into concrete and placeholder sets up front.
        // A placeholder rule (selector contains `%name`) is held back for the
        // global `@extend` pass and never rendered on its own; concrete selectors
        // emit normally.
        let (placeholders, concrete): (Vec<String>, Vec<String>) =
            resolved.iter().cloned().partition(|s| contains_placeholder(s));

        // Expand the body in source order into the shared `out` stream, keeping
        // this rule's pending declarations in `rule_decls`. Nested rules / at-rules
        // flush `rule_decls` first (dart-sass splits a rule's declarations whenever
        // a child block interrupts them) — see `collect_stmt`.
        let mut rule_decls: Vec<Item> = Vec::new();
        let saved_parents = std::mem::replace(&mut self.current_parents, resolved.clone());
        for s in &rs.body {
            self.collect_stmt(s, &resolved, &mut rule_decls, out)?;
        }
        self.current_parents = saved_parents;
        if !rule_decls.is_empty() && !concrete.is_empty() {
            out.push(css::render_rule(&concrete, &rule_decls));
        }
        for selector in placeholders {
            let pairs: Vec<(String, String)> = rule_decls
                .iter()
                .filter_map(|it| match it {
                    Item::Decl { prop, value } => Some((prop.clone(), value.clone())),
                    Item::Comment(_) => None,
                })
                .collect();
            // Reserve a slot in the emitted stream at this placeholder's own
            // source position; the global `@extend` pass later replaces the
            // marker with the grouped extended rule (or drops it when nothing
            // extends the placeholder), reproducing dart-sass's inline placement.
            let slot = self.placeholder_rules.len();
            out.push(format!("\u{f0000}qrass_extend_slot {slot}\u{f0001}"));
            self.placeholder_rules.push(super::PlaceholderRule {
                selector,
                decls: pairs,
                slot,
            });
        }
        self.env.leave();
        Ok(())
    }

    /// `@media` / `@supports`: render the body into a child buffer (selectors keep
    /// the current nesting context) and wrap the whole buffer in one at-rule block,
    /// so sibling rules under the same query share a single merged block.
    fn run_container(
        &mut self,
        header: &str,
        condition: &crate::ast::InterpText,
        body: &[Stmt],
        sel_ctx: &[String],
        out: &mut Vec<String>,
    ) -> Ev<()> {
        let query = self.render_interp(condition)?;
        self.env.enter();
        let mut inner: Vec<String> = Vec::new();
        let mut decls: Vec<Item> = Vec::new();
        for s in body {
            self.collect_stmt(s, sel_ctx, &mut decls, &mut inner)?;
        }
        // Declarations written directly under the query (no nested selector) apply
        // to the current nesting context, so render them as a leading rule inside
        // the block — dart-sass emits `@media { .parent { … } }` for these.
        let concrete: Vec<String> = sel_ctx
            .iter()
            .filter(|s| !contains_placeholder(s))
            .cloned()
            .collect();
        if !decls.is_empty() && !concrete.is_empty() {
            inner.insert(0, css::render_rule(&concrete, &decls));
        }
        self.env.leave();
        if !inner.is_empty() {
            out.push(css::render_at_block(&format!("{header} {query}"), &inner));
        }
        Ok(())
    }

    /// `@keyframes <name> { <stop rules> }`: wrap the stop rules under one block.
    fn run_keyframes(&mut self, k: &crate::ast::Keyframes, out: &mut Vec<String>) -> Ev<()> {
        let name = self.render_interp(&k.name)?;
        self.env.enter();
        let mut inner: Vec<String> = Vec::new();
        for s in &k.body {
            self.collect_stmt(s, &[], &mut Vec::new(), &mut inner)?;
        }
        self.env.leave();
        out.push(css::render_at_block(&format!("@keyframes {name}"), &inner));
        Ok(())
    }

    /// `@at-root { … }`: re-parent the wrapped rules to the top level (drop the
    /// descendant prefix) without emitting an at-rule wrapper, while `&` inside a
    /// lifted ruleset still resolves against the *outer* selector.
    fn run_at_root(
        &mut self,
        ar: &crate::ast::AtRoot,
        sel_ctx: &[String],
        out: &mut Vec<String>,
    ) -> Ev<()> {
        self.env.enter();
        let saved = std::mem::replace(&mut self.at_root, true);
        for s in &ar.body {
            self.at_root = true;
            self.collect_stmt(s, sel_ctx, &mut Vec::new(), out)?;
        }
        self.at_root = saved;
        self.env.leave();
        Ok(())
    }

    /// Render a declaration to `(property, value)`, or `None` when its value is
    /// `null` (sass drops null-valued declarations).
    fn render_declaration(&mut self, d: &Declaration) -> Ev<Option<(String, String)>> {
        let prop = self.render_interp(&d.prop)?;
        let value = self.eval_expr(&d.value)?;
        if matches!(value, SassValue::Null) {
            return Ok(None);
        }
        let mut css = value.to_css();
        if d.important {
            css.push_str(" !important");
        }
        Ok(Some((prop, css)))
    }
}

/// A CSS `@import` target (passed through) is a `url(…)` or a quoted string whose
/// URL ends in `.css`. Anything else is an SCSS module import (rejected, D3).
fn looks_like_css_import(value: &str) -> bool {
    value.contains("url(")
        || value
            .split(',')
            .map(|t| t.trim().trim_matches(|c| c == '"' || c == '\''))
            .any(|t| t.ends_with(".css"))
}

/// Combine a selector list for a `@at-root`-lifted ruleset. Like
/// [`combine_selectors`] a `&` is replaced by the outer parent, but a child
/// *without* `&` stands alone at the root instead of being descendant-prefixed.
fn combine_at_root(parents: &[String], child_text: &str) -> Vec<String> {
    let children: Vec<String> = crate::builtins::selector::split_commas(child_text)
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if parents.is_empty() {
        return children;
    }
    parents
        .iter()
        .flat_map(|p| {
            children.iter().map(move |c| {
                if c.contains('&') {
                    c.replace('&', p)
                } else {
                    c.clone()
                }
            })
        })
        .collect()
}

/// Combine a selector list with its resolved parent list. A `&` in a child is
/// replaced by the parent; otherwise the child is a descendant (space). At the
/// root (no parent) the child selectors stand alone.
fn combine_selectors(parents: &[String], child_text: &str) -> Vec<String> {
    let children: Vec<String> = crate::builtins::selector::split_commas(child_text)
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if parents.is_empty() {
        return children;
    }
    parents
        .iter()
        .flat_map(|p| {
            children.iter().map(move |c| {
                if c.contains('&') {
                    c.replace('&', p)
                } else {
                    format!("{p} {c}")
                }
            })
        })
        .collect()
}

/// Whether a resolved selector references a `%placeholder` (so it must be held
/// back for the `@extend` pass). A placeholder is a `%` immediately followed by
/// an identifier-start char (letter / `_` / non-ASCII); a trailing `%` (a
/// keyframe percentage like `50%` or `100%`) is *not* a placeholder.
fn contains_placeholder(sel: &str) -> bool {
    let c: Vec<char> = sel.chars().collect();
    c.windows(2).any(|w| {
        w[0] == '%' && (w[1].is_alphabetic() || w[1] == '_' || !w[1].is_ascii())
    })
}
