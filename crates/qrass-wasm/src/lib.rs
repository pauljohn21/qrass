//! Browser-facing facade over `qrass::compile_string` — the wasm integration
//! layer of the Local-context contract (design D1).
//!
//! The core crate stays pure Rust; this crate only adapts it to the JS boundary.
//! Every export comes in two layers: a plain Rust `*_text` function whose error
//! channel is the message `String` (callable and assertable from host `tests/`),
//! and a `#[wasm_bindgen]` wrapper that turns that message into a thrown
//! `JsError`. Multi-file `@use` runs against the [`qrass::vfs`] in-memory source
//! registry: JS calls [`register_source`] for every partial before compiling,
//! and the loader consults that registry instead of `std::fs` (which a browser
//! does not have).

use qrass::{Options, compile_string};
use wasm_bindgen::prelude::*;

/// Register an in-memory `.scss`/`.css` source so a later `compile_scss` can
/// resolve `@use`/`@forward` URLs against it without a file system. Paths are
/// matched after lexical normalization, the same canonical form the module
/// cache uses — register the partials of a tree, then compile any entry in it.
#[wasm_bindgen]
pub fn register_source(path: &str, content: &str) {
    qrass::vfs::register(path, content);
}

/// Drop every registered source (fresh state between independent compilations).
#[wasm_bindgen]
pub fn clear_sources() {
    qrass::vfs::clear();
}

/// Compile one in-memory SCSS source to expanded CSS (pure-Rust layer).
///
/// `name` is the display name **and** the base a relative `@use` resolves
/// against; sources registered via [`register_source`] (or the host
/// `qrass::vfs::register`) satisfy those URLs without a file system.
pub fn compile_scss_text(name: &str, content: &str) -> Result<String, String> {
    compile_string(name.to_string(), content.to_string(), &Options::default())
        .map_err(|err| err.to_string())
}

/// Style-checked variant (pure-Rust layer): only `expanded` exists, the same
/// single style the CLI supports; anything else is rejected as unsupported.
pub fn compile_scss_with_style_text(
    name: &str,
    content: &str,
    style: &str,
) -> Result<String, String> {
    if style != "expanded" {
        return Err(format!(
            "style '{style}' is not supported (only 'expanded')"
        ));
    }
    compile_scss_text(name, content)
}

/// JS export: compile one in-memory SCSS source, throwing the error text as a
/// JS exception on failure.
#[wasm_bindgen]
pub fn compile_scss(name: &str, content: &str) -> Result<String, JsError> {
    compile_scss_text(name, content).map_err(|msg| JsError::new(&msg))
}

/// JS export: compile with an explicit output style knob.
#[wasm_bindgen]
pub fn compile_scss_with_style(name: &str, content: &str, style: &str) -> Result<String, JsError> {
    compile_scss_with_style_text(name, content, style).map_err(|msg| JsError::new(&msg))
}
