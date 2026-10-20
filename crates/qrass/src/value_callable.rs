//! First-class callable payloads for [`SassValue`] (round-73 W1/W2),
//! split out of `value.rs` for the source-size budget; re-exported there so
//! `crate::value::{FnRef, FnKind, MixRef, MixKind}` keep resolving. The
//! serialization and equality semantics are pinned by the
//! `core_functions/meta/get_function/*`, `inspect/*` and `values/mixins.hrx`
//! goldens — never by dart internals.

/// Payload of a first-class function value. `module` anchors the defining
/// module for the `meta.call` closure switch; `kind` distinguishes the user
/// definition (locked at `get-function` time — the `same_module/redefined`
/// golden: a later redefinition must not affect a stored reference), a builtin
/// lookup, and a `$css: true` reference whose call re-emits the plain-CSS
/// spelling (`plain_css` golden: `round(0.6)`).
#[derive(Clone, Debug)]
pub struct FnRef {
    pub name: String,
    pub module: Option<String>,
    pub kind: FnKind,
}

/// What a function value points at.
#[derive(Clone, Debug)]
pub enum FnKind {
    /// A user `@function`; `Rc` identity backs dart's reference equality, and
    /// the captured `env`/`owner` are the definition-time closure snapshot
    /// `meta.call` re-enters (mirroring the by-name call path). Locked at
    /// `get-function` time — the `same_module/redefined` golden.
    User {
        decl: std::rc::Rc<crate::ast::FunctionDecl>,
        env: crate::env::Env,
        owner: String,
    },
    /// A registered builtin (global or module-qualified).
    Builtin { namespace: Option<String> },
    /// A `$css: true` lookup (CSS filter passthrough form).
    Css,
}

/// Payload of a first-class mixin value.
#[derive(Clone, Debug)]
pub struct MixRef {
    pub name: String,
    pub module: Option<String>,
    pub kind: MixKind,
}

/// What a mixin value points at. `owner` is the defining module for
/// `meta.apply`'s closure switch (mirrors the by-name include path).
#[derive(Clone, Debug)]
pub enum MixKind {
    User {
        decl: std::rc::Rc<crate::ast::MixinDecl>,
        owner: String,
    },
    /// A module-level builtin mixin (`meta.get-mixin(load-css, meta)`).
    Builtin { namespace: String },
}

/// Equality of two function values per the `get_function/equality` goldens.
pub(crate) fn fn_kind_eq(a: &FnRef, b: &FnRef) -> bool {
    match (&a.kind, &b.kind) {
        (FnKind::Builtin { namespace: x }, FnKind::Builtin { namespace: y }) => {
            a.name == b.name && x == y
        }
        (FnKind::User { decl: x, .. }, FnKind::User { decl: y, .. }) => std::rc::Rc::ptr_eq(x, y),
        (FnKind::Css, FnKind::Css) => a.name == b.name && a.module == b.module,
        _ => false,
    }
}

/// Equality of two mixin values (same shape as functions).
pub(crate) fn mix_kind_eq(a: &MixRef, b: &MixRef) -> bool {
    match (&a.kind, &b.kind) {
        (MixKind::Builtin { namespace: x }, MixKind::Builtin { namespace: y }) => {
            a.name == b.name && x == y
        }
        (MixKind::User { decl: x, .. }, MixKind::User { decl: y, .. }) => std::rc::Rc::ptr_eq(x, y),
        _ => false,
    }
}
