//! Turning a calculation's lexeme run into **depth-tagged runs**, so the reduction in [`crate::calc`] never
//! needs a tree or a recursive descent: one pass assigns every lexeme a `level` and the `owner` index of the
//! bracket that owns it, a call and its balanced argument list become a *single* lexeme, and a run that a
//! group split in two is merged back into one. Order is document order throughout, so a later fold can walk
//! the levels deepest-first and see each group already reduced.
//!
//! The two lexeme-level facts this file owns, both bought with a real bug: a **call is one operand** (cutting
//! `var(--c)` into `var(`, `--c`, `)` left no chain readable around it), and a **group bracket is not a call**
//! (treating `var(` as an opener ate its closer and turned `calc((1 + var(--c)))` into an unreadable run).

use crate::env::Env;
use crate::expr;
use crate::lexer::{Tok, render};

/// The owner of the outermost run; no group can collide with it, because group owners are lexeme indices.
pub(crate) const ROOT: usize = usize::MAX;

/// A maximal document-order run at one depth: `owner` is the index of the opening bracket that created it,
/// or [`ROOT`] for the outermost one.
#[derive(Clone)]
pub(crate) struct Run {
    pub(crate) level: u32,
    pub(crate) owner: usize,
    pub(crate) entries: Vec<Entry>,
}

/// A lexeme with its position, its depth, and the `(` that owns the run it belongs to.
#[derive(Clone)]
pub(crate) struct Entry {
    pub(crate) index: usize,
    pub(crate) level: u32,
    pub(crate) owner: usize,
    pub(crate) tok: Tok,
}

/// One pass over the lexemes: depth and ownership become data, the brackets themselves are dropped (they
/// are re-derived when printing), and a run that a group split in two is merged back into one, still in
/// document order — so the reduction sees operands and operators in the order the source wrote them.
pub(crate) fn partition(flat: &[Tok]) -> Vec<Run> {
    let seeded = (Vec::<Run>::new(), 0u32, vec![ROOT; flat.len() + 2]);
    let (runs, _, _) = flat.iter().enumerate().fold(seeded, |acc, (index, tok)| {
        let (mut runs, level, mut owners) = acc;
        let entry = Entry {
            index,
            level,
            owner: owners[level as usize],
            tok: tok.clone(),
        };
        if opens(tok) {
            // The opener stays in the **parent** run as the placeholder the parent reduces through: it is
            // the only handle linking a group to the summary its own run produced.
            push(&mut runs, &entry);
            owners[level as usize + 1] = index;
            runs.push(Run {
                level: level + 1,
                owner: index,
                entries: Vec::new(),
            });
            return (runs, level + 1, owners);
        }
        if level > 0 && closes(tok) {
            return (runs, level - 1, owners);
        }
        push(&mut runs, &entry);
        (runs, level, owners)
    });
    runs.into_iter().fold(Vec::<Run>::new(), |mut acc, run| {
        match acc
            .iter_mut()
            .rev()
            .find(|held| held.owner == run.owner && held.level == run.level)
        {
            Some(held) => held.entries.extend(run.entries),
            None => acc.push(run),
        }
        acc
    })
}

/// Append one entry to the run that owns it, opening that run if this is its first lexeme.
fn push(runs: &mut Vec<Run>, entry: &Entry) {
    match runs
        .iter_mut()
        .rev()
        .find(|run| run.owner == entry.owner && run.level == entry.level)
    {
        Some(run) => run.entries.push(Entry {
            index: entry.index,
            level: entry.level,
            owner: entry.owner,
            tok: entry.tok.clone(),
        }),
        None => runs.push(Run {
            level: entry.level,
            owner: entry.owner,
            entries: vec![Entry {
                index: entry.index,
                level: entry.level,
                owner: entry.owner,
                tok: entry.tok.clone(),
            }],
        }),
    }
}

/// A group-opening lexeme: a standalone `(`, or the `calc(` of a nested calculation whose terms splice into
/// the parent. Every other call — `var(`, `min(`, `c()` — is folded into one operand by [`merge_calls`].
pub(crate) fn opens(tok: &Tok) -> bool {
    match tok {
        Tok::Punct('(') => true,
        Tok::Word(word) => is_calculation_call(word),
        _ => false,
    }
}

/// A `calc(` lexeme: a nested calculation, whose terms splice into the parent rather than becoming an
/// operand of their own.
fn is_calculation_call(word: &str) -> bool {
    word.strip_suffix('(')
        .is_some_and(|name| name.eq_ignore_ascii_case("calc"))
}

/// A call lexeme whose name has a registered signature: those go through the builtin table, not through
/// [`crate::calc`]'s unit algebra.
pub(crate) fn builtin_call(word: &str) -> bool {
    word.strip_suffix('(')
        .is_some_and(|name| crate::builtins::signature(name).is_some())
}

/// Evaluate the run when it is exactly **one** builtin call: a head lexeme ending at `(` and a balanced
/// argument list closing at the last lexeme, with nothing else around it. A body that does not evaluate
/// declines the calculation — the same answer the caller gave before this module existed.
/// One pass that folds a call and its balanced argument list into a single `Word` lexeme, so
/// `1 + var(--c)` is two operands and an operator. Depth is counted as data, and a run whose call never
/// closes inside this argument is left as it came — the chain then declines and the source text stands.
pub(crate) fn merge_calls(run: &[Tok]) -> Vec<Tok> {
    let net = |tok: &Tok| match tok {
        Tok::Punct('(') => 1,
        Tok::Punct(')') => -1,
        Tok::Word(word) => word.matches('(').count() as i32 - word.matches(')').count() as i32,
        _ => 0,
    };
    let head = |index: usize| {
        matches!(&run[index], Tok::Word(word)
            if word.ends_with('(') && !is_calculation_call(word) && !builtin_call(word))
    };
    run.iter()
        .enumerate()
        .fold(
            (Vec::<Tok>::new(), 0usize),
            |(mut merged, skip), (index, tok)| {
                if index < skip {
                    return (merged, skip);
                }
                if !head(index) {
                    merged.push(tok.clone());
                    return (merged, skip);
                }
                let closed = run[index..]
                    .iter()
                    .scan(0i32, |depth, inner| {
                        *depth += net(inner);
                        Some(*depth)
                    })
                    .position(|depth| depth == 0);
                match closed {
                    Some(offset) => {
                        let end = index + offset;
                        merged.push(Tok::Word(crate::lexer::render(&run[index..=end])));
                        (merged, end + 1)
                    }
                    None => {
                        merged.push(tok.clone());
                        (merged, skip)
                    }
                }
            },
        )
        .0
}

fn closes(tok: &Tok) -> bool {
    // Only a standalone `)`: `calculation::cut_open` splits brackets into their own lexemes and
    // [`merge_calls`] has already folded every call (which ends in `)`) into a single operand, so a Word
    // ending with `)` here is a value, not a closer.
    matches!(tok, Tok::Punct(')'))
}

/// Whether this lexeme carries no meaning: layout, and a dropped silent comment.
pub(crate) fn is_layout(tok: &Tok) -> bool {
    matches!(tok, Tok::Space(_) | Tok::Silent(_))
}

/// Whether `+`/`-` at this index has whitespace on both sides, inside its own run.
pub(crate) fn spaced(entries: &[Entry], index: usize) -> bool {
    let layout = |side: usize| side < entries.len() && is_layout(&entries[side].tok);
    index > 0 && layout(index - 1) && layout(index + 1)
}

/// The run without its leading and trailing layout — the spacing *between* operands is evidence the
/// `+`/`-` law reads, so only the edges go.
pub(crate) fn trim_layout(run: &[Tok]) -> Vec<Tok> {
    let first = run.iter().position(|tok| !is_layout(tok));
    let last = run.iter().rposition(|tok| !is_layout(tok));
    match (first, last) {
        (Some(from), Some(to)) => run[from..=to].to_vec(),
        _ => Vec::new(),
    }
}

/// The names whose call is a calculation, matched on the **bare** name case-insensitively. A vendor
/// prefix means it is not one: `-a-calc(` is a single lexeme and
/// `css/functions/special/prefixed/lowercase::calc/script_like` keeps `-a-calc($c)` as text, so the
/// prefix-stripping `expr::special_call` does must not be reused here.
const CALCULATION_NAMES: [&str; 4] = ["calc", "min", "max", "clamp"];

/// A calculation call. `name` is the callee **as written** and `lower` its case-folded form, which is
/// the only thing the law reads: 1.4c forbids turning case normalisation into an output rule, so a
/// preserved calculation prints the name exactly as the source spelled it.
#[derive(Clone)]
pub(crate) struct Shell {
    pub(crate) name: String,
    pub(crate) lower: String,
    pub(crate) inner: Vec<Tok>,
}

/// The text of an argument this module cannot read as a value: references **resolved**, because a `calc`
/// argument is still parsed as SassScript (`::variable/number/simple_unit`, and the 0′.3b-2 lesson that
/// special-call arguments resolve variables while `+`/`-` stay non-arithmetic). A run with nothing dynamic
/// in it, and one whose reference is unbound, both fall back to [`text`] — an unbound `$x` inside
/// `calc( … )` keeps its own text and never becomes a fatal error
/// (`::variable/not_parsed_as_interpolation/*`).
pub(crate) fn resolved(toks: &[Tok], env: &Env) -> String {
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
pub(crate) fn find(toks: &[Tok]) -> Option<Shell> {
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
pub(crate) fn close_index(run: &[Tok]) -> Option<usize> {
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
pub(crate) fn open_at(toks: &[Tok]) -> Vec<Tok> {
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

/// The normalised text of an argument list: whitespace runs folded, and only the spaces a calculation
/// never prints are removed — one **after** `(`, one **before** `)` or `,`
/// (`calc( ( 1 ) )` → `(1)`, `min( 1% , 2px )` → `min(1%, 2px)`, and `1px + (2px)` keeps its space).
///
/// Structure is already decided by the lexeme pass; this normalises whitespace only. The spaces inside a
/// glued word (`Word("1px + 1%")`) are the sole record of the spacing the corpus expects, so dropping
/// them would be the bug rather than the shortcut.
pub(crate) fn text(toks: &[Tok]) -> String {
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
pub(crate) fn without_leading_sign(content: &[Tok]) -> Vec<Tok> {
    match content {
        [Tok::Op(sign), Tok::Word(number), rest @ ..] if matches!(sign, '+' | '-') => {
            let mut merged = vec![Tok::Word(format!("{sign}{number}"))];
            merged.extend(rest.iter().cloned());
            merged
        }
        _ => content.to_vec(),
    }
}

/// Every lexeme that is not layout — the view structure is read off.
pub(crate) fn meaningful(toks: &[Tok]) -> Vec<&Tok> {
    toks.iter().filter(|tok| !is_layout(tok)).collect()
}

fn name_exists(name: &str) -> bool {
    CALCULATION_NAMES
        .iter()
        .any(|call| name.eq_ignore_ascii_case(call))
}

pub(crate) fn is_calc(name: &str) -> bool {
    name == "calc"
}
