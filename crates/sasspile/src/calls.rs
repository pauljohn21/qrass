//! Call syntax read at the **lexeme** level (tasks 1.7a): where a declaration value is a call into a
//! builtin we have a signature for, its arguments are split and handed over for evaluation.
//!
//! Two rules decide everything here, both learned the hard way (1.2b, 0′.3b):
//! • a `$name:` in front of an argument is a **label**, never a reference;
//! • a classifier can never be finer than the lexer. Neither `(` nor `)` is a structural character and
//!   `(` does not flush the pending word, so the source `math.div(6)` arrives as the single lexeme
//!   `Word("math.div(6")` — callee, opening paren and the digits after it welded together. Reading a call
//!   therefore **normalises the paren boundaries first** ( [`brackets`] ), which is a local view of the
//!   lexemes: nothing upstream changes, and `url(http://x)`/`-a-calc(` keep their existing behaviour.

use crate::builtins;
use crate::lexer::Tok;

/// One argument of a call: its optional `$label:` and the lexemes that make up its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    pub label: Option<String>,
    pub toks: Vec<Tok>,
}

/// `name(…)` from a token run, **only when `name` resolves to a known builtin signature**.
///
/// The signature gate is the whole safety of this step: `color.hwb($hue: 180)` (no signature yet) and any
/// CSS-native `var(…)`/`calc(…)` never reach argument binding, so they keep today's verbatim behaviour
/// instead of being half-evaluated. Callers must therefore treat `None` as "not a call we know".
pub fn known_call(toks: &[Tok]) -> Option<(String, Vec<Argument>)> {
    let (name, args) = split(&brackets(toks))?;
    builtins::signature(&name).map(|found| (found.name.to_string(), args))
}

/// Re-cut a run so each bracket owns a lexeme: `Word("math.div(6")` → `Word("math.div(")`, `Word("6")`.
///
/// A callee keeps its opening paren (that is what marks it as one) while a `)` always becomes its own
/// `Punct`. Text between brackets is copied unchanged, so nothing that renders later can tell this
/// normalised view apart from the source.
fn brackets(toks: &[Tok]) -> Vec<Tok> {
    toks.iter()
        .flat_map(|tok| match tok {
            Tok::Word(word) => cut(word),
            other => vec![other.clone()],
        })
        .collect()
}

/// One word split at its brackets, in a single forward pass over its characters.
fn cut(word: &str) -> Vec<Tok> {
    let (lexemes, pending) = word.chars().fold(
        (Vec::<Tok>::new(), String::new()),
        |(mut lexemes, mut pending), c| {
            match c {
                '(' => {
                    pending.push('(');
                    lexemes.push(Tok::Word(std::mem::take(&mut pending)));
                }
                ')' => {
                    if !pending.is_empty() {
                        lexemes.push(Tok::Word(std::mem::take(&mut pending)));
                    }
                    lexemes.push(Tok::Punct(')'));
                }
                other => pending.push(other),
            }
            (lexemes, pending)
        },
    );
    lexemes
        .into_iter()
        .chain((!pending.is_empty()).then_some(Tok::Word(pending)))
        .collect()
}

/// Split `callee( … )` into the callee name and its top-level arguments.
///
/// The call must be the **whole** run: `math.div(6, 3) * 1px` is a unit expression whose result carries a
/// compound unit this compiler does not model yet, and inventing a partial value would print a wrong
/// number. Depth is counted over lexemes, not recursed into a tree: a `…(` lexeme opens one level, a
/// `Punct(')')` closes it, and only a depth-0 `,` separates arguments.
fn split(toks: &[Tok]) -> Option<(String, Vec<Argument>)> {
    let run: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_) | Tok::Silent(_)))
        .collect();
    let (Tok::Word(callee), rest) = run.split_first()? else {
        return None;
    };
    let name = callee.strip_suffix('(').filter(|name| !name.is_empty())?;
    if !matches!(rest.last(), Some(Tok::Punct(')'))) {
        return None;
    }
    let inner = rest[..rest.len() - 1]
        .iter()
        .map(|tok| (*tok).clone())
        .collect::<Vec<Tok>>();
    let arguments = group(&inner)
        .into_iter()
        .filter(|arg| !arg.is_empty())
        .map(argument)
        .collect::<Vec<Argument>>();
    // `name()` is a real zero-argument call; lexemes inside that produced no argument did not close the
    // way a call must, so the run is not read as one.
    if arguments.is_empty() && !inner.is_empty() {
        return None;
    }
    Some((name.to_string(), arguments))
}

/// Cut a run's lexemes at every depth-0 comma. Owned slices, no index cursor kept between calls.
fn group(toks: &[Tok]) -> Vec<Vec<Tok>> {
    let (groups, current) = toks.iter().fold(
        (Vec::<Vec<Tok>>::new(), Vec::<Tok>::new()),
        |(mut groups, mut current), tok| {
            match tok {
                Tok::Punct(',') if depth_of(&current) == 0 => {
                    groups.push(std::mem::take(&mut current));
                }
                other => current.push(other.clone()),
            }
            (groups, current)
        },
    );
    groups.into_iter().chain([current]).collect()
}

/// How many parentheses a partial argument has opened and not yet closed.
fn depth_of(toks: &[Tok]) -> i32 {
    toks.iter().fold(0, |depth, tok| match tok {
        Tok::Word(word) if word.ends_with('(') => depth + 1,
        Tok::Punct(')') => depth - 1,
        _ => depth,
    })
}

/// A single argument: peel a leading `$label:` (a name, not a reference) off the front.
fn argument(toks: Vec<Tok>) -> Argument {
    let label = match toks.as_slice() {
        [Tok::Var(name), Tok::Punct(':'), ..] => Some(name.clone()),
        _ => None,
    };
    let value = if label.is_some() {
        toks[2..].to_vec()
    } else {
        toks
    };
    Argument { label, toks: value }
}
