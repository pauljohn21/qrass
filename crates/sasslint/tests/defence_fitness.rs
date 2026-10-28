//! The defence's own fitness function. Four things are asserted here, each against a claim that has
//! silently rotted before in this repository:
//!
//! 1. **Every gate the engine can emit has a working positive control.** The id list is *derived from
//!    the engine's own tables* (`sasslint::SUBSTRING_BANS` / `AND_BANS` / `LINE_START_BANS` /
//!    `FILE_RULE_IDS`), never copied into this file — a hand-written copy is the same fact twice, and
//!    one such copy is exactly how a gate id vanished from a list unnoticed.
//! 2. **The tree under police stays clean**, which is what keeps a gate from being widened past the
//!    doctrine it encodes (measured twice this session: a `0` reading came out of the instrument).
//! 3. **A doctrine-shaped file produces no findings** — the lower bound. Without it, "every rule fires"
//!    could be satisfied by rules that fire on everything.
//! 4. **A deferred gate is recorded with its debt**, so a gap is legible instead of invisible.

use std::fs;
use std::path::PathBuf;

use sasslint::{
    AND_BANS, FILE_RULE_IDS, Finding, LINE_START_BANS, SUBSTRING_BANS, scan_dir, scan_file,
};

/// Every id the engine is capable of emitting, read off the engine.
fn engine_rule_ids() -> Vec<&'static str> {
    SUBSTRING_BANS
        .iter()
        .map(|(rule, _)| *rule)
        .chain(AND_BANS.iter().map(|(rule, _)| *rule))
        .chain((!LINE_START_BANS.is_empty()).then_some("no-imperative-loop"))
        .chain(FILE_RULE_IDS.iter().copied())
        .collect()
}

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
fn unwrap_to_text(eval: Option<i32>) { let _ = eval.unwrap_or_else(|| 1); }
fn multicast(src: i32) { let _ = merge_all(src).subscribe(|v| { let _ = v; }); }
loop { }
fn to_base(unit: &str) -> f64 { match unit { \"px\" => 1.0, \"in\" => 96.0, \"cm\" => 37.0, \"turn\" => 360.0, _ => 0.0 } }
";

#[test]
fn every_gate_the_engine_can_emit_has_a_positive_control() {
    let dir = std::env::temp_dir().join("sasslint_defence_probe");
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bad.rs"), PLANTED).unwrap();

    let findings: Vec<Finding> = scan_dir(&dir);
    let missing: Vec<&str> = engine_rule_ids()
        .into_iter()
        .filter(|rule| !findings.iter().any(|found| found.rule == *rule))
        .collect();
    assert!(
        missing.is_empty(),
        "these gates fire on nothing — fix the planted shape or the rule, never the list: {missing:?}",
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_production_tree_is_clean() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&dir);
    assert!(
        findings.is_empty(),
        "sasspile must stay gate-clean ({} findings): {findings:?}",
        findings.len()
    );
}

/// A source shaped the way the doctrine asks for — real operator kernels, `SingleAssignment` slots
/// parked before subscribing, `filter` for branching, units listed in an array rather than on `=>` arms
/// — must produce no findings at all.
#[test]
fn a_doctrine_shaped_file_produces_no_findings() {
    let path = std::env::temp_dir().join("sasslint_doctrine_clean.rs");
    fs::write(
        &path,
        "\
struct Lex<S> { source: S }
const NAMES: [&str; 4] = [\"px\", \"cm\", \"deg\", \"turn\"];
impl<C: Context> CoreObservable<C> for Lex<S> {
    fn subscribe(self, ctx: C) {
        let w = ctx.transform(|o| LexObserver { observer: o, state: State::default() });
        self.source.subscribe(w)
    }
}
fn pipeline(src: i32) {
    let slot = SingleAssignment::<LocalBoxedObservable<i32>>::channel();
    slot.subscribe(src);
}
fn is_length(name: &str) -> bool { matches!(name, \"px\" | \"cm\") }
fn failure(&self) -> bool { false }
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

/// The one gate this plan planned and did **not** land, kept on the record with the size of the debt.
/// `rust.md` bans `if/else if` chains outright (its only exception is a single guard `if` without
/// `else`), so the existing sites are stock debt rather than legal shapes — and a needle that cannot
/// tell them apart would have flagged the tree on day one. When they are reworked, implement the gate
/// and delete this test, which fails if either half of the record stops being true.
#[test]
fn the_deferred_branch_gate_stays_recorded() {
    assert!(
        !engine_rule_ids().contains(&"no-else-if-chain"),
        "`no-else-if-chain` now exists — this record is obsolete, delete it and let the positive control cover the gate"
    );
    let sites = [
        "lexer.rs",
        "loops.rs",
        "expr.rs",
        "builtins/math.rs",
        "color/read.rs",
    ];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let counted = sites
        .iter()
        .map(|site| {
            fs::read_to_string(root.join(site))
                .unwrap_or_else(|e| panic!("{} moved: {e}", root.join(site).display()))
                .lines()
                .filter(|line| line.contains("} else if"))
                .count()
        })
        .sum::<usize>();
    assert_eq!(
        counted, 7,
        "the recorded else-if debt changed size ({counted}): either some sites were reworked \
         (implement the gate and remove this record) or new ones were added (a doctrine violation \
         that must be fixed, not counted)"
    );
}
