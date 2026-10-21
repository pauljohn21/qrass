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
    /// Public `@mixin` names exported by the module as (exported-name,
    /// defining-name) pairs — they differ only through `@forward … as prefix-*`
    /// renames; `bind_alias` mirrors the registry under `{ns}.{exported}` while
    /// the declaration lookup stays keyed by the defining name.
    pub mixins: Vec<(String, String)>,
    /// Public `@function` names, same (exported, defining) contract as
    /// [`Module::mixins`].
    pub functions: Vec<(String, String)>,
    /// Normalized names of this module's public top-level `!default`
    /// declarations — the configuration surface a second `with (…)` would try to
    /// re-open. Only a module that has one makes re-configuring it an error
    /// (`directives/forward/error/with.hrx` `multi_configuration/through_forward`
    /// spells out that its target "only throws an error because it defines a
    /// configurable variable").
    pub configurable: Vec<String>,
    /// Canonical path of the file that **defines** this module — the fallback
    /// origin of its locally defined members (see [`Module::member_origins`]).
    pub origin_base: String,
    /// Provenance of `@forward`ed members: `(exported-name, origin-path of the
    /// file that actually defines the member, defining-module name)`, carried
    /// through each hop of a forward chain. Members absent here are locally
    /// defined and carry [`Module::origin_base`] as origin and their own name
    /// as defined. The wildcard-conflict ledger (OpenSpec M4) compares
    /// origins: two midstreams forwarding one upstream member share an origin
    /// (no conflict — element-plus chains), two local definitions of one name
    /// differ (`directives/use/error/member` goldens). The defined name keys
    /// the live `member_store` cell so `@forward … as prefix-*` renames still
    /// reach the base variable (`forward/member/as`).
    pub member_origins: Vec<(String, String, String)>,
}

/// A private member (`$-name`) is not re-exported to importers.
pub fn is_private(name: &str) -> bool {
    name.starts_with('-')
}

/// The `@forward`ed exports collected for one instantiation frame (task 6.3).
#[derive(Clone, Debug, Default)]
pub struct Forwarded {
    pub vars: Vec<(String, SassValue)>,
    pub mixins: Vec<(String, String)>,
    pub functions: Vec<(String, String)>,
    /// `(exported-name, origin-path, defining-module name)` provenance
    /// carried through this hop (see [`Module::member_origins`]).
    pub origins: Vec<(String, String, String)>,
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
    // A leading URL scheme drops out of the namespace derivation before the
    // path split: `@use "scheme:bar"` would bind `bar` if its load succeeded
    // (`use/error/load` `unknown_scheme` — the load error surfaces first,
    // not a namespace complaint), while `sass:` builtins reached through
    // `@use "sass:…"` resolve to their suffix (`color`, not `sass:color`).
    let rest = match url.split_once(':') {
        Some((scheme, tail))
            if !scheme.is_empty()
                && !tail.is_empty()
                && !url.contains('/')
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) =>
        {
            tail
        }
        _ => url,
    };
    let seg = rest.rsplit('/').next().unwrap_or(rest);
    let stem = seg.split_once('.').map(|(s, _)| s).unwrap_or(seg);
    stem.trim_start_matches(['.', '_']).to_string()
}
