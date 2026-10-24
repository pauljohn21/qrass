//! Owned AST — the values that move down the pipeline as stream emissions. A rule
//! owns a resolved (ancestor-combined) selector, its own declarations, and its
//! nested child rules; the whole tree is owned and moved, never borrowed.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    pub selector: String,
    pub decls: Vec<(String, String)>,
    pub children: Vec<Rule>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Program {
    pub rules: Vec<Rule>,
    /// Top-level `name: value` statements outside any block — variable bindings
    /// (`$x: …`) and, later, root-level at-rule config. They seed the eval Env and
    /// are never emitted as CSS declarations themselves.
    pub globals: Vec<(String, String)>,
}
