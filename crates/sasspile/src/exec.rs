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
//! Nesting depth is data inside this value (`scopes`), never a call stack. The one recursion is `step`
//! re-entering itself over a `@for` body that [`crate::structure::MsgAcc`] cut out at the braces: it goes as
//! deep as the source nests loops and never as deep as a loop counts. No observable type appears here,
//! because `compile` (crate root) owns the one operator chain.

use crate::ErrorKind;
use crate::brackets;
use crate::env::Env;
use crate::expr::{self, Expr};
use crate::lexer::{Tok, render, render_raw, token_text, without_flags};
use crate::loops::{ForLoop, for_loop};
use crate::scope::{Scope, StyleRule};
use crate::selector::{Piece, PropName, Selector};
use crate::structure::{Event, as_block};
use crate::trace;
use crate::unit::Unit;
use crate::value::Value;

use crate::SassError;

/// The program state the `Exec` kernel mutates as events flow: open blocks and the lexical
/// environment.
#[derive(Debug, Clone, Default)]
pub struct State {
    scopes: Vec<Scope>,
    env: Env,
}

impl State {
    /// Move one event into the state, emitting every rule it settles — zero, one, or one per loop
    /// iteration.
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
    ///
    /// **0..N per event** is why this takes an emit sink rather than returning one rule: a `@for` settles
    /// one rule *per iteration* when its body holds a block (`directives/for/for.hrx::to_scope` asks for
    /// four `a { … }` rules from one event). The sink is the same carrier the `Lexer` and `Structurize`
    /// phases use ([`crate::lexer::Emit`], driven with `observer.next(..)`).
    ///
    /// What this phase no longer does is *find out where a loop ends* — [`crate::structure::MsgAcc`] owns the
    /// braces and hands the body up as one owned [`Event::For`], so there is no buffer here, no count of
    /// blocks still to close, and no loop-shaped scope.
    ///
    /// What it **still** does is said, not sold: expanding one `For` into N passes over its body is written
    /// here as an iteration over generated values plus a re-feed (`unroll` → `pass` → `step`) with one `Vec`
    /// clone per pass. design.md line 88 asks for `内核内 flat_map(一项 → body 子流)`, and **that verb exists**
    /// in rc.6: `flat_map` is `Map` + `MergeAll` and the inner observable carries its own `Context`
    /// (`docs/rxrust-source-analysis/05-operators.md`, which also warns that `merge_all` above one
    /// concurrent slot drops what a queued inner emits — so order would need `concat_map`). So this is an
    /// **unmade choice, not a framework limit**: taking it means `State` stops being a field of this observer
    /// and becomes the accumulator the stream carries — the standing 0′.5 item (`C::RcMut`/`C::RcCell`), or a
    /// `scan`-owned value. That is a phase-shape decision, reported as such in `tasks.md` 9.8 instead of being
    /// quietly kept or dressed up as a limitation.
    pub fn step(
        &mut self,
        ev: Event,
        emit: crate::lexer::Emit<'_, StyleRule>,
    ) -> Result<(), SassError> {
        match ev {
            Event::Open(head) => {
                let parent = self
                    .scopes
                    .last()
                    .map(|s| s.selector.clone())
                    .unwrap_or_default();
                let child = self.head_selector(&head, &parent.to_css())?;
                let text = child.to_css();
                // A nested block inherits the contexts of the block around it: it is still inside that body.
                let plain_css = self.in_plain_css() || plain_css_body(&text);
                let inert = self.in_inert() || (!plain_css && callable_declaration(&head));
                // A callable declaration is not a CSS block: opening one must not flush the enclosing rule's
                // pending declarations. A `@for` does not either, and it no longer reaches this arm at all —
                // the delimiter phase hands it up as `Event::For`, handled below.
                if !inert {
                    self.settle_top().map(emit);
                }
                self.scopes.push(Scope {
                    selector: child.nest(&parent),
                    decls: Vec::new(),
                    plain_css,
                    inert,
                });
                Ok(())
            }
            Event::Decl { prop, value } => {
                // Inside a callable declaration's body the event is inert: evaluating it would resolve the
                // callable's own parameters (`$v`) against an environment that only a call supplies, and
                // binding it would leak a parameter into the scope where the mixin was declared.
                if self.in_inert() {
                    return Ok(());
                }
                self.declare(&prop, &value)
            }
            Event::Close => {
                Self::settled(self.scopes.pop()).map(emit);
                Ok(())
            }
            Event::For { head, body } => self.unroll(head, body, emit),
        }
    }

    /// One `@for` in, one pass over its body per generated value out.
    ///
    /// The extent is decided upstream: [`crate::structure::MsgAcc`] cut the body out at the braces and sent it
    /// as one owned value, so this only binds and re-feeds. Each pass goes through the **same** [`State::step`]
    /// the source's own events take, which is why a body of declarations lands in the pending run of the
    /// block the loop sat in (`directives/for/for.hrx::inclusive_forward/scss` → one `a` rule holding
    /// `b: 1 … b: 5`) while a body holding a block settles one rule per iteration (`…::to_scope` → four
    /// `a { limit: 2; i: <n> }` rules, document order). A nested `@for` is just a `For` event inside the body,
    /// so the recursion follows the source's nesting and never the iteration count.
    ///
    /// A header the reader declines — a bound it will not read, a body that must stay inert CSS — takes the
    /// shape of an ordinary block (`structure::as_block`): opened with its header as the block's text, fed,
    /// closed. Nothing of it is dropped, and the choice is **not** a corpus answer: the header reading as a
    /// selector is why `directives/for/for.hrx::unit/same` and `…::error/from_float` stay red (1.3/P3.2 and
    /// P7 own those answers).
    ///
    /// One consequence is stated rather than hidden: a binding made inside the body writes the one flat layer
    /// of [`Env`], so it stays visible after the loop ends. Nothing measures that either way: the scoping
    /// fixtures (`sass-spec/spec/non_conformant/scope/for.hrx`, `spec/libsass/variable-scoping/`) sit in the
    /// directories `tests/hrx.rs` excludes by policy, so they are not among the 11,533 rows and state no
    /// reading here — which is why this is **this compiler's present shape, not a Sass law**. Block scoping is
    /// task 3.6.
    fn unroll(
        &mut self,
        head: Vec<Tok>,
        body: Vec<Event>,
        emit: crate::lexer::Emit<'_, StyleRule>,
    ) -> Result<(), SassError> {
        let walk = (!self.in_inert() && !self.in_plain_css())
            .then(|| for_loop(&head, &self.env))
            .transpose()?
            .flatten();
        let Some(ForLoop { var, walk }) = walk else {
            return as_block(head, body)
                .into_iter()
                .try_for_each(|ev| self.step(ev, emit));
        };
        // The loop variable is bound once per value and the body moves through the same step; the binding is
        // a declaration in the flowing environment, not a frame pushed onto a traversal.
        walk.values().try_for_each(|value| {
            self.env
                .bind(&var, Value::Number(value as f64, Unit::unitless()));
            self.pass(body.clone(), emit)
        })
    }

    /// One pass of a loop body: its events move into `step` in the order the source wrote them, and each rule
    /// they settle goes out through the sink as it is decided. This is the kernel's own 0..N emission, the
    /// vehicle [`crate::lexer::LexAcc::feed`] uses for a character that becomes several tokens — not a `Vec`
    /// of rules collected to be flattened downstream.
    fn pass(
        &mut self,
        body: Vec<Event>,
        emit: crate::lexer::Emit<'_, StyleRule>,
    ) -> Result<(), SassError> {
        body.into_iter().try_for_each(|ev| self.step(ev, emit))
    }

    /// One declaration where it appears: a binding updates the flowing environment, a property joins the
    /// innermost block's pending declaration run.
    fn declare(&mut self, prop: &[Tok], value: &[Tok]) -> Result<(), SassError> {
        // The left side of `$x: …` is a **binding target**, named by the variable itself; an ordinary
        // property name is a structured [`PropName`]. No `"${var}"` round-trip that a later
        // `strip_prefix('$')` would have to undo.
        let target = match prop.iter().find(|tok| !skipped_in_name(tok)) {
            Some(Tok::Var(var)) => Target::Variable(var.clone()),
            // `math.$pi: 0;` binds nothing: a built-in module's constant is read-only, and the oracle says so
            // by name (`core_functions/math/variables.hrx::error/assignment/*`). The test asks the module
            // table rather than matching the namespace text, so `upstream.$a:` — a member this compiler has
            // not got — keeps its old reading instead of gaining an error nobody stated.
            Some(Tok::MemberVar(module, name))
                if crate::builtins::module_value(module, name).is_some() =>
            {
                Target::ReadOnly
            }
            _ => Target::Property(PropName::from_pieces(&self.head_pieces(prop, "")?)),
        };
        match target {
            // A binding updates the environment inside the flowing state; a later declaration reads the
            // value this position has, so document order holds without any snapshot being carried around.
            Target::Variable(var) => {
                // `!default` / `!global` describe the binding, not its value, and never reach CSS
                // (`lexer::without_flags`); a declaration's `!important` keeps its text.
                let bound = self.value_of(&var, without_flags(value))?;
                self.env.bind(&var, bound);
            }
            Target::ReadOnly => {
                return Err(SassError::typed(
                    ErrorKind::ReadOnlyVariable,
                    "Cannot modify built-in variable.",
                ));
            }
            Target::Property(name) => {
                let owned = self.decl_value(&name, value, self.in_plain_css())?;
                if let Some(top) = self.scopes.last_mut() {
                    top.decls.push((name, owned));
                }
            }
        }
        Ok(())
    }

    /// Is the innermost open block a plain-CSS at-rule body?
    fn in_plain_css(&self) -> bool {
        self.scopes.last().is_some_and(|scope| scope.plain_css)
    }

    /// Is the innermost open block inside a callable declaration's body?
    fn in_inert(&self) -> bool {
        self.scopes.last().is_some_and(|scope| scope.inert)
    }

    /// The innermost open block's pending declaration run **without** popping it: the block stays
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
            inert: top.inert,
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

    /// The declaration's owned value. A custom property ([`PropName::is_custom`], decided once from the
    /// name's pieces) and a value carrying a CSS native function (`var()`, `calc()`, `url()`, …) hold
    /// **raw CSS text** that Sass must not interpret; the first is stored verbatim, the others bail
    /// inside [`State::evaluated`]. Any other value becomes a typed [`Value`] when — and only when — the
    /// run is unambiguous; a run that does not parse, or whose failure is not fatal, keeps its verbatim
    /// text. `plain` is the enclosing plain-CSS at-rule body, which keeps an unbound `$x` as text.
    fn decl_value(&self, prop: &PropName, value: &[Tok], plain: bool) -> Result<Value, SassError> {
        if prop.is_custom() {
            // A custom property's value is raw CSS by definition, so there is no read to trace.
            return Ok(Value::from_text(render_raw(value)));
        }
        // One span per value read: what the source said, what it became (or that it declined), and how
        // long the read took. This is the question a conformance round always asks about a line that came
        // out verbatim, and answering it used to mean adding a throwaway probe and rebuilding.
        let read = trace::ValueRead::begin(prop.as_str(), &self.scope_id(), &render(value));
        let outcome = self.evaluated(value, plain);
        read.record(&outcome);
        Ok(outcome?.unwrap_or_else(|| Value::from_text(render(value))))
    }

    /// A binding's value: SassScript by definition, never plain-CSS text. `var` names the target because
    /// the token run carries only the value, and a diagnostic that says `module: "1px"` is no diagnostic.
    fn value_of(&self, var: &str, toks: &[Tok]) -> Result<Value, SassError> {
        let read = trace::ValueRead::begin(var, &self.scope_id(), &render(toks));
        let outcome = self.evaluated(toks, false);
        read.record(&outcome);
        Ok(outcome?.unwrap_or_else(|| Value::from_text(render(toks))))
    }

    /// The innermost open block, as the text a diagnostic names it by. Root level has no scope, and the
    /// empty string says so honestly rather than inventing a selector.
    fn scope_id(&self) -> String {
        self.scopes
            .last()
            .map_or_else(String::new, |scope| scope.selector.to_css())
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
    ///
    /// A run that **passes named arguments** is excluded outright: its `$name:` is an argument label, so
    /// evaluating the run at all would resolve a reference that does not exist
    /// (`expr::named_argument`; measured as 1,611 cases given an invalid `Undefined variable`).
    fn evaluated(&self, toks: &[Tok], plain: bool) -> Result<Option<Value>, SassError> {
        let arithmetic = expr::from_tokens(toks, &self.env).and_then(|expr| match expr {
            Expr::Num(_, _) => None,
            other => Some(other),
        });
        let Some(expr) = arithmetic.or_else(|| expr::interpolated(toks, &self.env)) else {
            return Ok(None);
        };
        // A run that **passes named arguments** is evaluated only when it resolved to a call we have a
        // signature for. Its `$name:` is an argument label, so evaluating anything else would resolve a
        // reference that does not exist (`expr::named_argument`; measured as 1,611 cases given an invalid
        // `Undefined variable`). An unknown callee such as `color.hwb($hue: 180)` therefore keeps its text.
        if expr::named_argument(toks) && !matches!(expr, Expr::Call { .. }) {
            return Ok(None);
        }
        match expr.eval(&self.env) {
            // A number carrying a **compound unit** is calculation-only (`Unit::is_plain`), so the run keeps
            // its text instead of printing `2in/s` bare. The corpus never shows one alone: `math.div(6in, 3s)`
            // occurs only as `math.div(6in, 3s) * 1s` → `2in` (`core_functions/math/div.hrx::unit/incompatible`),
            // where the product cancels the unit; the one file that spells out the rejection
            // (`libsass-closed-issues/issue_783.hrx`) is commented out and excluded from the baseline, so no
            // error message is invented for it here — that belongs to P7.
            Ok(value) if !value.is_plain() => Ok(None),
            Ok(value) => Ok(Some(value)),
            Err(err) if err.is_fatal() && !(plain || expr::special_call(toks)) => Err(err),
            // A `$label:` naming no parameter of the signature is a refusal to answer, not something to
            // re-render: those labels name parameters rather than referencing variables, so the run keeps
            // its own text (`color.mix(#a, #b, $method: oklab)` must not be answered with a legacy mix).
            Err(err) if err.kind == ErrorKind::UnknownParameter => Ok(None),
            // A call the builtin **declined** is rendered the way an unknown callee's text is rendered —
            // through [`expr::interpolated`], which keeps the token run verbatim and resolves only its
            // variable reads. That is the shape the corpus asks for, and each half of it was measured:
            // rebuilding the call from evaluated arguments lost 40 rows (`rgb(18 52 none)` needs the spaces
            // inside an argument, `attr(c, %)` needs them before a comma, and `math.max($number1: 1, 2)`
            // needs an unbound label), while keeping the raw run unsubstituted left `saturate($c)` where
            // `saturate.hrx::css_overload/with_sass_var` asks for `saturate(1)`.
            Err(_) => Ok(decline(toks, &self.env)),
        }
    }

    /// A head position (selector or property name) as structured pieces: **only `#{ … }` is resolved**,
    /// every other lexeme stays exactly as written, and a literal `&` lexeme becomes a `Piece::Parent`.
    ///
    /// Sass has no bare variable reference in a head, and a head is not always a selector — an
    /// A `@for` header *binds* its `$i`, so looking a bare `$var` up here turned
    /// `sass-spec/directives/for/whitespace.hrx`'s `@for $i from … {}` into an "Undefined variable"
    /// (measured: that one over-reach accounted for ~47 of the 60 regressions when `UndefinedVariable`
    /// was made fatal). Lexemes must also never be read as numbers (`.2xl` is an identifier).
    ///
    /// `parent` is the enclosing scope's resolved text, needed only by `#{&}`.
    fn head_selector(&self, toks: &[Tok], parent: &str) -> Result<Selector, SassError> {
        self.head_pieces(toks, parent).map(Selector::from_pieces)
    }

    /// The head's pieces, in source order: mapping each lexeme to 0..N pieces and flattening keeps the
    /// order without ever building a string and searching it for `&`. Edges are trimmed at the piece
    /// level, because a head's surrounding newlines are layout, not part of the selector — the difference
    /// between `a.b` and `a .b` when this head is nested.
    fn head_pieces(&self, toks: &[Tok], parent: &str) -> Result<Vec<Piece>, SassError> {
        toks.iter()
            .map(|tok| self.pieces_of(tok, parent))
            .collect::<Result<Vec<Vec<Piece>>, SassError>>()
            .map(|runs| Selector::from_pieces(runs.concat()).trimmed().into_pieces())
    }

    fn pieces_of(&self, tok: &Tok, parent: &str) -> Result<Vec<Piece>, SassError> {
        match tok {
            Tok::Punct('&') => Ok(vec![Piece::Parent]),
            // `#{&}` is **not** the parent-reference position: the corpus substitutes the parent as text
            // and then nests normally, so `.test { .selector#{&} {} }` prints
            // `.test .selector.test`, not `.selector.test`
            // (`libsass-closed-issues/issue_1043.hrx`).
            Tok::Interp(inner) if matches!(inner.as_slice(), [Tok::Punct('&')]) => {
                Ok(vec![Piece::Text(parent.to_string())])
            }
            other => self.piece_text(other).map(|text| vec![Piece::Text(text)]),
        }
    }

    /// One head token as text: only an interpolation is evaluated, every other lexeme stays exactly
    /// as written.
    fn piece_text(&self, tok: &Tok) -> Result<String, SassError> {
        let Tok::Interp(inner) = tok else {
            return Ok(token_text(tok));
        };
        let Some(expr) = expr::interpolated(inner, &self.env) else {
            return Ok(render(inner));
        };
        match expr.eval(&self.env) {
            Ok(value) => Ok(value.to_css()),
            Err(err) if err.is_fatal() => Err(err),
            Err(_) => Ok(render(inner)),
        }
    }
}

/// What a call that its builtin declined becomes: the run rendered the way an unknown callee's text is,
/// i.e. through [`expr::interpolated`], which keeps every lexeme verbatim and resolves only its **variable
/// reads** — that is what `saturate($c)` needs (`saturate.hrx::css_overload/with_sass_var`).
///
/// A declined call is never an error to report: the corpus's own answer to a run this compiler cannot
/// produce is the run's own text. Propagating a failure from the re-render was tried and measured — it
/// raised the invalid-error readings 53 → 74 and turned `math.max($number1: 1, 2)` into
/// `Undefined variable.`, because the error came from **our** second pass, not from the Sass source. What
/// must never be re-rendered at all — an unknown parameter **label**, which names a parameter rather than
/// a variable — is matched by its own kind in the caller, so this path is only reached for runs whose
/// labels (if any) bound successfully.
fn decline(toks: &[Tok], env: &Env) -> Option<Value> {
    expr::interpolated(toks, env)
        .map(|run| run.eval(env))
        .and_then(Result::ok)
        .filter(Value::is_plain)
}

/// What carries no meaning into a property name: layout whitespace and a dropped silent comment. Both
/// were invisible to the old rendered-text form (`canon` trimmed the first, [`render`] drops the second),
/// so the binding test looks past them to the first lexeme that is actually part of the name.
fn skipped_in_name(tok: &Tok) -> bool {
    matches!(tok, Tok::Space(_) | Tok::Silent(_))
}

/// What a declaration's left side names: the environment's variable, or a CSS property.
enum Target {
    Variable(String),
    Property(PropName),
    /// the left side of a declaration named a built-in module's constant, which has no binding to update
    ReadOnly,
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

/// Whether a block head **declares a callable**: `@mixin name(…)` or `@function name(…)` (the keyword's
/// case is irrelevant, as for every Sass at-rule).
///
/// Decided on lexemes, like every other classifier here: the first meaningful lexeme must be the
/// keyword glued as one `Word`, and the name that follows must **not** begin with `--` — `@function --a()`
/// is a plain-CSS at-rule (`css/function.hrx`), whose body *is* emitted CSS, so mistaking it for a Sass
/// declaration would delete working rules.
fn callable_declaration(toks: &[Tok]) -> bool {
    let words = brackets::meaningful(toks);
    let [Tok::Word(keyword), Tok::Word(name), ..] = words.as_slice() else {
        return false;
    };
    let declares =
        keyword.eq_ignore_ascii_case("@mixin") || keyword.eq_ignore_ascii_case("@function");
    declares && !name.starts_with("--")
}
