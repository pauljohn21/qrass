//! Source spans and position mapping, shared by every compiler stage.

/// Identifies a loaded source file inside the compilation session.
pub type FileId = u32;

/// Byte offset inside a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct BytePos(pub usize);

/// Human readable 1-based line/column position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// A region of a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: FileId,
    pub start: BytePos,
    pub end: BytePos,
}

impl Span {
    pub fn point(file: FileId, at: BytePos) -> Self {
        Self {
            file,
            start: at,
            end: at,
        }
    }
}

/// Compute 1-based line/col of `pos` within `content`.
pub fn line_col(content: &str, pos: BytePos) -> LineCol {
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, ch) in content.char_indices() {
        if i >= pos.0 {
            break;
        }
        match ch {
            '\n' => {
                line += 1;
                col = 1;
            }
            '\r' => {}
            _ => col += 1,
        }
    }
    LineCol { line, col }
}

/// Extract the source line containing `lc` (without trailing newline).
pub fn excerpt_line(content: &str, lc: LineCol) -> Option<String> {
    content
        .lines()
        .nth((lc.line.max(1) - 1) as usize)
        .map(str::to_owned)
}

/// Registry mapping FileId to file names, owned by the loader stage.
#[derive(Clone, Default)]
pub struct Files {
    names: Vec<String>,
    contents: Vec<String>,
}

impl Files {
    pub fn add(&mut self, name: String, content: String) -> FileId {
        let id = self.names.len() as FileId;
        self.names.push(name);
        self.contents.push(content);
        id
    }

    pub fn name(&self, id: FileId) -> &str {
        self.names
            .get(id as usize)
            .map(String::as_str)
            .unwrap_or("<unknown>")
    }

    pub fn content(&self, id: FileId) -> Option<&str> {
        self.contents.get(id as usize).map(String::as_str)
    }

    /// Existing id for `name`, used to load each canonical path only once.
    pub fn id_by_name(&self, name: &str) -> Option<FileId> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| i as FileId)
    }
}
