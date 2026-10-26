//! Calculation — the first slice of tasks 1.4: `calc( … )` and the calculation functions
//! (`min`/`max`/`clamp`) become an owned **value** carrying its inner expression as text normalised at
//! parse time, instead of the whole source run staying one opaque `Value::Raw`.
//!
//! Scope is exactly what `tasks.md` 1.4a states, and nothing wider. Each rule is quoted from
//! `sass-spec/values/calculation/calc/no_operator.hrx` (with `calc/parens.hrx` for the parenthesised
//! shapes), read through `ledger failures bool-units-746 "values/calculation/calc/no_operator"`:
//! • a single collapsible value leaves its shells — `calc(1px)` → `1px` (`::number/integer`),
//!   `CaLc(1px)` → `1px` (`::case_insensitive`), `calc( 1px )` → `1px` (`::extra_whitespace`),
//!   `calc( 1 )` and `calc( ( 1 ) )` → `1` (`syntax/extra_whitespace/*`), `calc((1px))` → `1px`
//!   (`calc/parens::number`);
//! • a nested calculation drops **one** shell, and dropping it can expose the next collapse —
//!   `calc(calc(1px))` → `1px` (`::calculation/calc/simplified`, two levels),
//!   `calc(calc(1px + 1%))` → `calc(1px + 1%)` (`::calculation/calc/preserved`),
//!   `calc(clamp(1%, 2px, 3%))` → `clamp(1%, 2px, 3%)` (`::calculation/clamp/preserved`),
//!   `calc(min(1%, 2px))` → `min(1%, 2px)` (`::calculation/min/preserved`),
//!   `calc((calc(1px + 1%)))` → `calc(1px + 1%)` (`calc/parens::calculation`);
//! • anything else keeps its outer shell — `calc(var(--c))`, `calc((var(--c)))`, `calc(((var(--c))))`,
//!   `calc(c())`, `calc(foobar)` from a bound `$a`, `calc(#{1px + 2px})`, `calc((#{1px + 2px}))`,
//!   `calc((d))` (`::var/bare`, `::function/css`, `::variable/unquoted_string`,
//!   `::interpolation/{number,parens}`, `calc/parens::{var/direct,double_preserved,identifier}` — several
//!   of these **already pass**, so they are this rule's safety net).
//!
//! Deliberately **not** here, because 1.4a forbids guessing them and later tasks own them: same-unit
//! simplification (`calc(1px + 2px)` → `3px` stays red — that is `calc/simplify.hrx`, whose own README
//! states only "number + number where both are compatible" simplifies, and which 1.4d lands together
//! with `*`/`/`), compound (rational) units, and **evaluating** `min`/`max`/`clamp`
//! (`::calculation/min/simplified` wants `1px`, which needs bare-name builtins ⇒ a later batch).
//!
//! ## The lexeme shape this reads (measured, not assumed)
//!
//! `(` and `)` are not structural in the lexer, so they ride inside `Word` and a whole declaration value
//! can arrive as ONE lexeme — `Word("calc(1px)")`, `Word("calc(1px")` (the block's `}` ended the call),
//! `Word("calc(1px + 1%)")`. Three attempts failed on that fact: matching the *first* lexeme for a
//! trailing `(`, gluing the `(` onto the piece *after* it, and filtering layout before the spacing rule
//! ran. [`cut_open`] is the normalisation that survived the traces; structure is then read off lexemes.

use crate::calls;
use crate::env::Env;
use crate::expr;
use crate::lexer::{Tok, render};
use crate::value::Value;

/// The names whose call is a calculation, matched on the **bare** name case-insensitively. A vendor
/// prefix means it is not one: `-a-calc(` is a single lexeme and
/// `css/functions/special/prefixed/lowercase::calc/script_like` keeps `-a-calc($c)` as text, so the
/// prefix-stripping `expr::special_call` does must not be reused here.
const CALCULATION_NAMES: [&str; 4] = ["calc", "min", "max", "clamp"];

/// A calculation call. `name` is the callee **as written** and `lower` its case-folded form, which is
/// the only thing the law reads: 1.4c forbids turning case normalisation into an output rule, so a
/// preserved calculation prints the name exactly as the source spelled it.
struct Shell {
    name: String,
    lower: String,
    inner: Vec<Tok>,
}

/// What one argument came down to. The shapes stay separate because the corpus treats them differently:
/// only a number/boolean leaves a shell, and an inner calculation's **body** is the one text that
/// replaces its wrapper instead of sitting inside it.
#[derive(Clone)]
enum Outcome {
    Collapsed(Value),
    /// an inner calculation's body without its wrapper: the enclosing shell's name takes it over
    Nested(String),
    /// a calculation that already carries its own name — content to the enclosing shell, whose wrapper
    /// drops in favour of it
    Named(String),
    /// readable content that is not a calculation
    Kept(String),
    /// nothing this slice reads; the caller keeps the run's own text
    Opaque,
}

/// Build the calculation value a run stands for, or `None` when it is not a calculation this slice
/// reads (unknown callee, unclosed bracket, an operation outside the call).
///
/// The event is the diagnostic this module exists to answer: the **lexeme shape as it arrived** and after
/// [`open_at`], plus which decision was taken. Every wrong version of this rule failed silently — the
/// callee test matched nothing, so nothing moved — and only these two facts made that visible.
pub fn value(toks: &[Tok], env: &Env) -> Option<Value> {
    let shell = find(toks)?;
    let built = to_value(&shell, &arguments(&shell, env), env);
    // Named outside the macro on purpose: `tracing` resolves `Value` to its own field trait inside a
    // macro body, so a `Value::Calculation` path there does not compile.
    let decision = match &built {
        None => "declined",
        Some(Value::Calculation(..)) => "preserved",
        Some(Value::Raw(..)) => "shell dropped",
        Some(_) => "collapsed",
    };
    let resolved = built.as_ref().map_or(String::new(), Value::to_css);
    tracing::debug!(
        stage = "exec",
        module = "calculation",
        name = shell.name.as_str(),
        arrived = %render(toks),
        cut = ?open_at(toks),
        decision,
        resolved = %resolved,
        "calculation read"
    );
    built
}

/// The shell law. A `calc` around **another named calculation** drops its wrapper and hands the inner
/// one over — even when the inner cannot be evaluated, which is how `calc(calc(1px))` reaches `1px` in
/// two steps while `calc(clamp(1%, 2px, 3%))` stops at `clamp(1%, 2px, 3%)`. A `calc` around plain
/// content keeps its wrapper (`calc(var(--c))`). `min`/`max`/`clamp` keep their own name and are never
/// evaluated here, because 1.4a refuses to guess that law.
fn to_value(shell: &Shell, parts: &[Outcome], env: &Env) -> Option<Value> {
    if !is_calc(&shell.lower) {
        // A `min`/`max`/`clamp` shell keeps its own name and never evaluates: that law is 1.4a's "do not
        // guess" case. One unreadable argument makes the whole run decline, which leaves the text exactly
        // as the source wrote it — no name or spacing is rewritten outside a `calc` (1.4c).
        let readable = parts.iter().all(|part| !matches!(part, Outcome::Opaque));
        return readable.then(|| Value::Calculation(shell.name.clone(), rendered(parts)));
    }
    match parts {
        [Outcome::Collapsed(value)] => Some(value.clone()),
        [Outcome::Nested(inner)] => Some(Value::Calculation(shell.name.clone(), inner.clone())),
        // An inner calculation printed with its own name: that name survives, this wrapper goes.
        [Outcome::Named(text)] => Some(Value::Raw(text.clone())),
        [Outcome::Kept(text)] => Some(Value::Calculation(shell.name.clone(), text.clone())),
        [Outcome::Opaque] => decline_to_text(shell, env),
        many => Some(Value::Calculation(shell.name.clone(), rendered(many))),
    }
}

/// The argument list this slice cannot read as a value: its normalised text survives, and if that text is
/// itself a calculation call, one wrapper drops in favour of the inner one
/// (`::calculation/*/preserved`, `calc/parens::calculation`). A bare `min( … )` with nothing readable in
/// it declines instead, which keeps today's verbatim behaviour for the shapes 1.4a leaves alone.
fn decline_to_text(shell: &Shell, env: &Env) -> Option<Value> {
    match find(&shell.inner) {
        Some(inner) => to_value(&inner, &arguments(&inner, env), env).map(|value| match value {
            Value::Calculation(name, body) => Value::Raw(format!("{name}({body})")),
            other => other,
        }),
        None if is_calc(&shell.lower) => Some(Value::Calculation(
            shell.name.clone(),
            resolved(&shell.inner, env),
        )),
        None => None,
    }
}

/// The text a preserved calculation carries: its references **resolved**, because a `calc` argument is
/// still parsed as SassScript (`::variable/number/simple_unit`, `0′.3b-2`'s lesson that special-call
/// arguments resolve variables while `+`/`-` stay non-arithmetic). A run with nothing dynamic in it, and
/// one whose reference is unbound, both fall back to the normalised lexeme text — an unbound `$x` inside
/// `calc( … )` keeps its own text and never becomes a fatal error
/// (`::variable/not_parsed_as_interpolation/*`).
fn resolved(toks: &[Tok], env: &Env) -> String {
    let dynamic = meaningful(toks)
        .into_iter()
        .any(|tok| matches!(tok, Tok::Var(_) | Tok::Interp(_)));
    let kept = dynamic
        .then(|| expr::interpolated(toks, env))
        .flatten()
        .and_then(|built| match built.eval(env) {
            Ok(value) => Some(value.to_css()),
            // The failure here means "this argument is not readable yet", not "the program is wrong":
            // an unbound reference inside `calc( … )` is defined passthrough, so it stops as a decline
            // and the caller keeps the lexeme text. Which failures terminate stays in `exec::evaluated`.
            Err(_) => None,
        });
    match kept {
        Some(css) => css,
        None => text(toks),
    }
}

/// A run as a calculation call, if it is one whole: a lexeme that ends at a `(` whose text is a
/// calculation name, and an argument list closing at the `)` that returns the bracket balance to zero.
/// Depth is counted over the **whole** run, so a nested call keeps its own closer — counting only from
/// the callee handed `var`'s `)` to `calc` and turned `calc(var(--c))` into `calc(--c)`.
///
/// Nothing but layout may sit on either side: a leading `1px + ` is an operation this slice does not
/// read, and a trailing lexeme means the run is not one call.
fn find(toks: &[Tok]) -> Option<Shell> {
    let cut = open_at(toks);
    let callee = cut.iter().enumerate().find_map(|(offset, tok)| match tok {
        Tok::Word(word) => word
            .strip_suffix('(')
            .filter(|name| name_exists(name))
            .map(|name| (offset, name.to_string(), name.to_lowercase())),
        _ => None,
    })?;
    let closing = close_index(&cut[callee.0..])?;
    let position = callee.0 + closing;
    let layout = |run: &[Tok]| {
        run.iter()
            .all(|tok| matches!(tok, Tok::Space(_) | Tok::Silent(_)))
    };
    layout(&cut[..callee.0])
        .then(|| Shell {
            name: callee.1,
            lower: callee.2,
            inner: cut[callee.0 + 1..position].to_vec(),
        })
        .filter(|_| layout(&cut[position + 1..]))
}

/// The offset of the `Punct(')')` that closes the call starting at the front of this run, or `None` when
/// it never closes. A `…(` lexeme opens one level, a word that both opens and closes (`url(x)`) nets
/// zero, and only a position after the callee can close it.
fn close_index(run: &[Tok]) -> Option<usize> {
    run.iter()
        .enumerate()
        .skip(1)
        .scan(delta(run.first()), |depth, (offset, lexeme)| {
            *depth += delta(Some(lexeme));
            Some((*depth, offset))
        })
        .find(|(depth, _)| *depth <= 0)
        .map(|(_, offset)| offset)
}

/// How much one lexeme moves the bracket depth. A word contributes its own balance, which is why
/// `Word("calc(")` opens one level and a self-closing `Word("url(x)")` nets to zero.
fn delta(lexeme: Option<&Tok>) -> i32 {
    match lexeme {
        Some(Tok::Word(word)) => {
            word.matches('(').count() as i32 - word.matches(')').count() as i32
        }
        Some(Tok::Punct('(')) => 1,
        Some(Tok::Punct(')')) => -1,
        _ => 0,
    }
}

/// Re-cut a run so every bracket owns its place; non-word lexemes are carried unchanged.
fn open_at(toks: &[Tok]) -> Vec<Tok> {
    toks.iter()
        .flat_map(|tok| match tok {
            Tok::Word(word) => cut_open(word),
            other => vec![other.clone()],
        })
        .collect()
}

/// One word cut at each `(` and each `)` — the three shapes the traces show:
/// * `Word("calc(1px)")` → `calc(`, `1px`, `)` — an ordinary declaration value, one lexeme holding both
///   brackets. The `(` stays glued to the text **before** it, because ending at `(` is what marks a
///   callee (gluing it to the piece after produced `Word("calc")`, nothing matched, and every case stayed
///   verbatim);
/// * `Word("calc(1px")` → `calc(`, `1px` — a run whose closing `)` is outside it;
/// * `Word("calc( ( 1 ) )")` → `calc(`, `(`, `1`, `)`, `)` — a paren with layout around it is released as
///   a bare `Punct('(')`, since that whitespace is layout and the group is not a callee.
///
/// Text between brackets keeps its inner spaces: Sass's spacing law is what makes `1px + 1%` an
/// operation, and dropping them turned every calculation's content into `1px+1%`.
fn cut_open(word: &str) -> Vec<Tok> {
    let (pieces, tail) = word.chars().fold(
        (Vec::<Tok>::new(), String::new()),
        |(mut pieces, text), c| match c {
            '(' => {
                let callee = !text.is_empty() && text.trim() == text;
                if callee {
                    pieces.push(Tok::Word(format!("{text}(")));
                } else {
                    let body = text.trim();
                    pieces.extend((!body.is_empty()).then(|| Tok::Word(body.to_string())));
                    pieces.push(Tok::Punct('('));
                }
                (pieces, String::new())
            }
            ')' => {
                let body = text.trim();
                pieces.extend((!body.is_empty()).then(|| Tok::Word(body.to_string())));
                pieces.push(Tok::Punct(')'));
                (pieces, String::new())
            }
            other => {
                let mut text = text;
                text.push(other);
                (pieces, text)
            }
        },
    );
    let body = tail.trim();
    pieces
        .into_iter()
        .chain((!body.is_empty()).then(|| Tok::Word(body.to_string())))
        .collect()
}

/// The argument list of a shell, group by group. The split runs on the bracket-cut view (a `,` must be
/// outside the glued lexeme to count as a separator).
fn arguments(shell: &Shell, env: &Env) -> Vec<Outcome> {
    calls::top_level_groups(&shell.inner)
        .into_iter()
        .map(|group| classify(group, env))
        .collect()
}

/// One argument, read structurally first and as a value second.
fn classify(toks: Vec<Tok>, env: &Env) -> Outcome {
    let content = open_at(&toks);
    let run = meaningful(&content);
    if run.is_empty() {
        return Outcome::Opaque;
    }
    // Redundant parentheses are peeled only far enough to *try*: `(1px)` is the number `1px`
    // (`calc/parens::number` wants `calc((1px))` → `1px`), while `(var(--c))`, `((var(--c)))` and `(d)`
    // are content that keeps its wrapper (`::var/direct`, `::double_preserved`, `::identifier`), so a
    // group that yields no value puts its own parentheses back.
    let group = wrapped(&content);
    if let Some(inside) = group {
        return match classify(inside.to_vec(), env) {
            outcome @ (Outcome::Collapsed(_) | Outcome::Nested(_) | Outcome::Named(_)) => outcome,
            Outcome::Kept(text) => Outcome::Kept(format!("({text})")),
            Outcome::Opaque => Outcome::Kept(format!("({})", resolved(inside, env))),
        };
    }
    if let Some(shell) = find(&content) {
        return match to_value(&shell, &arguments(&shell, env), env) {
            Some(value @ (Value::Number(..) | Value::Bool(..))) => Outcome::Collapsed(value),
            // Only a `calc` hands its body over (that is the dropped shell); a `clamp` keeps its own
            // name, which is what `::calculation/clamp/preserved` prints as `clamp(1%, 2px, 3%)`.
            Some(Value::Calculation(name, inner)) if is_calc(&name) => Outcome::Nested(inner),
            Some(Value::Calculation(name, inner)) => Outcome::Named(format!("{name}({inner})")),
            Some(value) => Outcome::Kept(value.to_css()),
            None => Outcome::Opaque,
        };
    }
    // A lone `$var` is a **lookup**: a bound calculation carries its own form out of the outer shell
    // (`::variable/calculation`: `$a: calc(1px + 1%)` then `calc($a)` → `calc(1px + 1%)`), a number
    // collapses straight out (`syntax/extra_whitespace/number`), and any other bound value is content
    // that stays inside it — printed as the value it resolved to, which is how
    // `::variable/unquoted_string` turns `calc($a)` into `calc(foobar)`. An *unbound* reference keeps its
    // own text and must not become a fatal error (`::variable/not_parsed_as_interpolation/*`).
    if let [Tok::Var(name)] = run.as_slice() {
        return match env.get(name) {
            // The bound calculation's *body* is what replaces the outer wrapper, so `calc($a)` over
            // `$a: calc(1px + 1%)` prints `calc(1px + 1%)` — one shell, not two.
            Ok(Value::Calculation(_, inner)) => Outcome::Nested(inner.clone()),
            Ok(value) if collapsible(&value) => Outcome::Collapsed(value),
            // A bound number with a compound unit is calculation-only too: decline rather than print a
            // bare `1(px*rad)^-1` outside any `calc( … )`.
            Ok(Value::Number(_, unit)) if !unit.is_plain() => Outcome::Opaque,
            Ok(value) => Outcome::Kept(value.to_css()),
            Err(_) => Outcome::Opaque,
        };
    }
    // An unknown callee is not evidence of a value: `calc(c())` keeps its `calc` (`::function/css`).
    if !calls_are_readable(&run) {
        return Outcome::Opaque;
    }
    // A leading sign that is glued to its number is part of the **literal**, not an operation:
    // `::number/leading_plus` wants `calc(+1px)` → `1px` while the lexer delivered `Op('+')` and
    // `Word("1px")` as two lexemes. Any other operator stays unreadable (1.4d owns it).
    let signed = without_leading_sign(&content);
    let run = meaningful(&signed);
    // A real operation stays **unevaluated in this slice**: `calc(1px + 1%)` and `calc(1px + 2px)` both
    // keep their text, because deciding when a `+` inside a calculation folds is 1.4d's question (its own
    // `simplify.hrx` README limits the real rule to "number + number where both are compatible").
    if run.iter().any(|tok| matches!(tok, Tok::Op(_))) {
        return Outcome::Opaque;
    }
    match expr::argument_expr(&signed, env).map(|built| built.eval(env)) {
        // A number or a boolean leaves its shells; anything else is content that stays inside them.
        // Interpolation falls in the second group by the type it produces — `Expr::Concat` evaluates to a
        // `Raw` — which is what `::interpolation/number` asks for: `calc(#{1px + 2px})` → `calc(3px)`,
        // because the fixture's own comment says an interpolated value "is always treated as an unquoted
        // string".
        Some(Ok(value)) if collapsible(&value) => Outcome::Collapsed(value),
        // Division can produce a compound unit (`1 / 1px / 1rad`), and such a number is
        // calculation-only: the run keeps the text the source wrote
        // (`values/numbers/units/multiple::multiple_denominators`), which is what `Opaque` means here.
        Some(Ok(Value::Number(_, unit))) if !unit.is_plain() => Outcome::Opaque,
        Some(Ok(value)) => Outcome::Kept(value.to_css()),
        Some(Err(_)) | None => Outcome::Opaque,
    }
}

/// The content of a whole parenthesised group, or `None` when the run is not one.
fn wrapped(content: &[Tok]) -> Option<&[Tok]> {
    let first = content.iter().position(|tok| !is_layout(tok))?;
    let last = content.iter().rposition(|tok| !is_layout(tok))?;
    (last > first + 1 && opens_group(&content[first]) && closes_group(&content[last]))
        .then(|| &content[first + 1..last])
}

/// An argument's text, in the form the corpus states it: whitespace runs folded, and only the spaces a
/// calculation never prints are removed — one **after** `(`, one **before** `)` or `,`
/// (`calc( ( 1 ) )` → `(1)`, `min( 1% , 2px )` → `min(1%, 2px)`, and `1px + (2px)` keeps its space).
///
/// Structure is already decided by the lexeme pass; this normalises whitespace only. The spaces inside a
/// glued word (`Word("1px + 1%")`) are the sole record of the spacing the corpus expects, so dropping
/// them would be the bug rather than the shortcut.
fn text(toks: &[Tok]) -> String {
    render(toks)
        .chars()
        .fold((String::new(), false), |(mut out, pending), c| {
            if c == ' ' {
                let had_content = !out.is_empty();
                return (out, had_content);
            }
            let suppress = matches!(c, ')' | ',') || out.ends_with('(');
            if pending && !suppress {
                out.push(' ');
            }
            out.push(c);
            (out, false)
        })
        .0
}

/// The group's lexemes with a **leading sign merged into the number it is glued to**: `Op('+')` then
/// `Word("1px")` (no layout between them) becomes `Word("+1px")`, the shape `number_lexeme` already
/// reads. A sign separated by whitespace, or any second operator, is left exactly as it was so the
/// operation stays unreadable — 1.4d's question, not this slice's.
fn without_leading_sign(content: &[Tok]) -> Vec<Tok> {
    match content {
        [Tok::Op(sign), Tok::Word(number), rest @ ..] if matches!(sign, '+' | '-') => {
            let mut merged = vec![Tok::Word(format!("{sign}{number}"))];
            merged.extend(rest.iter().cloned());
            merged
        }
        _ => content.to_vec(),
    }
}

/// Whether a value may leave its shells: a **boolean**, or a number whose unit is expressible outside a
/// calculation (see [`Unit::is_plain`]). A `Raw` never qualifies, and that one boundary is what keeps the
/// already-green cases of this cluster green (`calc(foobar)`, `calc(c())`, `calc(var(--c))`,
/// `calc(#{1px + 2px})`, `calc((#{1px + 2px}))`).
fn collapsible(value: &Value) -> bool {
    match value {
        Value::Bool(..) => true,
        Value::Number(_, unit) => unit.is_plain(),
        _ => false,
    }
}

/// The readable parts of an argument list, joined the way the corpus separates them (`, `).
fn rendered(parts: &[Outcome]) -> String {
    parts
        .iter()
        .map(|part| match part {
            Outcome::Collapsed(value) => value.to_css(),
            Outcome::Nested(inner) | Outcome::Named(inner) | Outcome::Kept(inner) => inner.clone(),
            Outcome::Opaque => String::new(),
        })
        .collect::<Vec<String>>()
        .join(", ")
}

/// Every lexeme that is not layout — the view structure is read off.
fn meaningful(toks: &[Tok]) -> Vec<&Tok> {
    toks.iter().filter(|tok| !is_layout(tok)).collect()
}

fn is_layout(tok: &Tok) -> bool {
    matches!(tok, Tok::Space(_) | Tok::Silent(_))
}

/// Whether a lexeme opens a group. After [`cut_open`] a grouping paren is always its own `Punct`.
fn opens_group(tok: &Tok) -> bool {
    matches!(tok, Tok::Punct('('))
}

/// Whether a lexeme closes a group. `Op('-')` deliberately does **not**: `--c` starts with that dash, and
/// reading it as a closer turned `calc(var(--c))` into `calc(--c)`.
fn closes_group(tok: &Tok) -> bool {
    matches!(tok, Tok::Punct(')'))
}

fn name_exists(name: &str) -> bool {
    CALCULATION_NAMES
        .iter()
        .any(|call| name.eq_ignore_ascii_case(call))
}

fn is_calc(name: &str) -> bool {
    name == "calc"
}

/// Whether every call lexeme in a run is one this slice can read: a registered builtin (so
/// `calc(math.round(2.3))` → `2`, which `::function/sass/namespace` asks for) or another calculation
/// shell. Anything else — `c()`, `var()` — answers "not evidence", which is how `::function/css`
/// (`calc(c())` → `calc(c())`) survives this rule.
fn calls_are_readable(run: &[&Tok]) -> bool {
    run.iter()
        .filter_map(|tok| {
            let Tok::Word(word) = *tok else {
                return None;
            };
            (word.ends_with('(') && word.len() > 1).then_some(word)
        })
        .all(|word| {
            let name = word.trim_end_matches('(');
            crate::builtins::signature(name).is_some() || name_exists(name)
        })
}
