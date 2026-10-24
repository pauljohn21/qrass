//! Owned AST — the values that move down the pipeline as stream emissions. A rule
//! owns a resolved (ancestor-combined) selector, its declarations, and its nested
//! child rules. An at-rule block (`@if`/`@else`/`@media`/…) carries `directive` =
//! Some(head): it contributes NO selector to its children (they inherit the parent)
//! and is not emitted as a plain selector block. The whole tree is owned and moved.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    pub selector: String,
    pub decls: Vec<(String, String)>,
    pub children: Vec<Rule>,
    /// `Some("@if …" / "@else" / "@media …" / …)` for an at-rule block head.
    pub directive: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Program {
    pub rules: Vec<Rule>,
    /// Top-level `name: value` statements outside any block — variable bindings
    /// (`$x: …`) and, later, root-level at-rule config. They seed the eval Env and
    /// are never emitted as CSS declarations themselves.
    pub globals: Vec<(String, String)>,
}
