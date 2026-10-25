//! State — the running program state, held by the `Exec` phase kernel's **observer** (`crate::phases`).
//!
//! This is the framework's own shape for stateful forward computation: the state lives inside
//! `ExecObserver` and is moved by `&mut self` per event, while the stream carries only what the
//! phase actually produces (the settled rule). The previous form — `scan(seed, step)` — made the
//! accumulator **be** the emitted item, so every event cloned the whole `State` (open blocks, env,
//! rules) and `filter_map` then threw all but the rule away: correct decomposition, wrong vehicle.
//! Nothing is smuggled alongside a declaration either (no per-decl `Arc<Env>` snapshot), so a
//! declaration is still evaluated at the position it appears — Sass's document-order semantics.
//!
//! Nesting depth is data inside this value (`scopes`), never a call stack; no observable type
//! appears here, because `compile` (crate root) owns the one operator chain.

use crate::env::Env;
use crate::expr::{self, Expr};
use crate::lexer::{Event, Tok, canon, render, render_raw, token_text};
use crate::value::Value;

use crate::SassError;

/// A resolved CSS rule: one **declaration run** flushed from a block. A block can yield several
/// rules (`sass-spec` `css/style_rule` `declaration/interleaved/around_style_rule`), so this is not
/// "the block's whole body".
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: String,
    pub decls: Vec<(String, Value)>,
}

impl StyleRule {
    /// Serialise the rule to an expanded CSS block. [`Value::to_css`] stays the one place a
    /// value becomes text.
    pub fn to_css(&self) -> String {
        let body = self
            .decls
            .iter()
            .map(|(prop, value)| format!("  {prop}: {};\n", value.to_css()))
            .collect::<String>();
        format!("{} {{\n{}}}\n", self.selector, body)
    }
}

/// One currently open block, as data inside the flowing state.
#[derive(Debug, Clone)]
struct Scope {
    selector: String,
    decls: Vec<(String, Value)>,
    /// This block is the body of a **plain-CSS at-rule** (`@function --a()`), or sits inside one, so
    /// its declarations are CSS text: an unbound `$x` stays written rather than raising
    /// `UndefinedVariable` (the three `*/sass_script` cases of `css/function.hrx`).
    /// Interpolation inside such a body is still SassScript (`lowercase/result/interpolation`:
    /// `#{1 + 1}` → `2`), so this suppresses fatality only. (`css/plain/function.hrx` states the same
    /// bodies through `@use "plain"`; those cases still fail for want of the P5 loader, not here.)
    plain_css: bool,
}

/// The program state the `Exec` kernel mutates as events flow: open blocks and the lexical
/// environment.
#[derive(Debug, Clone, Default)]
pub struct State {
    scopes: Vec<Scope>,
    env: Env,
}

impl State {
    /// Move one event into the state, returning the rule it settled, if any.
    ///
    /// **When a block's text settles** is what puts the output in document order without an
    /// ordering key and without re-sorting. Per `sass-spec/css/style_rule` `declaration/interleaved/**`:
    /// - opening a nested block first settles the enclosing block's **pending declaration run**
    ///   (`.a {b:c; .d{e:f}}` → `.a{b:c}` before `.a .d{e:f}`);
    /// - closing a block settles whatever it still holds (`.a {.b{c:d}; e:f}` → `.a{e:f}` **after**
    ///   `.a .b{c:d}`);
    /// - so one block split around children becomes several rules (`b:c`, then `g:h`).
    ///
    /// Errors reach the caller as `Err` so the kernel can put them on the stream's real `Err`
    /// channel; whether a given kind is fatal or stays a verbatim fallback is decided by
    /// `SassError::is_fatal` (see the `FATAL` set in the crate root and P0′.3b).
    pub fn step(&mut self, ev: Event) -> Result<Option<StyleRule>, SassError> {
        match ev {
            Event::Open(head) => {
                let parent_text = self
                    .scopes
                    .last()
                    .map(|s| s.selector.clone())
                    .unwrap_or_default();
                let text = self.interp_text(&head)?;
                let selector = combine(&parent_text, &text);
                let settled = self.settle_top();
                // A nested block inherits the plain-CSS context: it is still inside that body.
                let plain_css = self.in_plain_css() || plain_css_body(&text);
                self.scopes.push(Scope {
                    selector,
                    decls: Vec::new(),
                    plain_css,
                });
                Ok(settled)
            }
            Event::Decl { prop, value } => {
                // The left side of `$x: …` is a binding target: its name is taken verbatim and
                // is never interpolated, unlike an ordinary property name.
                let name = match prop.as_slice() {
                    [Tok::Var(var)] => format!("${var}"),
                    _ => self.interp_text(&prop)?,
                };
                match name.strip_prefix('$') {
                    // A binding updates the environment inside the flowing state; a later
                    // declaration reads the value this position has, so document order holds
                    // without any snapshot being carried around.
                    Some(var) => {
                        let bound = self.value_of(&value)?;
                        self.env.bind(var, bound);
                    }
                    None => {
                        let owned = self.decl_value(&name, &value, self.in_plain_css())?;
                        if let Some(top) = self.scopes.last_mut() {
                            top.decls.push((name, owned));
                        }
                    }
                }
                Ok(None)
            }
            Event::Close => Ok(Self::settled(self.scopes.pop())),
        }
    }

    /// Is the innermost open block a plain-CSS at-rule body?
    fn in_plain_css(&self) -> bool {
        self.scopes.last().is_some_and(|scope| scope.plain_css)
    }

    /// Settle the innermost block's pending declaration run **without** popping it: the block stays
    /// open for later declarations and children, but its text so far becomes a rule right now.
    fn settle_top(&mut self) -> Option<StyleRule> {
        let top = self.scopes.last_mut()?;
        if top.decls.is_empty() {
            return None;
        }
        Self::settled(Some(Scope {
            selector: top.selector.clone(),
            decls: std::mem::take(&mut top.decls),
            plain_css: top.plain_css,
        }))
    }

    /// Turn a finished or flushed scope into a rule; a declaration-less run, or one at the root
    /// (empty selector), settles to nothing.
    fn settled(scope: Option<Scope>) -> Option<StyleRule> {
        scope
            .filter(|s| !s.decls.is_empty() && !s.selector.is_empty())
            .map(|s| StyleRule {
                selector: s.selector,
                decls: s.decls,
            })
    }

    /// The declaration's owned value. A custom property (`--x: …`) and a value carrying a CSS
    /// native function (`var()`, `calc()`, `url()`, …) hold **raw CSS text** that Sass must not
    /// interpret; the first is stored verbatim, the others bail inside [`State::evaluated`].
    /// Any other value becomes a typed [`Value`] when — and only when — the run is
    /// unambiguous; a run that does not parse, or whose failure is not fatal, keeps its verbatim
    /// text. `plain` is the enclosing plain-CSS at-rule body, which keeps an unbound `$x` as text.
    fn decl_value(&self, prop: &str, value: &[Tok], plain: bool) -> Result<Value, SassError> {
        Ok(match prop {
            p if p.starts_with("--") => Value::from_text(render_raw(value)),
            _ => self
                .evaluated(value, plain)?
                .unwrap_or_else(|| Value::from_text(render(value))),
        })
    }

    fn value_of(&self, toks: &[Tok]) -> Result<Value, SassError> {
        // A binding (`$x: …`) is SassScript by definition, never plain-CSS text.
        Ok(self
            .evaluated(toks, false)?
            .unwrap_or_else(|| Value::from_text(render(toks))))
    }

    /// Fold a token run into a typed value. A lone number is **not** an expression yet — only
    /// an operation (or an interpolation) earns evaluation, so `w: 10px` keeps its exact source
    /// digits instead of being normalised to `10px` from a float.
    ///
    /// `Ok(None)` means "stays verbatim": the run did not parse as a Sass expression, or it failed
    /// inside plain CSS — the arguments of a **special** call (`expr::special_call`) or a declaration
    /// in a **plain-CSS at-rule body** (`plain`), where an unbound `$x` is just text
    /// (`css/functions/special/prefixed/lowercase::calc/script_like`, `css/function.hrx::lowercase/
    /// result/sass_script`). Anywhere else a fatal failure travels down the real `Err` channel — the
    /// decision is explicit per shape and per kind, never one blanket `.ok()` that also eats real
    /// errors.
    fn evaluated(&self, toks: &[Tok], plain: bool) -> Result<Option<Value>, SassError> {
        let arithmetic = expr::from_tokens(toks).and_then(|expr| match expr {
            Expr::Num(_, _) => None,
            other => Some(other),
        });
        let Some(expr) = arithmetic.or_else(|| expr::interpolated(toks)) else {
            return Ok(None);
        };
        match expr.eval(&self.env) {
            Ok(value) => Ok(Some(value)),
            Err(err) if err.is_fatal() && !(plain || expr::special_call(toks)) => Err(err),
            Err(_) => Ok(None),
        }
    }

    /// A head position (selector or property name) resolves **only `#{ … }`**: Sass has no bare
    /// variable reference there. This matters because a block head is not always a selector — an
    /// `@for`/`@each` header *binds* its `$i`, so looking a bare `$var` up here turned
    /// `sass-spec/directives/for/whitespace.hrx`'s `@for $i from … {}` into an "Undefined variable"
    /// (measured: that one over-reach accounted for ~47 of the 60 regressions when `UndefinedVariable`
    /// was made fatal). Lexemes must also never be read as numbers (`.2xl` is an identifier).
    fn interp_text(&self, toks: &[Tok]) -> Result<String, SassError> {
        toks.iter()
            .map(|tok| self.head_piece(tok))
            .collect::<Result<String, SassError>>()
            .map(|text| canon(&text))
    }

    /// One head token as text: only an interpolation is evaluated, every other lexeme stays exactly
    /// as written.
    fn head_piece(&self, tok: &Tok) -> Result<String, SassError> {
        let Tok::Interp(inner) = tok else {
            return Ok(token_text(tok));
        };
        let Some(expr) = expr::interpolated(inner) else {
            return Ok(render(inner));
        };
        match expr.eval(&self.env) {
            Ok(value) => Ok(value.to_css()),
            Err(err) if err.is_fatal() => Err(err),
            Err(_) => Ok(render(inner)),
        }
    }
}

/// Parent/child selector combination (Sass semantics): `&` replaces the parent, else descendant.
fn combine(parent: &str, child: &str) -> String {
    match parent {
        "" => child.to_string(),
        p if child.contains('&') => child.replace('&', p),
        p => format!("{p} {child}"),
    }
}

/// Whether a settled head text opens a **plain-CSS at-rule** body: `@function` (any case) naming a
/// custom ident that begins with `--` — `@function --a()`, `@function --a(--b <color>)`,
/// `@function --a() returns <ident>`, `@#{function} --a()` (`css/function.hrx`,
/// `css/plain/function.hrx`).
///
/// It reads the **resolved** head because a head may itself carry `#{…}` (the last case above), so the
/// interpolation must be folded before the shape is known. `@mixin --a` is deliberately not included:
/// the corpus makes it an error (`css/mixin.hrx::error/css/mixin`), not a passthrough body.
fn plain_css_body(text: &str) -> bool {
    let resolved = text.to_lowercase();
    let mut words = resolved.strip_prefix('@').unwrap_or("").split_whitespace();
    matches!(words.next(), Some("function"))
        && words.next().is_some_and(|name| name.starts_with("--"))
}
