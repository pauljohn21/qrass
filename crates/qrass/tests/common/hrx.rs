//! `.hrx` (HRX) spec-file parser (task 11.1).
//!
//! sass-spec stores each behavioral test group as a single `.hrx` document: a
//! flat text file concatenating one or more *test cases*. The on-disk grammar
//! (mirrored from `sass-spec/lib/spec-directory/hrx.ts`) is:
//!
//! * a *file entry* is a header line `<===> <relative/path>` followed by its
//!   contents up to the next header (contents keep their newlines);
//! * test cases inside one document are separated by the literal divider
//!   [`SECTION_SEPARATOR`] — a bare `<===>` header whose body is a line of 80
//!   `=` characters.
//!
//! This module turns that text into structured [`Case`]es: every case exposes
//! its raw file list plus the handful of well-known members the runner needs —
//! the `input.scss`/`input.sass` entry, an optional `output.css`, an optional
//! bare `error` expectation, and any sibling `_*.scss` partials the input
//! `@use`s/`@import`s (which the runner must materialize alongside the input).
//!
//! Pure parsing only — no compilation, no filesystem, no comparison — so it can
//! be unit-tested deterministically against either an inline document or a real
//! file checked out under `sass-spec/`.

/// The exact byte sequence joining two test cases within one `.hrx` document:
/// a newline, a bare `<===>` divider header, the 80-`=` marker line, and its
/// trailing newline. Reproduced verbatim from sass-spec's writer.
pub const SECTION_SEPARATOR: &str = concat!(
    "\n<===>\n",
    "================================================================================\n"
);

/// One file entry: its path relative to the `.hrx` directory, and its contents
/// (newlines preserved).
pub type Entry = (String, String);

/// A parsed test case: the member files plus the classified accessors the
/// runner consumes.
#[derive(Debug, Clone)]
pub struct Case {
    /// Every file entry in source order (input, partials, output, error, …).
    pub files: Vec<Entry>,
    /// Directory portion shared by the case's files (e.g. `declaration/alone`),
    /// empty when the input sits at the `.hrx` root. Used to key allowlist and
    /// temp-dir layout.
    pub dir: String,
    /// The `input.scss`/`input.sass` entry, if present.
    pub input: Option<Entry>,
    /// Whether the input is indented (`.sass`) syntax rather than `.scss`.
    pub indented: bool,
    /// The `output.css` contents, if present.
    pub output: Option<String>,
    /// The bare `error` expectation contents, if present.
    pub error: Option<String>,
    /// The `options.yml` contents, if present (drives legacy/impl skips).
    pub options: Option<String>,
}

impl Case {
    /// Build a case from its raw file entries, classifying the well-known
    /// members and deriving the shared case directory from the input path.
    pub fn from_entries(files: Vec<Entry>) -> Self {
        let input = files
            .iter()
            .find(|(p, _)| p.ends_with("/input.scss") || p == "input.scss" ||
                p.ends_with("/input.sass") || p == "input.sass")
            .cloned();
        let indented = input
            .as_ref()
            .map(|(p, _)| p.ends_with(".sass"))
            .unwrap_or(false);
        let dir = input
            .as_ref()
            .map(|(p, _)| match p.rfind('/') {
                Some(i) => p[..i].to_string(),
                None => String::new(),
            })
            .unwrap_or_default();
        let output = lookup(&files, "output.css");
        let error = lookup(&files, "error");
        let options = lookup(&files, "options.yml");
        Self {
            files,
            dir,
            input,
            indented,
            output,
            error,
            options,
        }
    }

    /// A stable identifier for allowlisting: `<hrx-rel-dir>/<case-dir>` joined.
    pub fn case_dir(&self) -> &str {
        &self.dir
    }
}

/// First entry whose path basename equals `name`, returning its contents.
fn lookup(files: &[Entry], name: &str) -> Option<String> {
    files
        .iter()
        .find(|(p, _)| p.rsplit('/').next().unwrap_or(p) == name)
        .map(|(_, c)| c.clone())
}

/// Parse a `.hrx` document into its test cases. Empty / headerless fragments
/// (which the writer can emit around separators) are dropped.
///
/// Follows sass-spec's *accumulating file set*: source files (partials like
/// `_utils.scss`) declared in any section stay in scope for all later sections
/// of the same document — a section that only declares fixtures contributes no
/// case but seeds the shared files, and a later input section sees both. Each
/// case's `files` are the inherited sources plus the section's own entries, so
/// expectation lookups (`output.css`, `error`) never leak across cases.
pub fn parse(text: &str) -> Vec<Case> {
    let mut inherited: Vec<Entry> = Vec::new();
    text.split(SECTION_SEPARATOR)
        .map(parse_files)
        .filter(|files| !files.is_empty())
        .map(|files| {
            for (path, contents) in &files {
                if is_source_file(path) {
                    upsert(&mut inherited, path.clone(), contents.clone());
                }
            }
            // The section's own entries win over inherited ones at the same
            // path (a later declaration redefines the file), so they come first.
            let mut merged = files.clone();
            for (path, contents) in inherited.iter() {
                if !merged.iter().any(|(p, _)| p == path) {
                    merged.push((path.clone(), contents.clone()));
                }
            }
            Case::from_entries(merged)
        })
        .collect()
}

/// Whether a member is a Sass source file (subject to cross-section
/// inheritance) rather than an expectation (`output*`, `error`, `warning`,
/// `options.yml`) which belongs to its own section only.
fn is_source_file(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    !(base.starts_with("output")
        || base.starts_with("error")
        || base.starts_with("warning")
        || base == "options.yml")
}

/// Insert or replace an entry by path, preserving source order.
fn upsert(entries: &mut Vec<Entry>, path: String, contents: String) {
    match entries.iter_mut().find(|(p, _)| *p == path) {
        Some(slot) => slot.1 = contents,
        None => entries.push((path, contents)),
    }
}

/// Split one case fragment into ordered `(path, contents)` entries. A line
/// beginning `<===> ` opens a new file; every other line (newline retained)
/// belongs to the current file's contents.
fn parse_files(section: &str) -> Vec<Entry> {
    let mut files: Vec<Entry> = Vec::new();
    let mut path: Option<String> = None;
    let mut buf = String::new();
    for line in section.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("<===> ") {
            if let Some(p) = path.take() {
                files.push((p, std::mem::take(&mut buf)));
            }
            path = Some(rest.trim_end_matches(['\n', '\r']).to_string());
        } else if line.trim_end() == "<===>" {
            // Bare divider header (only survives if a document lacks the 80-`=`
            // marker line): close the current entry without opening a new one.
            if let Some(p) = path.take() {
                files.push((p, std::mem::take(&mut buf)));
            }
        } else if path.is_some() {
            buf.push_str(line);
        }
    }
    if let Some(p) = path.take() {
        files.push((p, buf));
    }
    files
}
