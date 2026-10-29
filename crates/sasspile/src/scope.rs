//! The two **owned data values** the `Exec` kernel carries: one open block and one settled rule.
//!
//! They live apart from [`crate::exec`]'s forward step so that what execution *holds* is readable on its
//! own: a block is a selector plus its pending declaration run plus whatever the block *is* (plain CSS or a
//! callable body); a rule is a selector plus one declaration run. Nesting depth is this data
//! (`State::scopes` in [`crate::exec`]), never a call stack, and no observable type appears here.
//!
//! A block holds **no loop state**. A `@for` is cut out by the phase that owns the braces
//! ([`crate::structure::MsgAcc`]) and reaches execution as one finished event, so nothing here grows fields to
//! buffer a body or count blocks that still have to close.

use crate::selector::{PropName, Selector};
use crate::value::Value;

/// A resolved CSS rule: one **declaration run** flushed from a block. A block can yield several
/// rules (`sass-spec` `css/style_rule` `declaration/interleaved/around_style_rule`), so this is not
/// "the block's whole body". One `@for` yields one rule **per iteration** when its body holds a block
/// (`sass-spec` `directives/for/for.hrx::to_scope`), which is the same fact stated in the other direction.
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub decls: Vec<(PropName, Value)>,
}

impl StyleRule {
    /// Serialise the rule to an expanded CSS block. [`Value::to_css`] stays the one place a
    /// value becomes text, and [`Selector::to_css`] the one place a head does.
    pub fn to_css(&self) -> String {
        let body = self
            .decls
            .iter()
            .map(|(prop, value)| format!("  {}: {};\n", prop.as_str(), value.to_css()))
            .collect::<String>();
        format!("{} {{\n{}}}\n", self.selector.to_css(), body)
    }
}

/// One currently open block, as data inside the flowing state.
#[derive(Debug, Clone)]
pub(crate) struct Scope {
    pub(crate) selector: Selector,
    pub(crate) decls: Vec<(PropName, Value)>,
    /// This block is the body of a **plain-CSS at-rule** (`@function --a()`), or sits inside one, so
    /// its declarations are CSS text: an unbound `$x` stays written rather than raising
    /// `UndefinedVariable` (the three `*/sass_script` cases of `css/function.hrx`).
    /// Interpolation inside such a body is still SassScript (`lowercase/result/interpolation`:
    /// `#{1 + 1}` → `2`), so this suppresses fatality only. (`css/plain/function.hrx` states the same
    /// bodies through `@use "plain"`; those cases still fail for want of the P5 loader, not here.)
    pub(crate) plain_css: bool,
    /// This block is the body of a **callable declaration** (`@mixin name(…)` / `@function name(…)`),
    /// which Sass does not *execute* at the place it is written: nothing is evaluated, nothing is bound
    /// into the surrounding environment, and no rule comes out. Invoking it (`@include` / a call) is what
    /// would evaluate the body, and that is 4.1/4.2 — until then treating the body as ordinary CSS raises
    /// an `Undefined variable` for its own parameter (measured as 31 in-scope cases, e.g.
    /// `core_functions/meta/get_mixin/same_module::dash_insensitive/*`).
    pub(crate) inert: bool,
}
