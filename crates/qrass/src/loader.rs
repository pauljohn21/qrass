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
    // 2-4. Extension-adding and directory-index candidates. A URL that already
    // carries a recognized extension never grows one and never resolves through
    // its own directory index: `@use "dir.scss"` must NOT hit `dir.scss/index.scss`
    // (`use/error/load` `dir_dot_scss` demands `Can't find stylesheet to import.`
    // while the directory exists with an index inside).
    if !ext_matches("scss") && !ext_matches("css") {
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
        out.extend(["_index.scss", "index.scss"].iter().map(|idx| root.join(rel).join(idx)));
        // 4. Plain CSS passthrough variants.
        let mut cssp = rel.to_path_buf();
        cssp.set_extension("css");
        out.push(root.join(&cssp));
        if !stem.is_empty() {
            let mut css_partial = cssp.clone();
            css_partial.set_file_name(format!("_{stem}.css"));
            out.push(root.join(css_partial));
        }
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
/// The importer-relative search and its load-path retries share this order:
/// the first root with a hit decides; nothing found raises the goldens'
/// `Can't find stylesheet to import.` (`use/error/load` `missing`), and two
/// hits in one root — partial vs non-partial, `_index` vs `index` — are
/// ambiguous: `It's not clear which file to import. Found:` lists the
/// candidate spellings relative to that root, sorted
/// (`use/error/load` `conflict/partial`, `conflict/index`).
pub fn canonicalize_with(
    url: &str,
    base_dir: &Path,
    load_paths: &[PathBuf],
) -> Result<Resolved, SassError> {
    for root in std::iter::once(base_dir).chain(load_paths.iter().map(|p| p.as_path())) {
        let candidates = candidate_paths(url, root);
        let existing = |p: &PathBuf| crate::vfs::exists(p) || p.is_file();
        // Ambiguity is *within* one level only, and later candidate groups
        // are consulted exclusively when earlier ones miss everything:
        // SCSS-level vs SCSS-index (`use/load` `precedence/normal_before_index`
        // resolves `dir.scss` even with `dir/index.scss` present) and the CSS
        // pair likewise. A partial vs non-partial (or `_index` vs `index`)
        // pair inside one level is `use/error/load` `conflict/*`.
        let report = |group: Vec<&PathBuf>| -> SassError {
            let mut names: Vec<String> = group
                .iter()
                .map(|p| {
                    p.strip_prefix(root)
                        .unwrap_or(p.as_path())
                        .display()
                        .to_string()
                })
                .collect();
            names.sort();
            SassError::new(
                ErrorKind::Load,
                format!(
                    "It's not clear which file to import. Found:\n  {}",
                    names.join("\n  ")
                ),
            )
        };
        let is_scss = |p: &&PathBuf| {
            existing(p) && p.extension().is_some_and(|e| e == "scss")
        };
        let split = candidates.iter().position(|p| {
            p.file_name().is_some_and(|f| f == "_index.scss" || f == "index.scss")
        });
        let (level, index): (&[_], &[_]) = match split {
            Some(i) => candidates.split_at(i),
            None => (candidates.as_slice(), &[]),
        };
        let scss: Vec<&PathBuf> = level.iter().filter(is_scss).collect();
        match scss.len() {
            0 => {}
            1 => {
                // A lone SCSS hit decides this root even when it is not
                // loadable (an `other.import.scss` the @use search cannot use
                // — `use/load` `precedence/ignores_import_only`); the next
                // root must not resurrect earlier-priority spellings.
                let path = scss[0];
                return Ok(Resolved {
                    path: normalize_lexically(path),
                    kind: SourceKind::Scss,
                });
            }
            _ => return Err(report(scss)),
        }
        let scss_index: Vec<&PathBuf> = index.iter().filter(is_scss).collect();
        match scss_index.len() {
            0 => {}
            1 => {
                let path = scss_index[0];
                return Ok(Resolved {
                    path: normalize_lexically(path),
                    kind: SourceKind::Scss,
                });
            }
            _ => return Err(report(scss_index)),
        }
        let is_css =
            |p: &&PathBuf| existing(p) && p.extension().is_some_and(|e| e == "css");
        let css: Vec<&PathBuf> = level.iter().filter(is_css).collect();
        match css.len() {
            0 => {}
            1 => {
                let path = css[0];
                return Ok(Resolved {
                    path: normalize_lexically(path),
                    kind: SourceKind::Css,
                });
            }
            _ => return Err(report(css)),
        }
        let css_index: Vec<&PathBuf> = index.iter().filter(is_css).collect();
        match css_index.len() {
            0 => continue,
            1 => {
                let path = css_index[0];
                return Ok(Resolved {
                    path: normalize_lexically(path),
                    kind: SourceKind::Css,
                });
            }
            _ => return Err(report(css_index)),
        }
    }
    let tried: Vec<String> = candidate_paths(url, base_dir)
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    Err(SassError::new(
        ErrorKind::Load,
        "Can't find stylesheet to import.",
    )
    // The golden dictates the bare first line; the URL and the probed
    // candidates ride a note so CLI users (and the L2 fan-out tests, which
    // prove the base directory through them) can still tell *which* branch
    // failed and where it looked.
    .note(format!(
        "url: {url}\ntried candidates:\n   = {}",
        tried.join("\n   = ")
    )))
}



/// Collapse `.` and `..` segments lexically so that two URLs reaching the same
/// file through different importer directories (e.g. `common/var` from `src/`
/// versus `../common/var` from `src/mixins/`) yield an identical canonical path.
/// Without this the module cache key differs and a shared partial is instantiated
/// twice (duplicating its emitted CSS and loud comments). Requires the file to
/// exist, so it never resolves `..` past the true location.
pub(crate) fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    // fold the component stream: `.` drops, `..` pops a preceding normal segment
    // and is kept when there is none (a relative path reaching above its base)
    let stack = path.components().fold(Vec::new(), |mut stack: Vec<Component<'_>>, c| {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                match matches!(stack.last(), Some(Component::Normal(_))) {
                    true => {
                        stack.pop();
                    }
                    false => stack.push(c),
                }
            }
            other => stack.push(other),
        }
        stack
    });
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
        let name = path.display().to_string();
        let content = crate::vfs::read_to_string(path, &name)?;
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
        let content = crate::vfs::read_to_string(&resolved.path, &name)?;
        tracing::info!(file = %name, kind = ?resolved.kind, len = content.len(), "loaded module source");
        Ok(self.register(name, content, resolved.kind))
    }
}

/// Read `path` from disk and register it as file id 0 (the entry). Free-function
/// shim kept for the single-entry skeleton pipeline.
pub fn load_entry(path: &Path, files: &mut Files) -> Result<Source, SassError> {
    let name = path.display().to_string();
    let content = crate::vfs::read_to_string(path, &name)?;
    tracing::info!(file = %name, len = content.len(), "loaded entry source");
    let file = files.add(name, content);
    Ok(Source {
        file,
        kind: SourceKind::Scss,
    })
}
