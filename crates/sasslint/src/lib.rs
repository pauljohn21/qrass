//! Deterministic iron-rule linter for `sasspile`.
//!
//! The recurring violations all stem from one root — the pipeline not being a single
//! owned-value stream. This tool encodes the high-signal structural bans as a rule
//! table and scans Rust sources, so a violation is caught by a machine (CI / test gate)
//! instead of relying on self-discipline. Findings are reported via `tracing`.
//!
//! Three kinds of evidence, because the shapes differ:
//! * [`SUBSTRING_BANS`] / [`LINE_START_BANS`] — one line carries the signature alone;
//! * [`AND_BANS`] — only **two signatures on the same line** identify the shape, so a legal use of
//!   either one (`from_iter` as the source, `.ok()` on a lexeme parse) does not fire;
//! * [`FileFacts`] — a rule about a whole file ("a verbatim fallback must come with a fatality
//!   decision"), which no single line can state.
//!
//! The last two kinds are **proxies** for the iron rule, never proofs: they are tuned to catch the
//! shapes this repository actually removed, and they skip comments (see [`is_comment`]).

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: &'static str,
    pub text: String,
}

/// Unit names that must be spelled in exactly one table. A file branching on four or more of them is a
/// second convertible-unit table (see the `single-table` rule); the real table is `sasspile`'s
/// `crate::unit::UNITS`, which lists them as array entries rather than `match` arms.
const UNIT_LITERALS: [&str; 12] = [
    "\"px\"", "\"in\"", "\"cm\"", "\"mm\"", "\"pt\"", "\"pc\"", "\"q\"", "\"deg\"", "\"grad\"",
    "\"rad\"", "\"turn\"", "\"ms\"",
];

/// Substring bans: each is a precise signature of a forbidden pattern (kept narrow to
/// avoid false positives on `@for`/`@else if` string literals and comments).
/// Public so `tests/defence_fitness.rs` can derive the gate list from the engine itself: a hand-copied
/// list in the test is the same fact written twice, which is what let a gate id disappear from it once.
pub const SUBSTRING_BANS: &[(&str, &[&str])] = &[
    ("no-tree", &["Vec<Rule>", "pub children", ".children"]),
    // A hand-rolled orchestration layer is the shape that bent rxrust into running the project's own
    // control flow — the recurring rework root. The names are that layer's own vocabulary, anchored with
    // their delimiter the way `no-recursive-descent` anchors `fn resolve(`: a bare `fn fail` would also
    // match `fn failure(&self) -> bool`, which is not a framework layer.
    (
        "no-stage-layer",
        &[
            "type Stage",
            "fn drain(",
            "fn drain<",
            "fn fail(",
            "fn fail<",
        ],
    ),
    ("no-relay-vec", &["-> Vec<Rule>"]),
    ("no-string-machine", &["fn subst", "stmt_text"]),
    (
        "no-recursive-descent",
        &["fn write_rule", "fn resolve_all", "fn resolve("],
    ),
    ("no-from-fn-driver", &["from_fn("]),
    ("no-work-queue", &["VecDeque<"]),
    // heuristic: a mutable accumulator that DRIVES traversal (frame stack / @if chain),
    // i.e. a state machine wearing operator clothing — banned even though it uses no loop
    (
        "mutable-accumulator-drives",
        &["Vec<Frame>", "stack: Vec<", "chain_taken"],
    ),
    // Order was restored by sorting rules produced concurrently. The corpus settles a rule at an
    // *emission moment* instead (css/style_rule declaration/interleaved/**), so an ordering key is
    // the signature of re-adding the machinery that was deleted with P0′.4.
    (
        "no-sort-for-order",
        &["sort_by_key", "sort_unstable_by", "sort_by(", ".sort()"],
    ),
];

/// Same-line conjunctions: `(rule, &[(needle_a, needle_b)])`. Each pair is one forbidden shape whose
/// meaning only exists when both halves appear together — which is exactly what keeps the legal
/// single half from firing.
pub const AND_BANS: &[(&str, &[(&str, &str)])] = &[
    // a phase that batches its output into a `Vec` and flattens it again downstream: the
    // `scan_map(acc) -> Vec<Tok> -> flat_map(from_iter)` shell removed by P0′.1/.2, where the
    // kernel emits its own 0..N items with `observer.next(..)`
    (
        "no-batch-shell",
        &[
            ("scan_map(", "Vec<"),
            ("flat_map(", "from_iter("),
            ("then(", "from_iter("),
        ],
    ),
    // an evaluation failure turned into `None` and then into text: the error never reaches the
    // stream's `Err` channel (P0′.3a/b). `.ok()` on a lexeme parse is not this shape, hence the pair.
    ("require-err-channel", &[(".ok()", "eval")]),
];

/// The ids emitted by the file-level checks in [`FileFacts::line_findings`], in the order they are
/// checked. The fitness test reads this list instead of a copy of it, so a gate added there cannot be
/// left out of the positive controls without a test failing.
pub const FILE_RULE_IDS: &[&str] = &[
    "require-err-channel",
    "require-slot-before-subscribe",
    "single-table",
];

/// Combinators whose inner subscription must be parked in a slot **before** subscribing: rc.6
/// `into_future`/higher-order sources drop (or lose) what arrives while the slot is still empty
/// (docs/rxrust-source-analysis 09 §1), and a queued inner in `merge_all` **loses** emissions.
const HIGHER_ORDER: &[&str] = &[
    "merge_all(",
    "concat_all(",
    "switch_map(",
    "flat_map(",
    "connectable(",
    "Subject",
    "from_stream(",
];

/// Control-flow keywords banned when they START a (non-comment, non-`//`) line.
pub const LINE_START_BANS: &[&str] = &["for ", "while ", "loop "];

fn is_comment(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}

fn finding(file: &str, line_no: usize, rule: &'static str, raw: &str) -> Finding {
    Finding {
        file: file.to_string(),
        line: line_no,
        rule,
        text: raw.trim().to_string(),
    }
}

/// What the whole file states, for the rules no single line can decide. Built from **code lines
/// only**, so a doc comment can never launder a violation (e.g. mentioning `is_fatal` in prose while
/// the code swallows the error).
#[derive(Debug, Default)]
struct FileFacts {
    /// the file makes its fatality decision by kind (`SassError::is_fatal`), so a
    /// `unwrap_or_else(verbatim)` in it is the corpus-defined passthrough of a *specific* shape —
    /// not the blanket `.ok()` fallback P0′.3 replaces
    declares_fatality: bool,
    /// a verbatim fallback exists in code
    has_verbatim_fallback: bool,
    /// the file drives a higher-order/multi-source combinator
    uses_higher_order: bool,
    /// the file subscribes something itself
    subscribes: bool,
    /// the file parks a subscription in the framework's slot
    builds_slot: bool,
    /// distinct unit names appearing on a `=>` arm line — see the `single-table` check
    table_units: usize,
    /// the first such arm line, so one table produces one finding rather than six
    first_table_arm: Option<usize>,
}

/// An arm line of a unit table: something maps a quoted unit name to a value. Both halves on the same
/// line is the correlation that makes the shape — what keeps a lexer listing units in an array, or a
/// `matches!(name, "px" | "cm")`, from being read as a second table.
fn is_table_arm(line: &str) -> bool {
    line.contains("=>") && UNIT_LITERALS.iter().any(|needle| line.contains(needle))
}

/// Distinct unit names appearing on `=>` arm lines.
fn distinct_table_units(code: &str) -> usize {
    UNIT_LITERALS
        .into_iter()
        .filter(|needle| {
            code.lines()
                .any(|line| is_table_arm(line) && line.contains(needle))
        })
        .count()
}

/// The first arm line of such a table, so one table is one finding rather than one per arm.
fn first_table_arm_line(code: &str) -> Option<usize> {
    code.lines()
        .enumerate()
        .find(|(_, line)| is_table_arm(line))
        .map(|(index, _)| index + 1)
}

impl FileFacts {
    fn of(content: &str) -> Self {
        let code = content.lines().filter(|line| !is_comment(line)).fold(
            String::new(),
            |mut acc, line| {
                acc.push_str(line);
                acc.push('\n');
                acc
            },
        );
        let holds = |needles: &[&str]| needles.iter().any(|n| code.contains(n));
        Self {
            declares_fatality: code.contains("is_fatal"),
            has_verbatim_fallback: code.contains("unwrap_or_else("),
            uses_higher_order: holds(HIGHER_ORDER),
            subscribes: code.contains(".subscribe("),
            builds_slot: code.contains("SingleAssignment"),
            // Correlation matters: a file may legally *name* units (a lexer table, a serialiser) and may
            // legally contain `matches!(…)`. What makes a second conversion table is unit literals sitting
            // **on `=>` arms**, where each arm is one row of that table.
            table_units: distinct_table_units(&code),
            first_table_arm: first_table_arm_line(&code),
        }
    }

    /// The file-level rules, reported at the line that carries the offending call: a verbatim
    /// fallback with no `is_fatal` decision in the file, or a higher-order subscription with no
    /// `SingleAssignment` slot parked first.
    fn line_findings(&self, file: &str, line_no: usize, raw: &str) -> Vec<Finding> {
        if is_comment(raw) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.has_verbatim_fallback && !self.declares_fatality && raw.contains("unwrap_or_else(")
        {
            out.push(finding(file, line_no, FILE_RULE_IDS[0], raw));
        }
        if self.uses_higher_order
            && self.subscribes
            && !self.builds_slot
            && raw.contains(".subscribe(")
        {
            out.push(finding(file, line_no, FILE_RULE_IDS[1], raw));
        }
        // One fact, one place. Four or more unit names sitting on `=>` arms is a second convertible-unit
        // table: the copy in `builtins/math.rs` (`"px" => Some(1.0)`, `"cm" => Some(37.7952755906)`, …)
        // could not answer `hypot.hrx::units`, which needs `q` and the exact quarter-millimetre relation
        // that only `unit::UNITS` carries. No file is exempt by name — the shared table is not flagged
        // because it is an array of `(unit, (family, factor))` rows, not because of what the file is called
        // (a name-based exemption would be a hole a new file could walk through).
        if self.table_units >= 4 && self.first_table_arm == Some(line_no) {
            out.push(finding(file, line_no, FILE_RULE_IDS[2], raw));
        }
        out
    }
}

fn scan_line(file: &str, line_no: usize, raw: &str) -> Vec<Finding> {
    if is_comment(raw) {
        return Vec::new();
    }
    let matched = |needles: &[&str]| needles.iter().any(|n| raw.contains(n));
    SUBSTRING_BANS
        .iter()
        .filter(|(_, needles)| matched(needles))
        .map(|(rule, _)| finding(file, line_no, rule, raw))
        .chain(AND_BANS.iter().flat_map(|(rule, pairs)| {
            pairs
                .iter()
                .filter(|(a, b)| raw.contains(a) && raw.contains(b))
                .map(move |_| finding(file, line_no, rule, raw))
        }))
        .chain(
            if LINE_START_BANS
                .iter()
                .any(|kw| raw.trim_start().starts_with(kw))
            {
                vec![finding(file, line_no, "no-imperative-loop", raw)]
            } else {
                Vec::new()
            },
        )
        .collect()
}

pub fn scan_file(path: &Path) -> Vec<Finding> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let facts = FileFacts::of(&content);
    content
        .lines()
        .enumerate()
        .flat_map(|(i, line)| {
            scan_line(name, i + 1, line)
                .into_iter()
                .chain(facts.line_findings(name, i + 1, line))
        })
        .collect()
}

/// Recursively collect `.rs` files under `dir` (tooling may use recursion; the ban is
/// on the compiled pipeline walking a tree, not on a directory scan).
fn rust_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .for_each(|p| {
                if p.is_dir() {
                    rust_files(&p, acc);
                } else if p.extension().is_some_and(|e| e == "rs") {
                    acc.push(p);
                }
            });
    }
}

pub fn scan_dir(dir: &Path) -> Vec<Finding> {
    if dir.is_file() {
        return scan_file(dir);
    }
    let mut files = Vec::new();
    rust_files(dir, &mut files);
    files.iter().flat_map(|f| scan_file(f)).collect::<Vec<_>>()
}

/// Human-readable review report for a directory (used by the CLI and the MCP tool).
pub fn review_report(dir: &Path) -> String {
    let findings = scan_dir(dir);
    if findings.is_empty() {
        return format!("{}: no iron-rule violations", dir.display());
    }
    findings.iter().fold(String::new(), |mut s, f| {
        s.push_str(&format!(
            "[{}] {}:{} — {}\n",
            f.rule, f.file, f.line, f.text
        ));
        s
    }) + &format!("{} violation(s)", findings.len())
}

use serde_json::{Value, json};

/// Handle one MCP/JSON-RPC message, returning the response (or `None` for
/// notifications, which have no `id`). Implements the subset a tool client needs:
/// `initialize`, `tools/list`, `tools/call`.
pub fn handle_message(req: &Value) -> Option<Value> {
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "sasslint", "version": env!("CARGO_PKG_VERSION") }
        }),
        "tools/list" => json!({
            "tools": [{
                "name": "review",
                "description": "Scan a Rust source tree for sasspile iron-rule violations (tree/recursion/relay-Vec/string-machine/imperative loops, batch shells, sorted order, swallowed Err channel, missing subscription slot).",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string", "description": "directory to scan" } },
                    "required": ["path"]
                }
            }]
        }),
        "tools/call" => {
            let path = req
                .pointer("/params/arguments/path")
                .and_then(Value::as_str)
                .unwrap_or("crates/sasspile/src");
            json!({
                "content": [{ "type": "text", "text": review_report(Path::new(path)) }],
                "isError": false
            })
        }
        "ping" => json!({}),
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "method not found" }
            }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}
