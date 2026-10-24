//! Render a resolved leaf to an expanded CSS block (pure per-leaf `map`).

use crate::parse::Leaf;

pub(crate) fn leaf_block(leaf: &Leaf) -> String {
    let body = leaf
        .decls
        .iter()
        .map(|(prop, value)| format!("  {prop}: {value};\n"))
        .collect::<String>();
    format!("{} {{\n{}}}\n", leaf.selector, body)
}
