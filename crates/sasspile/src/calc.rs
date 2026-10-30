//! Reading the arithmetic **inside** a `calc( … )`, flattened rather than interpreted.
//!
//! The GC-shaped way to do this is to build a tree of nodes that point at other nodes and walk it twice —
//! once to simplify, once to print. This module keeps no such structure. One pass turns the lexeme run
//! into a document-order list of **runs tagged with their depth** (`level` and `owner` are fields of the
//! data, so nesting becomes an index, not a call stack), and the runs are then reduced **deepest level
//! first**, which guarantees a group is already a single owned summary by the time its parent looks at it.
//! A summary holds a folded value, the text it prints (written at the moment it was formed, never
//! re-derived) and its precedence — no child pointers, no recursion, no traversal flags.
//!
//! ## The laws, each traced to a fixture
//!
//! *A chain folds while every step is computable*: `calc(1px - 2px)` → `-1px` (`operator/minus::simplified`),
//! `calc(1px + 20px - 300px + 4000px - 50000px)` → `-46279px` and `calc(2 * 3 + 4 / 5 - 6)` → `0.8`
//! (`operator/precedence::simplified/*`), `calc(1px * 2)` → `2px` (`operator/times::simplified`). One
//! impossible step stops only itself: `calc(1px - 2%)` stays (`minus::preserved/number`), and a compound
//! unit stays inside the shell (`Unit::is_plain`, `values/numbers/units/multiple::multiple_denominators`).
//! `+`/`-` reuse [`Value::plus`]/[`Value::minus`] and `*`//`/` reuse [`Value::times`]/[`Value::divided_by`],
//! so the unit law lives in one place, shared with the operators outside `calc`.
//!
//! *Parentheses are decided by precedence at the moment of joining*, never copied and never re-derived by a
//! second pass: `calc((1 + var(--c)))` → `calc(1 + var(--c))` (`operator/var::indirectly_parenthesized`),
//! `calc(1px + (2% - 3em))` → `calc(1px + 2% - 3em)` (`precedence::preserved/additive/parens`),
//! `calc(1px * (2 / var(--c)))` → `calc(1px * 2 / var(--c))` (`…/multiplicative/default/parens`), while
//! `calc(1px / (2 * var(--c)))` (`…/needs_parens/parens`), `calc(1px * (2 + var(--c)))`
//! (`…/multiplicative_then_additive/parens`) and `calc(1px - (2% + var(--c)))` (`operator/minus::preserved/plus`)
//! keep theirs. A group holding a single operand keeps **its own** parentheses — that text belongs to the
//! operand (`var::directly_parenthesized`), which is also what a nested `calc(var(--c))` becomes
//! (`var::calculation`), whereas a nested `calc( … )` holding a chain splices its terms in
//! (`precedence::preserved/additive/calculation`).
//!
//! *What cannot be read declines*: a run whose lexemes do not alternate into operands and operators —
//! adjacent operands are a **space-separated list** and stay as written (`calc/space.hrx::var/before`:
//! `calc(var(--c) 1)` → `calc(var(--c) 1)`), an operator missing the whitespace `+`/`-` requires, a bracket
//! whose group never closes inside the argument, or an interpolation whose body fails to evaluate (turning
//! that into text would swallow a fatal error). The caller then keeps the source text untouched.

use crate::brackets::{
    Entry, ROOT, Run, builtin_call, is_layout, merge_calls, opens, partition, spaced, trim_layout,
};
use crate::env::Env;
use crate::expr;
use crate::expr::BinOp;
use crate::lexer::Tok;
use crate::unit::Unit;
use crate::value::Value;

/// What `calculation` needs from one argument: whether it can be read at all, whether it is a value that
/// may leave the `calc( … )` wrapper, or what it should print inside the wrapper.
pub enum Reading {
    Declined,
    Value(Value),
    Expression(Content),
}

/// One content of a chain: the value it folded to (when every step was computable), the text it prints,
/// and the precedence that lets its parent decide about parentheses. `3` is a single operand.
///
/// This is also what a `Value::Calculation` stores, so a **bound calculation can be split back into
/// arithmetic**: `calc($number / 1px)` over `$number: calc(1px * 1rad / 1ms / 1Hz)` cancels to
/// `calc(1rad / 1ms / 1Hz)` (`values/numbers/units/multiple::division/cancels/numerator/once`) — which the
/// normalised text alone could not express.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Content {
    /// the magnitude and unit the chain folded to, when every step computed — kept as data rather than as a
    /// `Value` so a calculation's content cannot point back into the value it belongs to
    pub(crate) folded: Option<(f64, Unit)>,
    pub(crate) printed: String,
    pub(crate) precedence: u8,
}

/// A standalone value as content: its CSS text, and its number when it is one.
fn content_of(value: &Value) -> Content {
    Content {
        folded: match value {
            Value::Number(value, unit) => Some((*value, unit.clone())),
            _ => None,
        },
        printed: value.to_css(),
        precedence: 3,
    }
}

/// One step of the reduced chain: an operand, or the operator that joins the next one.
#[derive(Clone)]
enum Slot {
    Operand(Content),
    Join(BinOp),
}

/// A `calc( … )` shell (case-insensitively): the only name whose body splices into the parent.
pub(crate) fn is_calc_name(callee: &str) -> bool {
    callee.eq_ignore_ascii_case("calc")
}

/// Read one calculation argument.
pub fn read(toks: &[Tok], env: &Env) -> Reading {
    let flat = trim_layout(&expr::release_slashes(toks));
    // A call is **one operand**: cutting `var(--c)` into `var(`, `--c`, `)` would make it three lexemes and
    // no chain could be read around it (the same lesson as 1.4a's bracket glue — classification cannot be
    // finer than the lexeme). `calc(` alone keeps its brackets, because splicing a nested calculation is
    // itself a law (`precedence::preserved/additive/calculation`).
    // A registered builtin call that **is** the whole argument is the builtin table's business, not this
    // module's: `calc(math.round(2.3))` → `2` (`no_operator::function/sass/namespace`). Mixed with other
    // operands no fixture states an answer, so such a run declines instead of inventing one.
    if let Some(value) = whole_call(&flat, env) {
        return match value {
            Value::Number(number, unit) if unit.is_plain() => {
                Reading::Value(Value::Number(number, unit))
            }
            value => Reading::Expression(content_of(&value)),
        };
    }
    let merged = merge_calls(&flat);
    if merged
        .iter()
        .any(|tok| matches!(tok, Tok::Word(word) if builtin_call(word)))
    {
        return Reading::Declined;
    }
    let runs = partition(&merged);
    let deepest = runs.iter().map(|run| run.level).max().unwrap_or(0);
    // Deepest first, so a group is one summary before its parent reads it. Owners are bracket indices,
    // and a child's bracket always sits after its parent's, so a level-ordered fold is enough.
    let done = (0..=deepest).rev().collect::<Vec<u32>>().into_iter().fold(
        Vec::<(usize, Content)>::new(),
        |mut reduced, level| {
            let finished = runs
                .iter()
                .filter(|run| run.level == level)
                .filter_map(|run| reduce(run, &reduced, env).map(|term| (run.owner, term)))
                .collect::<Vec<(usize, Content)>>();
            reduced.extend(finished);
            reduced
        },
    );
    done.iter()
        .find(|(owner, _)| *owner == ROOT)
        .map(|(_, root)| finishing(root))
        .unwrap_or(Reading::Declined)
}

/// A folded **plain** number leaves the shell (`calc(1px * 2)` → `2px`, `operator/times::simplified`);
/// anything else, including a compound unit (`1 / 1px / 1rad`), prints inside it
/// (`values/numbers/units/multiple::multiple_denominators`).
fn finishing(content: &Content) -> Reading {
    match &content.folded {
        Some((value, unit)) if unit.is_plain() => {
            Reading::Value(Value::Number(*value, unit.clone()))
        }
        _ => Reading::Expression(content.clone()),
    }
}

/// Reduce one run: split at the additive operators (the loosest level), fold each piece at its `*`/`/`
/// operators (the tighter one), then join the pieces left to right. Two folds, so precedence needs no
/// recursion; `1 - 2 - 3` comes out `(1 - 2) - 3` because joining walks in document order.
fn reduce(run: &Run, done: &[(usize, Content)], env: &Env) -> Option<Content> {
    let entries = run.entries.as_slice();
    let slots = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !is_layout(&entry.tok))
        .map(|(index, entry)| slot(entry, index, entries, done, env))
        .collect::<Option<Vec<Slot>>>()?;
    let pieces =
        slots
            .into_iter()
            .fold(Vec::<(Option<BinOp>, Vec<Slot>)>::new(), |mut acc, slot| {
                match slot {
                    Slot::Join(op) if matches!(op, BinOp::Add | BinOp::Sub) => {
                        acc.push((Some(op), Vec::new()));
                    }
                    other => {
                        if acc.is_empty() {
                            acc.push((None, Vec::new()));
                        }
                        if let Some((_, piece)) = acc.last_mut() {
                            piece.push(other);
                        }
                    }
                }
                acc
            });
    pieces
        .into_iter()
        .map(|(op, piece)| chain(piece).map(|term| (op, term)))
        .collect::<Option<Vec<(Option<BinOp>, Content)>>>()?
        .into_iter()
        .fold(None::<Content>, |left, (op, right)| match (left, op) {
            (None, _) => Some(right),
            (Some(left), Some(op)) => Some(join(left, op, right)),
            // Two pieces with no operator between them are a list, not a sum: the run declines.
            (Some(_), None) => None,
        })
}

/// Fold one multiplicative piece: operands and joining operators must alternate, exactly as in
/// [`crate::operations`], and the strict alternation test is what rejects a run this cannot model.
fn chain(piece: Vec<Slot>) -> Option<Content> {
    let operands = piece
        .iter()
        .step_by(2)
        .map(|slot| match slot {
            Slot::Operand(term) => Some(term.clone()),
            Slot::Join(_) => None,
        })
        .collect::<Option<Vec<Content>>>()?;
    let joins = piece
        .iter()
        .skip(1)
        .step_by(2)
        .map(|slot| match slot {
            Slot::Join(op) => Some(*op),
            Slot::Operand(_) => None,
        })
        .collect::<Option<Vec<BinOp>>>()?;
    if operands.is_empty() || joins.len() + 1 != operands.len() {
        return None;
    }
    let head = operands.first()?.clone();
    Some(
        operands
            .into_iter()
            .skip(1)
            .zip(joins)
            .fold(head, |left, (right, op)| join(left, op, right)),
    )
}

/// One lexeme as a slot: an operator that the spacing law accepts, a group already reduced, or an operand.
fn slot(
    entry: &Entry,
    index: usize,
    entries: &[Entry],
    done: &[(usize, Content)],
    env: &Env,
) -> Option<Slot> {
    if let Some(op) = operator(&entry.tok) {
        // `+`/`-` are operators only with whitespace on both sides — that law keeps `10px -5px` a
        // two-item list. `*` and a released `/` carry no such ambiguity.
        return match (op, spaced(entries, index)) {
            (BinOp::Add | BinOp::Sub, false) => None,
            _ => Some(Slot::Join(op)),
        };
    }
    operand(entry, done, env).map(Slot::Operand)
}

/// An operand: a group, resolved by the position of the bracket that opened it (which `partition` left in
/// the parent run as the placeholder), or a single lexeme.
fn operand(entry: &Entry, done: &[(usize, Content)], env: &Env) -> Option<Content> {
    if is_layout(&entry.tok) {
        return None;
    }
    if opens(&entry.tok) {
        return done
            .iter()
            .find(|(owner, _)| *owner == entry.index)
            .map(|(_, term)| parenthesised(term));
    }
    leaf(&entry.tok, env)
}

/// A group around a single operand keeps its own parentheses; a group around a chain has them re-derived
/// by precedence (`operator/var::directly_parenthesized` vs `var::indirectly_parenthesized`).
fn parenthesised(term: &Content) -> Content {
    match term.precedence {
        3 => Content {
            folded: term.folded.clone(),
            printed: format!("({})", term.printed),
            precedence: 3,
        },
        _ => term.clone(),
    }
}

/// A single lexeme as a term. An interpolation is a **value** (`calc(#{1px + 2px})` → `calc(3px)`,
/// `no_operator::interpolation/number`); a body that does not evaluate declines rather than becoming text,
/// because swallowing that error into a string is what let a fatal `Undefined variable` disappear.
fn leaf(tok: &Tok, env: &Env) -> Option<Content> {
    let atom = |printed: String| {
        Some(Content {
            folded: None,
            printed,
            precedence: 3,
        })
    };
    match tok {
        Tok::Word(word) => Some(match crate::operations::number_lexeme(word) {
            Some((value, unit)) => number(value, &Unit::named(unit)),
            // A word that is not a number may still be one of CSS's **calculation constants**; anything else
            // is the unknown identifier the corpus keeps written (`…::constant/undefined/alone` answers
            // `calc(c)` with `calc(c)`).
            None => constant(word).map_or_else(
                || Content {
                    folded: None,
                    printed: (*word).clone(),
                    precedence: 3,
                },
                |value| number(value, &Unit::unitless()),
            ),
        }),
        // A bound name resolves through the environment. A bound **calculation** enters as the content it
        // already is — folded value, printed form and precedence — which is what lets
        // `calc($number / 1px)` cancel against the bound product
        // (`values/numbers/units/multiple::division/cancels/numerator/once`). A calculation carrying its own
        // name (`min( … )`) keeps that name, because only `calc` splices its body into a parent.
        Tok::Var(name) => match env.get(name) {
            Ok(Value::Number(value, unit)) => Some(number(value, &unit)),
            Ok(Value::Calculation(callee, content)) => Some(if is_calc_name(&callee) {
                content.clone()
            } else {
                Content {
                    folded: content.folded.clone(),
                    printed: format!("{callee}({})", content.printed),
                    precedence: 3,
                }
            }),
            Ok(other) => atom(other.to_css()),
            Err(_) => atom(format!("${name}")),
        },
        // A namespaced reference is a built-in module constant or nothing: no fixture puts one inside a
        // calculation, so an unknown `upstream.$a` keeps its own text exactly as an unbound `$name` above
        // does, instead of becoming a guess about a file this compiler never reads.
        Tok::MemberVar(module, name) => match crate::builtins::module_value(module, name) {
            Some(Value::Number(value, unit)) => Some(number(value, &unit)),
            Some(other) => atom(other.to_css()),
            None => atom(format!("{module}.${name}")),
        },
        Tok::Interp(inner) => match expr::interpolated(inner, env).map(|built| built.eval(env)) {
            Some(Ok(value)) => atom(value.to_css()),
            _ => None,
        },
        Tok::Punct(_) | Tok::Op(_) | Tok::Space(_) | Tok::Silent(_) => None,
    }
}

fn number(value: f64, unit: &Unit) -> Content {
    Content {
        folded: Some((value, unit.clone())),
        printed: unit.calc_form(value),
        precedence: 3,
    }
}

/// A word that names one of CSS's four calculation constants, case-insensitively, with a sign glued to it:
/// `values/calculation/calc/constant.hrx` reads `calc(pi)` → `3.1415926536`, `calc(pI)` the same, `calc(e)`
/// → `2.7182818285`, `calc(infinity)` and `calc(NaN)` back as their own shells (a non-finite number has no
/// plain-CSS spelling — [`crate::value::Value::to_css`] wraps it), and `calc(-infinity)` keeps the sign.
///
/// The **numbers are not repeated here**: they come through [`crate::builtins::module_value`], the same door
/// the `math.$pi` arm of [`leaf`] reads, so `pi` has one value in this crate. What this file owns is the
/// **list of names**, and it is deliberately narrower than that table: the table also carries `epsilon` and
/// `max-safe-integer`, and no golden puts those inside a calculation, so `calc(epsilon)` stays the unknown
/// identifier `…::constant/undefined/alone` says it is.
fn constant(word: &str) -> Option<f64> {
    let (sign, name) = word
        .strip_prefix('-')
        .map_or((1.0, word), |tail| (-1.0, tail));
    ["pi", "e", "infinity", "nan"]
        .into_iter()
        .find(|spell| name.eq_ignore_ascii_case(spell))
        .and_then(|_| crate::builtins::module_value("math", &name.to_ascii_lowercase()))
        .and_then(|value| match value {
            Value::Number(number, _) => Some(number * sign),
            _ => None,
        })
}

/// Join two terms: fold the value when both sides are numbers and the unit law agrees, and write the text
/// once, here, with the parentheses this position calls for.
fn join(left: Content, op: BinOp, right: Content) -> Content {
    let level = if matches!(op, BinOp::Add | BinOp::Sub) {
        1
    } else {
        2
    };
    let printed = format!(
        "{} {} {}",
        wrap(&left, op, Side::Left),
        op.symbol(),
        wrap(&right, op, Side::Right)
    );
    match op.fold(&left, &right) {
        Some((value, unit)) => Content {
            folded: Some((value, unit.clone())),
            printed: unit.calc_form(value),
            precedence: level,
        },
        folded => Content {
            folded,
            printed,
            precedence: level,
        },
    }
}

/// Which side of the operator a child sits on: `-` and `/` are not associative on their right.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// A child is parenthesised exactly where dropping the group would change the value.
fn wrap(child: &Content, parent: BinOp, side: Side) -> String {
    let up = match parent {
        BinOp::Add | BinOp::Sub => 1,
        BinOp::Mul | BinOp::Div => 2,
    };
    let needs = match side {
        Side::Left => child.precedence < up,
        Side::Right => {
            child.precedence < up
                || (child.precedence == up && matches!(parent, BinOp::Sub | BinOp::Div))
        }
    };
    if needs && child.precedence < 3 {
        format!("({})", child.printed)
    } else {
        child.printed.clone()
    }
}

impl BinOp {
    /// The value-level law, reused rather than restated. `None` means this step is not computable, which
    /// is what keeps the operation in the printed text.
    fn fold(self, left: &Content, right: &Content) -> Option<(f64, Unit)> {
        let (Some((a, au)), Some((b, bu))) = (&left.folded, &right.folded) else {
            return None;
        };
        let (left, right) = (Value::Number(*a, au.clone()), Value::Number(*b, bu.clone()));
        match self {
            BinOp::Add => left.plus(&right),
            BinOp::Sub => left.minus(&right),
            BinOp::Mul => left.times(&right),
            BinOp::Div => left.divided_by(&right),
        }
        .ok()
        .and_then(|value| match value {
            Value::Number(value, unit) => Some((value, unit)),
            _ => None,
        })
    }
}

fn operator(tok: &Tok) -> Option<BinOp> {
    let Tok::Op(sign) = tok else {
        return None;
    };
    match sign {
        '+' => Some(BinOp::Add),
        '-' => Some(BinOp::Sub),
        '*' => Some(BinOp::Mul),
        '/' => Some(BinOp::Div),
        _ => None,
    }
}

fn whole_call(run: &[Tok], env: &Env) -> Option<Value> {
    let Tok::Word(head) = run.first()? else {
        return None;
    };
    let closes_last = match run.last() {
        Some(Tok::Punct(')')) => true,
        Some(Tok::Word(word)) => word.ends_with(')') && !word.ends_with('('),
        _ => false,
    };
    if !builtin_call(head) || !closes_last {
        return None;
    }
    match expr::argument_expr(run, env).map(|built| built.eval(env)) {
        Some(Ok(value)) => Some(value),
        _ => None,
    }
}
