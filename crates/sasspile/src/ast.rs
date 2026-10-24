//! Owned AST — the values that move down the pipeline as stream emissions.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    pub selector: String,
    pub decls: Vec<(String, String)>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Program {
    pub rules: Vec<Rule>,
}
