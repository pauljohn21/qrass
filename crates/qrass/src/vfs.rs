//! In-memory source registry — the file-system-free half of the wasm contract.
//!
//! On `wasm32-unknown-unknown` there is no usable `std::fs`, so the browser
//! integration registers every `.scss`/`.css` source from JS (through the
//! `qrass-wasm` facade) and the load path consults this registry **before**
//! touching disk. Keys are compared with the same lexical normalization the
//! canonicalizer applies (`.`/`..` collapsed), so a candidate reached through
//! different importer directories hits the same entry, and a registered source
//! participates in Sass resolution order (literal → `_` partial → `_index`)
//! exactly like a file.
//!
//! The store is a `thread_local` owned map — no `Arc<Mutex<_>>`, matching the
//! single-threaded Local-context pipeline. Host callers may use it too (virtual
//! fixtures, preloaded caches); an empty registry is byte-for-byte today's
//! behaviour: every probe falls through to `std::fs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use crate::error::SassError;
use crate::loader::normalize_lexically;

thread_local! {
    static SOURCES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Register an in-memory source under `path`. Idempotent: a second `register`
/// for the same canonical path replaces the earlier text.
pub fn register(path: impl AsRef<Path>, content: impl Into<String>) {
    SOURCES.with(|s| {
        s.borrow_mut().insert(key(path.as_ref()), content.into());
    });
}

/// Drop every registered source — fresh fixture state between tests or
/// between independent browser compilations.
pub fn clear() {
    SOURCES.with(|s| s.borrow_mut().clear());
}

/// Whether `path` resolves to a registered source (probe used by the
/// candidate-search exactly where `Path::is_file` would ask the disk).
pub(crate) fn exists(path: &Path) -> bool {
    SOURCES.with(|s| s.borrow().contains_key(&key(path)))
}

/// Virtual-first read: a registry hit wins; otherwise fall through to
/// `std::fs` (native and `wasm32-wasip1` have a real file system; on
/// `wasm32-unknown-unknown` the fs stub reports `Unsupported` at runtime,
/// which surfaces as the usual load error).
pub(crate) fn read_to_string(path: &Path, name: &str) -> Result<String, SassError> {
    let hit = SOURCES.with(|s| s.borrow().get(&key(path)).cloned());
    match hit {
        Some(text) => Ok(text),
        None => std::fs::read_to_string(path).map_err(|err| SassError::io(err, name)),
    }
}

/// Registry key: the lexically normalized display string, matching the
/// canonical form `first_existing` hands downstream as the module identity.
fn key(path: &Path) -> String {
    normalize_lexically(path).display().to_string()
}
