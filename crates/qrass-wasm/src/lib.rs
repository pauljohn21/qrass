//! Browser-facing facade over `qrass::compile_string` — the wasm integration
//! layer of the Local-context contract (design D1).
//!
//! The core crate stays pure Rust; this crate only adapts it to the JS boundary.
//! Every export comes in two layers: a plain Rust `*_text` function whose error
//! channel is the message `String` (callable and assertable from host `tests/`),
//! and a `#[wasm_bindgen]` wrapper that turns that message into a thrown
//! `JsError`. Multi-file `@use` against a *virtual* file system (sources
//! supplied from JS instead of `std::fs`) is the documented follow-up; until
//! then the entry compiles a single in-memory source plus the `sass:` built-in
//! namespaces, which never touch the file system.

use qrass::{Options, compile_string};
use wasm_bindgen::prelude::*;

/// Compile one in-memory SCSS source to expanded CSS (pure-Rust layer).
///
/// `name` is the display name **and** the base a relative `@use` would resolve
/// against; on the wasm target file-system-backed `@use` is unavailable, so
/// relative URLs report a load error while `sass:` namespaces work fully.
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
