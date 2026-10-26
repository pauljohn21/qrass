//! Turning a calculation's lexeme run into **depth-tagged runs**, so the reduction in [`crate::calc`] never
//! needs a tree or a recursive descent: one pass assigns every lexeme a `level` and the `owner` index of the
//! bracket that owns it, a call and its balanced argument list become a *single* lexeme, and a run that a
//! group split in two is merged back into one. Order is document order throughout, so a later fold can walk
//! the levels deepest-first and see each group already reduced.
//!
//! The two lexeme-level facts this file owns, both bought with a real bug: a **call is one operand** (cutting
//! `var(--c)` into `var(`, `--c`, `)` left no chain readable around it), and a **group bracket is not a call**
//! (treating `var(` as an opener ate its closer and turned `calc((1 + var(--c)))` into an unreadable run).

use crate::lexer::Tok;

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
