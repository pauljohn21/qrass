//! Tests for the `sasslint` rule engine.

use std::fs;
use std::path::PathBuf;

use sasslint::{Finding, scan_dir, scan_file};

fn has(findings: &[Finding], rule: &str) -> bool {
    findings.iter().any(|f| f.rule == rule)
}

#[test]
fn detects_forbidden_patterns() {
    let path = std::env::temp_dir().join("sasslint_probe.rs");
    fs::write(
        &path,
        "struct Rule { pub children: Vec<Rule> }\nfn resolve_all(&self, r: Vec<Rule>) -> Vec<Rule> { }\nfn subst() { let _ = stmt_text(); }\nloop { let _ = 1; }\n",
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(has(&f, "no-tree"), "tree/children not caught: {f:?}");
    assert!(has(&f, "no-relay-vec"), "relay Vec<Rule> not caught: {f:?}");
    assert!(
        has(&f, "no-string-machine"),
        "string machine not caught: {f:?}"
    );
    assert!(
        has(&f, "no-recursive-descent"),
        "recursion not caught: {f:?}"
    );
    assert!(has(&f, "no-imperative-loop"), "loop not caught: {f:?}");
    fs::remove_file(&path).ok();
}

#[test]
fn ignores_comments() {
    let path = std::env::temp_dir().join("sasslint_comment.rs");
    fs::write(
        &path,
        "// Vec<Rule> mentioned in a doc comment\nlet x = 1;\n",
    )
    .unwrap();
    assert!(
        scan_file(&path).is_empty(),
        "comment line should not be flagged"
    );
    fs::remove_file(&path).ok();
}

/// CI gate: `sasspile/src` must be free of iron-rule violations — and it is now, because P0′ landed
/// the kernels (0′.1/.2), the stream-produced order (0′.4) and the real `Err` channel (0′.3). This is
/// the machine half of tasks 0′.6: re-introducing a batch shell, an ordering sort, a swallowed `Err`
/// or a slotless higher-order subscription fails the test suite instead of relying on a grep habit.
#[test]
fn gate_sasspile_is_clean() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&root);
    assert!(
        findings.is_empty(),
        "sasspile has rule violations: {findings:#?}"
    );
}

/// The four P0′.6 rules fire on the shapes they name.
#[test]
fn detects_the_p0_prime_shapes() {
    let path = std::env::temp_dir().join("sasslint_p0prime.rs");
    fs::write(
        &path,
        [
            // a phase batching its output, then flattening it again: the shell P0′.1/.2 deleted
            "acc = acc.scan_map(|a, tok| -> Vec<Tok> { a.feed(tok) });",
            "stream.flat_map(|toks| Local::from_iter(toks));",
            // an evaluation failure turned into `None`, i.e. never reaching the Err channel
            "let v = expr.eval(&env).ok();",
            // order restored by sorting instead of by the emission moment (P0′.4)
            "rules.sort_by_key(|r| r.order);",
            "rows.sort_unstable_by(|a, b| a.cmp(b));",
        ]
        .join("\n"),
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(
        has(&f, "no-batch-shell"),
        "scan_map→Vec / flat_map(from_iter) not caught: {f:?}"
    );
    assert!(
        has(&f, "require-err-channel"),
        "eval(...).ok() swallow not caught: {f:?}"
    );
    assert!(
        has(&f, "no-sort-for-order"),
        "sort used to restore stream order not caught: {f:?}"
    );
    fs::remove_file(&path).ok();
}

/// A verbatim fallback is only the corpus-defined passthrough when the file also decides fatality by
/// kind; without that decision it is the blanket `.ok()` shape P0′.3 replaced.
#[test]
fn verbatim_fallback_needs_a_fatality_decision() {
    let bad = std::env::temp_dir().join("sasslint_fallback_bad.rs");
    let good = std::env::temp_dir().join("sasslint_fallback_ok.rs");
    let fallback = "let owned = evaluated.unwrap_or_else(|| Value::from_text(render(toks)));\n";
    fs::write(&bad, fallback).unwrap();
    fs::write(
        &good,
        format!("{fallback}if err.is_fatal() {{ return Err(err); }}\n"),
    )
    .unwrap();
    assert!(
        has(&scan_file(&bad), "require-err-channel"),
        "fallback with no fatality decision should fire"
    );
    assert!(
        !has(&scan_file(&good), "require-err-channel"),
        "an explicit per-kind decision is the allowed shape"
    );
    fs::remove_file(&bad).ok();
    fs::remove_file(&good).ok();
}

/// rc.6 hands a higher-order source's subscription to a `SingleAssignment` slot **before**
/// subscribing; subscribing one without a slot loses what the inner emits while it waits.
#[test]
fn higher_order_subscribe_needs_a_slot() {
    let bad = std::env::temp_dir().join("sasslint_slot_bad.rs");
    let good = std::env::temp_dir().join("sasslint_slot_ok.rs");
    let body = "let subject = Subject::new();\nsource.subscribe(wrapped);\n";
    fs::write(&bad, body).unwrap();
    fs::write(
        &good,
        format!("let (slot, _) = SingleAssignment::new();\n{body}"),
    )
    .unwrap();
    assert!(
        has(&scan_file(&bad), "require-slot-before-subscribe"),
        "higher-order subscribe with no slot should fire"
    );
    assert!(
        !has(&scan_file(&good), "require-slot-before-subscribe"),
        "parking the slot first is the allowed shape"
    );
    fs::remove_file(&bad).ok();
    fs::remove_file(&good).ok();
}

/// The negative half, which is what makes the four rules usable as a CI gate: the **legal** use of one
/// half of a conjunction must not fire. `from_iter` is the pipeline's own source, a 1:1 kernel's
/// `source.subscribe(..)` is the framework's `map` shape, and a lexeme parse's `.ok()` is not an
/// evaluation swallow — `sasspile/src` today is exactly this code, so it passes the gate above.
#[test]
fn legal_stream_shapes_are_not_flagged() {
    let path = std::env::temp_dir().join("sasslint_legal.rs");
    fs::write(
        &path,
        [
            "let chars = Shared::from_iter(content.chars().collect::<Vec<char>>());",
            "source.subscribe(wrapped)",
            "numeric.parse::<f64>().ok().map(|value| (value, unit))",
            "// sort_by_key and scan_map(..) -> Vec stay legal in a comment",
        ]
        .join("\n"),
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(f.is_empty(), "legal stream code was flagged: {f:?}");
    fs::remove_file(&path).ok();
}

#[test]
fn detects_state_machine_in_operator_clothing() {
    // the meta-error: a hand-rolled driver disguised as an iterator/rxrust operator
    let path = std::env::temp_dir().join("sasslint_sm.rs");
    fs::write(
        &path,
        "let q: VecDeque<Job> = VecDeque::new();\nstd::iter::from_fn(|| q.pop_front()).for_each(drop);\n",
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(
        has(&f, "no-work-queue"),
        "VecDeque work-queue not caught: {f:?}"
    );
    assert!(
        has(&f, "no-from-fn-driver"),
        "from_fn driver not caught: {f:?}"
    );
    fs::remove_file(&path).ok();
}

#[test]
fn detects_mutable_accumulator_drives() {
    // a fold accumulator holding a frame stack / @if chain to DRIVE traversal
    let path = std::env::temp_dir().join("sasslint_acc.rs");
    fs::write(
        &path,
        "struct Acc { stack: Vec<Frame>, chain_taken: bool }\n",
    )
    .unwrap();
    let f = scan_file(&path);
    assert!(
        has(&f, "mutable-accumulator-drives"),
        "frame-stack/chain accumulator not caught: {f:?}"
    );
    fs::remove_file(&path).ok();
}

#[test]
fn mcp_protocol_handshake_and_review() {
    use sasslint::handle_message;
    use serde_json::json;

    let init =
        handle_message(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}))
            .unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "sasslint");

    let list = handle_message(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    assert_eq!(list["result"]["tools"][0]["name"], "review");

    assert!(
        handle_message(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none()
    );

    let dir = std::env::temp_dir().join("sasslint_mcp_probe");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bad.rs"), "pub children: Vec<Rule>\n").unwrap();
    let call = handle_message(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "review", "arguments": { "path": dir.to_str().unwrap() } }
    }))
    .unwrap();
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("no-tree"),
        "review tool missed violation: {text}"
    );
    fs::remove_dir_all(&dir).ok();
}

/// Positive control for `no-stage-layer`: a hand-rolled `Stage`/`drain`/`fail` layer is exactly the
/// shape that made rxrust run the project's own control flow, so the gate must fire on it. The
/// kernel source that rxrust actually asks for must not fire — that is the lower bound.
#[test]
fn planted_framework_layer_is_caught_and_a_kernel_is_not() {
    let planted = std::env::temp_dir().join("sasslint_planted_layer.rs");
    fs::write(
        &planted,
        "type Stage<T> = Box<dyn Fn(T) -> T>;\n\
         fn drain(&mut self) -> Vec<Item> { vec![] }\n\
         fn fail<T>(m: &str) -> Result<T, Error> { Err(err(m)) }\n",
    )
    .unwrap();
    let f = scan_file(&planted);
    for (line, rule) in [
        (1usize, "no-stage-layer"),
        (2, "no-stage-layer"),
        (3, "no-stage-layer"),
    ] {
        assert!(
            f.iter()
                .any(|found| found.rule == rule && found.line == line),
            "planted {rule} at line {line} not caught: {f:?}"
        );
    }

    let kernel = std::env::temp_dir().join("sasslint_legal_kernel.rs");
    fs::write(
        &kernel,
        "struct Lex<S> { source: S }\n\
         impl<C: Context> CoreObservable<C> for Lex<LocalBoxedObservable<Event>> {\n\
         \x20   fn subscribe(self, ctx: C) { let w = ctx.transform(|o| LexObserver { observer: o }); }\n\
         }\n\
         fn go(x: i32) { if x == 0 { return; } run(x); }\n",
    )
    .unwrap();
    let clean = scan_file(&kernel);
    assert!(
        !clean.iter().any(|found| found.rule == "no-stage-layer"),
        "a real operator kernel was misflagged: {clean:?}"
    );
}

/// The gates are measured against the tree they police: the production source must stay clean, so a
/// gate cannot have been widened past what the doctrine bans.
#[test]
fn the_new_gates_leave_the_production_tree_clean() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&dir);
    for rule in ["no-stage-layer", "single-table"] {
        assert!(
            !findings.iter().any(|f| f.rule == rule),
            "{rule} fires on existing code — narrow it, never relax it: {:?}",
            findings
                .iter()
                .filter(|f| f.rule == rule)
                .collect::<Vec<_>>()
        );
    }
}

/// Positive control for `single-table`: the shape this project actually shipped twice — a `match` over
/// unit literals carrying its own conversion factors — must be caught in any file except the single
/// source of truth (`crates/sasspile/src/unit.rs`, whose table is an array of `(unit, (family, factor))`
/// rather than `match` arms).
#[test]
fn a_second_unit_table_is_caught_and_the_shared_one_is_not() {
    let planted = std::env::temp_dir().join("sasslint_planted_table.rs");
    fs::write(
        &planted,
        "fn to_base(unit: &str) -> Option<f64> {\n\
         \x20   match unit {\n\
         \x20       \"px\" => Some(1.0),\n\
         \x20       \"in\" => Some(96.0),\n\
         \x20       \"cm\" => Some(37.7952755906),\n\
         \x20       \"turn\" => Some(360.0),\n\
         \x20       _ => None,\n\
         \x20   }\n}\n",
    )
    .unwrap();
    let f = scan_file(&planted);
    assert!(
        f.iter().any(|found| found.rule == "single-table"),
        "a second convertible-unit table was not caught: {f:?}"
    );

    let shared = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src/unit.rs");
    let findings = scan_file(&shared);
    assert!(
        !findings.iter().any(|f| f.rule == "single-table"),
        "the single source of truth flagged itself: {findings:?}"
    );
}
