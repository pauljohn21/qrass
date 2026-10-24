//! Render: the mapping applied to each `Rule` in the pipeline. A rule with its own
//! declarations becomes one expanded block, emitted before its nested children
//! (document order); a rule with only children emits nothing for itself.

use crate::ast::Rule;

/// The expanded block for a rule's own declarations; empty when it has none.
fn own_block(rule: &Rule) -> String {
    if rule.decls.is_empty() {
        String::new()
    } else {
        let body = rule
            .decls
            .iter()
            .map(|(prop, value)| format!("  {prop}: {value};\n"))
            .collect::<String>();
        format!("{} {{\n{}}}\n", rule.selector, body)
    }
}

/// Depth-first: emit `rule`'s own block (skipped for at-rule/directive blocks,
/// which are transparent wrappers) then recurse into children.
pub(crate) fn write_rule(out: &mut String, rule: &Rule) {
    if rule.directive.is_none() {
        out.push_str(&own_block(rule));
    }
    rule.children.iter().for_each(|child| write_rule(out, child));
}
