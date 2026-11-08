//! `sasslint` 规则引擎的测试。

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

/// CI 闸：`sasspile/src` 必须没有铁律违规——现在没有了，因为 P0′ 落地了内核（0′.1/.2）、流产生的
/// 顺序（0′.4）与真正的 `Err` 通道（0′.3）。这是任务 0′.6 的机器那一半：重新引入 batch shell、
/// 用排序恢复顺序、吞掉 `Err` 或无槽高阶订阅，都会让测试套件失败，而不是依赖 grep 习惯。
#[test]
fn gate_sasspile_is_clean() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sasspile/src");
    let findings = scan_dir(&root);
    assert!(
        findings.is_empty(),
        "sasspile has rule violations: {findings:#?}"
    );
}

/// P0′.6 的四条规则对它们点名的形状触发。
#[test]
fn detects_the_p0_prime_shapes() {
    let path = std::env::temp_dir().join("sasslint_p0prime.rs");
    fs::write(
        &path,
        [
            // 一个 phase 先批处理输出、再展平：P0′.1/.2 删掉的那个壳
            "acc = acc.scan_map(|a, tok| -> Vec<Tok> { a.feed(tok) });",
            "stream.flat_map(|toks| Local::from_iter(toks));",
            // 求值失败被变成 `None`，即永远不到达 Err 通道
            "let v = expr.eval(&env).ok();",
            // 顺序靠排序而不是发射时刻恢复（P0′.4）
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

/// 逐字兜底只有在文件还按种类决定致命性时才是语料定义的放行；没有那个决定，它就是 P0′.3 替换掉
/// 的 blanket `.ok()` 形状。
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

/// rc.6 在订阅**之前**把高阶源的订阅交给 `SingleAssignment` 槽；无槽就订阅会丢掉内部等待期间
/// 发出的东西。
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

/// 负的那一半，正是它让四条规则可作 CI 闸：合取之一的**合法**使用不得触发。`from_iter` 是管线自己的
/// 源，1:1 内核的 `source.subscribe(..)` 是框架的 `map` 形状，词素解析的 `.ok()` 不是求值吞掉——
/// 今天的 `sasspile/src` 恰好就是这些代码，所以它过了上面那道闸。
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
    // 元错误：一个手搓的驱动器伪装成迭代器/rxrust 算子
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
    // 一个 fold 累加子，持有帧栈 / @if 链来*驱动*遍历
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

/// `no-stage-layer` 的阳性对照：手搓的 `Stage`/`drain`/`fail` 层正是让 rxrust 跑起项目自身控制流的
/// 形状，所以该闸必须在它上面触发。rxrust 真正索要的内核源不得触发——那是下界。
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

/// 各闸对着它们所管的树量：生产源必须保持干净，所以一道闸不可能被放宽到超出教义所禁。
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

/// `single-table` 的阳性对照：本项目实际交付过两次的形状——一张对单位字面量做 `match`、自带换算
/// 系数的表——必须在除唯一真相源之外的任何文件里被抓住（`crates/sasspile/src/unit.rs` 的表是
/// `(unit, (family, factor))` 数组而不是 `match` 臂）。
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

/// 让该闸的上一版几乎沦为装饰的盲区：写成**元组数组**的系数表（`unit::UNITS` 本身用的形状）对只看
/// `=>` 臂的规则不可见。如今被认可的表靠它定义什么（`pub const UNITS`）识别，而不是靠文件名——
/// 所以停在一个叫 `unit.rs` 的文件里的拷贝照样被抓住。
#[test]
fn a_tuple_array_table_is_caught_even_when_the_file_is_named_unit_rs() {
    let array_table = std::env::temp_dir().join("sasslint_planted_tuple_table.rs");
    fs::write(
        &array_table,
        "const FACTORS: [(&str, f64); 4] = [\n\
         \x20   (\"px\", (1.0)),\n\
         \x20   (\"cm\", (37.0)),\n\
         \x20   (\"in\", (96.0)),\n\
         \x20   (\"turn\", (360.0)),\n\
         ];\n",
    )
    .unwrap();
    assert!(
        scan_file(&array_table)
            .iter()
            .any(|f| f.rule == "single-table"),
        "a factor table written as tuples slipped through"
    );

    let disguised = std::env::temp_dir().join("unit.rs");
    fs::write(
        &disguised,
        "const F: [(&str, f64); 4] [(\"px\", (1.0)), (\"cm\", (37.0)), (\"in\", (96.0)), (\"turn\", (360.0))]\n",
    )
    .unwrap();
    assert!(
        scan_file(&disguised)
            .iter()
            .any(|f| f.rule == "single-table"),
        "naming a copy `unit.rs` must not exempt it - the sanctioned file is the one defining `pub const UNITS`"
    );
}
