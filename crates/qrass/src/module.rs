//! Module instantiation (task 6.2 / 6.3). A [`Module`] is one instantiated
//! `@use`/`@forward` unit: the CSS it emitted plus the variables it exports.
//! Instantiation is cached by canonical path (in [`crate::eval::EvalState`]) so a
//! diamond dependency evaluates exactly once; its CSS is published only at that
//! first instantiation. Modern-module privacy (design D3): a member whose name
//! starts with `-` (i.e. `$-foo`) stays inside its own module.

use crate::value::SassValue;

/// An instantiated module: emitted CSS chunks + exported variable bindings.
#[derive(Clone, Debug, Default)]
pub struct Module {
    pub css: Vec<String>,
    /// Non-private variable bindings visible to importers.
    pub vars: Vec<(String, SassValue)>,
    /// Public `@mixin` names defined by the module (the declarations themselves
    /// live in the eval registry under their bare names; `@use` binds a
    /// `{ns}.{name}` alias so `@include ns.name(…)` resolves).
    pub mixins: Vec<String>,
    /// Public `@function` names, same aliasing contract as [`Module::mixins`].
    pub functions: Vec<String>,
}

/// A private member (`$-name`) is not re-exported to importers.
pub fn is_private(name: &str) -> bool {
    name.starts_with('-')
}

/// Filter a module's global bindings down to its public exports.
pub fn export_vars(bindings: Vec<(String, SassValue)>) -> Vec<(String, SassValue)> {
    bindings
        .into_iter()
        .filter(|(name, _)| !is_private(name))
        .collect()
}

/// Default `@use` namespace = the URL's final path segment with any file
/// extension and leading `.`/`_` stripped (`src/vars` → `vars`, `_a.scss` → `a`).
pub fn default_namespace(url: &str) -> String {
    let seg = url.rsplit('/').next().unwrap_or(url);
    let stem = seg.split_once('.').map(|(s, _)| s).unwrap_or(seg);
    stem.trim_start_matches(['.', '_']).to_string()
}
