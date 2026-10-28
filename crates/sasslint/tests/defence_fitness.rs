//! The defence's own fitness function. Two things are asserted here, and both are the kind of claim
//! that has silently rotted before:
//!
//! 1. **Every gate has a working positive control.** A rule nobody plants is a rule that can be renamed,
//!    narrowed or broken with nobody noticing — this project measured two `0` readings that came out of
//!    the instrument rather than the corpus (`ledger failures … --count` once permanently returned 0; a
//!    `substr` slice made "differs only in whitespace" read 0). So this test plants one source carrying
//!    every forbidden shape and requires each registered id to fire.
//! 2. **The tree under police stays clean**, which is what keeps a gate from being widened past the
//!    doctrine it encodes.
//!
//! If a new gate is added to `crates/sasslint/src/lib.rs`, its id must be added to `RULES` **and** its
//! shape to `PLANTED` in the same change — otherwise the first test fails, which is the point.

use std::fs;
use std::path::PathBuf;

use sasslint::{Finding, scan_dir, scan_file};

/// Every gate the defence relies on. Read from `SUBSTRING_BANS` / `AND_BANS` / `LINE_START_BANS` /
/// `FileFacts::line_findings` in `src/lib.rs`; the newest two (`no-stage-layer`, `single-table`) were
/// added by this plan.
const RULES: [&str; 14] = [
    "no-tree",
    "no-stage-layer",
    "no-relay-vec",
    "no-string-machine",
    "no-recursive-descent",
    "no-from-fn-driver",
    "no-work-queue",
    "mutable-accumulator-drives",
    "no-sort-for-order",
    "no-batch-shell",
    "require-err-channel",
    "require-slot-before-subscribe",
    "no-imperative-loop",
    "single-table",
];

/// One source that carries every forbidden shape. Each line is written the way the offending code was
/// actually written in this repository at some point, not as a comment mentioning the rule.
const PLANTED: &str = "\
pub struct Rule { pub children: Vec<Rule> }
type Stage<T> = Box<dyn Fn(T) -> T>;
fn relay(items: Vec<Rule>) -> Vec<Rule> { items }
fn subst(text: &str) -> String { text.to_string() }
fn resolve(&self, node: Node) -> Value { self.resolve(node.next()) }
fn driver() { let _ = from_fn(|_| Vec::<Item>::new()); }
fn queue(q: VecDeque<Item>) -> VecDeque<Item> { q }
fn drive(&mut self) { let frames = Vec::<Frame>::default(); let _ = self.chain_taken; }
fn order(&mut self) { self.rows.sort_by_key(|row| row.order); }
fn batch(src: i32) { let _ = scan_map(src, |acc, item| -> Vec<Item> { vec![] }); }
fn silent(eval: Option<i32>) { let _ = eval.ok(); }
fn multicast(src: i32) { let _ = merge_all(src).subscribe(|v| { let _ = v; }); }
loop { }
fn to_base(unit: &str) -> f64 { match unit { \"px\" => 1.0, \"in\" => 96.0, \"cm\" => 37.0, \"turn\" => 360.0, _ => 0.0 } }
";

#[test]
fn every_gate_has_a_working_positive_control() {
    let dir = std::env::temp_dir().join("sasslint_defence_probe");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bad.rs"), PLANTED).unwrap();

    let findings: Vec<Finding> = scan_dir(&dir);
    let missing: Vec<&str> = RULES
        .into_iter()
        .filter(|rule| !findings.iter().any(|found| found.rule == *rule))
        .collect();
    assert!(
        missing.is_empty(),
        "these gates fire on nothing — a planted shape or the rule needs fixing, never the list: \
         {missing:?}\nfindings: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );

    fs::remove_dir_all(&dir).ok();
}

/// The planted shapes must not be so broad that the real compiler trips over its own gates.
#[test]
fn the_production_tree_is_clean() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&dir);
    assert!(findings.is_empty(), "sasspile must stay gate-clean: {findings:?}");
}

/// A source of the shape the doctrine *asks* for — real operator kernels, one terminal `subscribe`,
/// `SingleAssignment` slots, `filter` for branching — must produce no findings at all. This is the
/// lower bound: without it, "every rule fires" could be satisfied by a rule that fires on everything.
#[test]
fn a_doctrine_shaped_file_produces_no_findings() {
    let path = std::env::temp_dir().join("sasslint_doctrine_clean.rs");
    fs::write(
        &path,
        "\
struct Lex<S> { source: S }
impl<C: Context> CoreObservable<C> for Lex<S> {
    fn subscribe(self, ctx: C) {
        let w = ctx.transform(|o| LexObserver { observer: o, state: State::default() });
        self.source.subscribe(w)
    }
}
fn pipeline(src: i32) {
    let slot = SingleAssignment::<LocalBoxedObservable<i32>>::channel();
    let _ = src.subscribe(slot.observer());
    slot.subscribe(src);
}
fn branch(items: Vec<Item>) { items.into_iter().filter(|item| item.keep) }
",
    )
    .unwrap();
    let findings = scan_file(&path);
    assert!(
        findings.is_empty(),
        "a file written the way the doctrine requires was flagged: {findings:?}"
    );
}
