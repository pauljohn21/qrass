//! Render: flatten the rule tree into CSS text with an OWNED explicit stack
//! (`std::iter::from_fn`) — no recursion, no loop keywords. Pre-order (a rule's own
//! block, then its children) preserves document order; at-rule/directive blocks are
//! transparent wrappers (their own block is skipped, children still emitted).

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

pub(crate) fn to_css(rules: Vec<Rule>) -> String {
    let mut stack: Vec<Rule> = rules.into_iter().rev().collect();
    std::iter::from_fn(move || {
        stack.pop().map(|rule| {
            let block = if rule.directive.is_none() { own_block(&rule) } else { String::new() };
            rule.children.into_iter().rev().for_each(|child| stack.push(child));
            block
        })
    })
    .collect()
}
