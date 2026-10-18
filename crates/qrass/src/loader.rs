//! Source file loading and `@use`/`@forward` URL canonicalization (task 6.1).
//!
//! [`canonicalize`] resolves a module URL relative to an importer directory using
//! the Sass file-existence order (literal, `_`-prefixed partial, directory
//! `_index`/`index`, `.css` fallback) and reports whether the hit is a plain CSS
//! passthrough. [`Loader`] registers sources in [`Files`], deduplicating by
//! canonical path so every resolved module is read from disk exactly once.

use std::path::{Path, PathBuf};

use crate::error::{ErrorKind, SassError};
use crate::span::{FileId, Files};

/// Whether a resolved module is SCSS to compile or plain CSS to pass through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Scss,
    Css,
}

/// A canonicalized module reference: absolute path plus its kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    pub kind: SourceKind,
}

/// A loaded source file registered in [`Files`].
#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub file: FileId,
    pub kind: SourceKind,
}

impl Source {
    pub fn content<'a>(&self, files: &'a Files) -> Option<&'a str> {
        files.content(self.file)
    }
}

/// Build the ordered candidate paths for `url` resolved against `base_dir`.
/// Mirrors Sass: try the literal, the `_`-prefixed partial, directory index
/// files, then a `.css` passthrough — the first existing path wins.
pub fn candidate_paths(url: &str, base_dir: &Path) -> Vec<PathBuf> {
    // Absolute URL: resolve from the filesystem root, not the importer.
    let root = if url.starts_with('/') {
        PathBuf::from("/")
    } else {
        base_dir.to_path_buf()
    };
    let trimmed = url.trim_start_matches('/');
    let rel = Path::new(trimmed);
    let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext_matches = |e: &str| rel.extension().and_then(|x| x.to_str()) == Some(e);

    let mut out: Vec<PathBuf> = Vec::new();
    // 1. As given (already carries a recognized extension).
    if ext_matches("scss") || ext_matches("css") {
        out.push(root.join(rel));
    }
    // 2. `.scss`, then the `_`-prefixed partial of the final segment.
    let mut with = rel.to_path_buf();
    with.set_extension("scss");
    out.push(root.join(&with));
    if !stem.is_empty() {
        let mut partial = with.clone();
        partial.set_file_name(format!("_{stem}.scss"));
        out.push(root.join(partial));
    }
    // 3. Directory index files.
    for idx in ["_index.scss", "index.scss"] {
        out.push(root.join(rel).join(idx));
    }
    // 4. Plain CSS passthrough variants.
    let mut cssp = rel.to_path_buf();
    cssp.set_extension("css");
    out.push(root.join(&cssp));
    if !stem.is_empty() {
        let mut css_partial = cssp.clone();
        css_partial.set_file_name(format!("_{stem}.css"));
        out.push(root.join(css_partial));
    }
    out
}

/// Resolve `url` relative to `base_dir` to the first existing file on disk.
pub fn canonicalize(url: &str, base_dir: &Path) -> Result<Resolved, SassError> {
    canonicalize_with(url, base_dir, &[])
}

/// Resolve `url` like [`canonicalize`], but after the importer-relative search
/// fails, retry against each Sass **load path** in order (the `--load-path`
/// mechanism). A load path lets `@use "a/b/c"` resolve to `<load_path>/a/_b.scss`
///-style absolute-from-root URLs the way the Sass CLI and the sass-spec suite
/// expect, while the importer-relative form keeps priority.
/// The first existing candidate wins; when nothing resolves the error lists the
/// importer-relative candidates that were tried.
pub fn canonicalize_with(
    url: &str,
    base_dir: &Path,
    load_paths: &[PathBuf],
) -> Result<Resolved, SassError> {
    let base_candidates = candidate_paths(url, base_dir);
    if let Some(resolved) = first_existing(&base_candidates) {
        return Ok(resolved);
    }
    for lp in load_paths {
        if let Some(resolved) = first_existing(candidate_paths(url, lp)) {
            return Ok(resolved);
        }
    }
    let list = base_candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n   = ");
    Err(SassError::new(
        ErrorKind::Load,
        format!("Couldn't resolve @use/@forward URL \"{url}\""),
    )
    .note(format!("tried candidates:\n   = {list}")))
}

/// First candidate path that is an existing file, classified SCSS/CSS.
fn first_existing(candidates: impl IntoIterator<Item = impl AsRef<Path>>) -> Option<Resolved> {
    for candidate in candidates {
        let path = candidate.as_ref();
        if path.is_file() {
            let kind = if path.extension().and_then(|e| e.to_str()) == Some("css") {
                SourceKind::Css
            } else {
                SourceKind::Scss
            };
            return Some(Resolved {
                path: normalize_lexically(path),
                kind,
            });
        }
    }
    None
}

/// Collapse `.` and `..` segments lexically so that two URLs reaching the same
/// file through different importer directories (e.g. `common/var` from `src/`
/// versus `../common/var` from `src/mixins/`) yield an identical canonical path.
/// Without this the module cache key differs and a shared partial is instantiated
/// twice (duplicating its emitted CSS and loud comments). Requires the file to
/// exist, so it never resolves `..` past the true location.
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut stack: Vec<Component<'_>> = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Pop a preceding normal segment; keep a leading `..` when there is
                // none (relative path reaching above its base).
                if matches!(stack.last(), Some(Component::Normal(_))) {
                    stack.pop();
                } else {
                    stack.push(c);
                }
            }
            other => stack.push(other),
        }
    }
    stack.into_iter().collect()
}

/// Reads and registers sources, deduplicating by canonical path.
#[derive(Default)]
pub struct Loader {
    files: Files,
}


impl Loader {
    pub fn files(&self) -> &Files {
        &self.files
    }

    pub fn into_files(self) -> Files {
        self.files
    }

    /// Register an already-read source string under `name`.
    pub fn register(&mut self, name: String, content: String, kind: SourceKind) -> Source {
        let file = self.files.add(name, content);
        Source { file, kind }
    }

    /// Read `path` from disk and register it as file id 0 (the entry).
    pub fn load_entry(&mut self, path: &Path) -> Result<Source, SassError> {
        let content = std::fs::read_to_string(path)
            .map_err(|err| SassError::io(err, &path.display().to_string()))?;
        let name = path.display().to_string();
        tracing::info!(file = %name, len = content.len(), "loaded entry source");
        let kind = if path.extension().and_then(|e| e.to_str()) == Some("css") {
            SourceKind::Css
        } else {
            SourceKind::Scss
        };
        Ok(self.register(name, content, kind))
    }

    /// Load a resolved module, returning the existing id when already registered.
    pub fn load_resolved(&mut self, resolved: &Resolved) -> Result<Source, SassError> {
        let name = resolved.path.display().to_string();
        if let Some(file) = self.files.id_by_name(&name) {
            return Ok(Source {
                file,
                kind: resolved.kind,
            });
        }
        let content =
            std::fs::read_to_string(&resolved.path).map_err(|err| SassError::io(err, &name))?;
        tracing::info!(file = %name, kind = ?resolved.kind, len = content.len(), "loaded module source");
        Ok(self.register(name, content, resolved.kind))
    }
}

/// Read `path` from disk and register it as file id 0 (the entry). Free-function
/// shim kept for the single-entry skeleton pipeline.
pub fn load_entry(path: &Path, files: &mut Files) -> Result<Source, SassError> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| SassError::io(err, &path.display().to_string()))?;
    let name = path.display().to_string();
    tracing::info!(file = %name, len = content.len(), "loaded entry source");
    let file = files.add(name, content);
    Ok(Source {
        file,
        kind: SourceKind::Scss,
    })
}
