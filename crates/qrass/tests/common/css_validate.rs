//! Lightweight structural well-formedness checks on emitted CSS (task 10.2).
//! These are deliberately cheap smoke gates — not a full CSS parser — verifying:
//! balanced `{} [] ()`, no empty selector before a `{`, and that every declaration
//! line inside a rule is semicolon-terminated (the expanded output puts one
//! declaration per line). String (`'…'`/`"…"`) and `/* … */` comment bodies are
//! skipped when balancing delimiters.

/// Validate `css`; `Err` carries a human message with a 1-based line number.
pub fn validate(css: &str) -> Result<(), String> {
    check_delimiters(css)?;
    check_structure(css)
}

/// Balanced braces / brackets / parens, ignoring strings and comments.
fn check_delimiters(css: &str) -> Result<(), String> {
    let bytes: Vec<char> = css.chars().collect();
    let mut stack: Vec<char> = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            '\n' => line += 1,
            '/' if bytes.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == '*' && bytes.get(i + 1) == Some(&'/')) {
                    if bytes[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 1; // land on '/' of the closer
            }
            '\'' | '"' => {
                let q = c;
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    if bytes[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            '{' | '[' | '(' => stack.push(c),
            '}' | ']' | ')' => {
                let want = match c {
                    '}' => '{',
                    ']' => '[',
                    _ => '(',
                };
                match stack.pop() {
                    Some(o) if o == want => {}
                    _ => return Err(format!("unbalanced '{c}' at line {line}")),
                }
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(o) = stack.last() {
        return Err(format!("unclosed '{o}' at end of file"));
    }
    Ok(())
}

/// Empty-selector and missing-semicolon checks over the trimmed, non-blank lines.
fn check_structure(css: &str) -> Result<(), String> {
    let mut depth = 0i32;
    for (idx, raw) in css.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("/*") || line.starts_with('}') {
            // A declaration may end a block via `}` on its own line; nothing to add.
            if line == "}" {
                depth -= 1;
            }
            continue;
        }
        if line.ends_with('{') {
            let head = line.trim_end_matches('{').trim();
            // A `{` with no selector/at-rule prelude text before it is malformed.
            if head.is_empty() {
                return Err(format!("empty selector before '{{' at line {line_no}"));
            }
            depth += 1;
            continue;
        }
        if depth > 0 {
            // A declaration line must be terminated by `;` (or continue a selector /
            // value across lines). A line carrying `prop: value` with no trailing
            // `;`/`,`/`:`/`{` means a missing semicolon.
            let ends_open = line.ends_with(';')
                || line.ends_with(',')
                || line.ends_with(':')
                || line.ends_with('{');
            if !ends_open && line.contains(':') {
                return Err(format!("declaration missing ';' at line {line_no}: {line}"));
            }
        }
    }
    if depth != 0 {
        return Err(format!("brace depth ended at {depth}, expected 0"));
    }
    Ok(())
}
