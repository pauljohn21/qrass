//! Render: the mapping function applied to each `Rule` emission in the pipeline.

use crate::ast::Rule;

pub(crate) fn rule_css(rule: &Rule) -> String {
    let body = rule
        .decls
        .iter()
        .map(|(prop, value)| format!("  {prop}: {value};\n"))
        .collect::<String>();
    format!("{} {{\n{}}}\n", rule.selector, body)
}
