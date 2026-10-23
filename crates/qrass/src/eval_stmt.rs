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

use super::eval_stmt_text::{
    combine_at_root, combine_selectors, contains_placeholder, looks_like_css_import,
};
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
            Stmt::Variable(v) => self.bind_variable(v, sel_ctx.is_empty()),
            Stmt::Declaration(d) => {
                // A `ns.$var: value` is a member assignment into that module
                // (never a CSS declaration) — at ANY block depth: "Namespaced
                // assignments always assign to the other module's variable,
                // even if they're nested in a block scope" (`forward/member`
                // `as` `variable_assignment/nested`). Write the defining
                // module's live cell + the ns view (OpenSpec M4 slice 3/4).
                {
                    let prop = self.render_interp(&d.prop)?;
                    if let Some((ns, name)) = prop.split_once(".$")
                        && self.env.has_namespace(ns)
                    {
                        let value = self.eval_expr(&d.value)?;
                        let cell =
                            self.ns_origins.get(&(ns.to_string(), name.to_string())).cloned();
                        self.env.set_member(ns, name, value.clone());
                        if let Some(c) = cell {
                            self.member_store.insert(c, value);
                        }
                        return Ok(());
                    }
                    // A bare declaration at stylesheet root is an error in
                    // the goldens (`load_css/error/load` `top_level`/`nesting`:
                    // "Declarations may only be used within style rules.")
                    // — never silently dropped.
                    if sel_ctx.is_empty() {
                        return Err(SassError::new(
                            ErrorKind::Eval,
                            "Declarations may only be used within style rules.",
                        ));
                    }
                }
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
            Stmt::AtRule(a) => self.run_unknown_atrule(a, sel_ctx, decls, out),
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
    ///
    /// `root` marks a declaration at the running module's top level, which is the
    /// only place a `with (…)` configuration can land.
    pub(super) fn bind_variable(&mut self, v: &VariableDecl, root: bool) -> Ev<()> {
        // `ns.$var: value` — a namespaced assignment always reaches the
        // DEFINING module's live cell (`forward/member` `bare` "Namespaced
        // assignments always assign to the defining module"), visible to
        // every other read path (OpenSpec M4 slice 3).
        if let Some((ns, name)) = v.name.split_once(".$")
            && self.env.has_namespace(ns)
        {
            let value = self.eval_expr(&v.value)?;
            let cell = self
                .ns_origins
                .get(&(ns.to_string(), name.to_string()))
                .cloned();
            self.env.set_member(ns, name, value.clone());
            if let Some(c) = cell {
                self.member_store.insert(c, value);
            }
            return Ok(());
        }
        // A `!default` at module root answers the matching offer: the offered value
        // is bound and the declaration's own right-hand side is never evaluated
        // (`with/doesnt_run_default.hrx`), except for a `null` offer, which the
        // default still wins (`with/null.hrx`). Any other declaration leaves the
        // configuration alone.
        let configured = if v.default && root { self.take_config(&v.name) } else { None };
        let value = match configured {
            Some(value) => value,
            None => self.eval_expr(&v.value)?,
        };
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
    /// then clear them — the goldens do this at every nested-rule / at-rule
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
        // flush `rule_decls` first (the goldens split a rule's declarations whenever
        // a child block interrupts them) — see `collect_stmt`.
        let mut rule_decls: Vec<Item> = Vec::new();
        let saved_parents = std::mem::replace(&mut self.current_parents, resolved.clone());
        rs.body
            .iter()
            .try_for_each(|s| self.collect_stmt(s, &resolved, &mut rule_decls, out))?;
        self.current_parents = saved_parents;
        if !rule_decls.is_empty() && !concrete.is_empty() {
            out.push(css::render_rule(&concrete, &rule_decls));
        }
        placeholders.into_iter().for_each(|selector| {
            let pairs: Vec<(String, String)> = rule_decls
                .iter()
                .filter_map(|it| match it {
                    Item::Decl { prop, value } => Some((prop.clone(), value.clone())),
                    Item::Comment(_) | Item::Raw(_) => None,
                })
                .collect();
            // Reserve a slot in the emitted stream at this placeholder's own
            // source position; the global `@extend` pass later replaces the
            // marker with the grouped extended rule (or drops it when nothing
            // extends the placeholder), reproducing the goldens' inline placement.
            let slot = self.placeholder_rules.len();
            out.push(format!("\u{f0000}qrass_extend_slot {slot}\u{f0001}"));
            self.placeholder_rules.push(super::PlaceholderRule {
                selector,
                decls: pairs,
                slot,
            });
        });
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
        // `css/supports/comment` + `css/media/whitespace` goldens: a comment
        // eaten from a query head/tail leaves its spacing behind, and the goldens
        // re-serializes each parenthesized condition with trimmed edges and no
        // space before the `:`. Apply only to at-rule query heads, never the
        // body selectors.
        let query = if header == "@supports" || header == "@media" {
            crate::css::tidy_condition(&query)
        } else {
            query
        };
        self.env.enter();
        let mut inner: Vec<String> = Vec::new();
        let mut decls: Vec<Item> = Vec::new();
        body.iter()
            .try_for_each(|s| self.collect_stmt(s, sel_ctx, &mut decls, &mut inner))?;
        // Declarations written directly under the query (no nested selector) apply
        // to the current nesting context, so render them as a leading rule inside
        // the block — the goldens emit `@media { .parent { … } }` for these.
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
        k.body
            .iter()
            .try_for_each(|s| self.collect_stmt(s, &[], &mut Vec::new(), &mut inner))?;
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
        ar.body.iter().try_for_each(|s| {
            self.at_root = true;
            self.collect_stmt(s, sel_ctx, &mut Vec::new(), out)
        })?;
        self.at_root = saved;
        self.env.leave();
        Ok(())
    }

    /// Unknown at-rule fallback (`@name value;` / `@name value { … }`): echo the
    /// directive verbatim, evaluating any `#{…}` in the value but passing every
    /// other token through unchanged. A block body is rendered like a container so
    /// nested rules stay grouped; a body-less directive becomes one output line
    /// (or, when nested inside a rule, an interleaved raw item).
    fn run_unknown_atrule(
        &mut self,
        a: &crate::ast::AtRule,
        sel_ctx: &[String],
        decls: &mut Vec<Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        let value = self.render_interp(&a.value)?;
        let header = if value.trim().is_empty() {
            format!("@{}", a.name)
        } else {
            format!("@{} {}", a.name, value)
        };
        match &a.body {
            None => {
                let line = format!("{header};");
                if sel_ctx.is_empty() {
                    out.push(line);
                } else {
                    decls.push(Item::Raw(line));
                }
                Ok(())
            }
            Some(body) => {
                self.flush_decls(sel_ctx, decls, out);
                self.env.enter();
                let mut inner: Vec<String> = Vec::new();
                body.iter()
                    .try_for_each(|s| self.collect_stmt(s, sel_ctx, &mut Vec::new(), &mut inner))?;
                self.env.leave();
                out.push(css::render_at_block(&header, &inner));
                Ok(())
            }
        }
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
