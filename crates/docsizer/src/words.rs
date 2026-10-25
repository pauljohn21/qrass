//! Words — the anti-padding measure: how much **prose** the tutorial actually contains.
//!
//! Fenced code blocks and table separator rows are excluded, and whitespace is not counted, so
//! pasting more source or stretching tables cannot inflate the number the tutorial is graded on.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Count {
    pub file: String,
    /// every character in the file
    pub total: usize,
    /// prose characters: outside ``` fences, excluding table separators, whitespace removed
    pub prose: usize,
}

/// Count both measures for one Markdown document.
pub fn count_markdown(file: &str, text: &str) -> Count {
    let total = text.chars().count();
    let prose = text
        .lines()
        .fold((false, 0usize), |(inside_fence, acc), line| {
            let fence = line.trim_start().starts_with("```");
            let separator = line.trim_start().starts_with('|') && line.contains("---");
            let counted = !fence && !inside_fence && !separator;
            let added = usize::from(counted) * line.chars().filter(|c| !c.is_whitespace()).count();
            // A fence line flips the state; anything else keeps it.
            (inside_fence ^ fence, acc + added)
        })
        .1;
    Count { file: file.to_string(), total, prose }
}

/// Every `.md` file under `dir`, recursively, sorted by path.
pub fn markdown(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect(dir, dir, &mut found)?;
    found.sort();
    Ok(found)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    fs::read_dir(dir)?.filter_map(Result::ok).try_fold((), |(), entry| {
        let path = entry.path();
        match (path.is_dir(), path.extension().and_then(|e| e.to_str()) == Some("md")) {
            (true, _) => collect(root, &path, out),
            (false, true) => {
                out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
                Ok(())
            }
            (false, false) => Ok(()),
        }
    })
}

/// Read and count every Markdown file under a directory.
pub fn tally(dir: &Path) -> io::Result<Vec<Count>> {
    markdown(dir)?.iter().try_fold(Vec::new(), |mut acc, path| {
        let text = fs::read_to_string(dir.join(path))?;
        acc.push(count_markdown(&path.to_string_lossy(), &text));
        Ok(acc)
    })
}
