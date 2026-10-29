//! Expr — a declaration value (or a selector) as a structured expression, assembled from the
//! `Tok` run in one forward pass — never by re-scanning rendered text — and evaluated
//! bottom-up against the owned [`Env`].
//!
//! P1 keeps the grammar honest by **bailing instead of guessing**: a run becomes an `Expr`
//! only when it is unambiguous. Any lexeme carrying `/`, `*` or `%`, and any shape the
//! algebra does not cover yet, returns `None` and keeps its verbatim text — which is what
//! protects `font: 12px/1.5`, `grid: 1/2` and CSS-native `calc()`/`min()` from being read as
//! Sass. Sass itself supplies the spacing law for `+`/`-`: they are operators only with
//! whitespace on both sides, so `10px - 5px` subtracts while `10px -5px` is a two-item list.

use crate::SassError;
use crate::calls::Argument;
use crate::env::Env;
use crate::lexer::{Tok, canon, render, token_text};
use crate::operations::{arithmetic, concatenation, has_dynamic, number_lexeme, protected};
use crate::unit::Unit;
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    /// `*`, which the lexer emits as its own lexeme and Sass always reads as multiplication
    Mul,
    /// division, recognised only where a `/` cannot mean anything else (see [`crate::unit`] and
    /// `calculation`): in a declaration value a `/` is the slash-separated list.
    Div,
}

/// An expression over declaration-value lexemes. Numbers carry their unit text verbatim.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// a number literal: the magnitude and the **unit it was read with** (never a compound one — those
    /// only come out of `*` and `/`, which is what [`BinOp`] produces)
    Num(f64, Unit),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    /// verbatim lexeme text around an interpolation (`".col-"`, `"0 "`)
    Text(String),
    /// `$name` — the lookup key never carries the `$` (see [`Env`])
    Var(String),
    /// `math.$pi` — a built-in module's constant, arriving as one lexeme from [`Tok::MemberVar`].
    MemberVar(String, String),
    /// a call into a builtin whose signature we have (`1.7a`); each argument keeps its `$label:` so
    /// binding can be by name rather than by position
    Call {
        name: String,
        args: Vec<Argument>,
    },
    /// the parts of an interpolated run, concatenated with **no extra space** at eval
    Concat(Vec<Expr>),
    /// A **space-separated run read as one value**: `rgb(18 52 86)` and `hsl(0 100% 50%)` are CSS Color 4's
    /// one-argument spelling, which Sass reads as a single space list rather than as three arguments
    /// (`core_functions/color/rgb/one_arg/no_alpha.hrx::unitless/numbers` → `rgb(18, 52, 86)`; the `$channels:`
    /// row in the same file proves it is one argument, since a label binds exactly one).
    /// Unlike [`Expr::Concat`] the separators are **kept**, because that is how a space list prints.
    Space(Vec<Expr>),
    /// A **slash-separated list**: CSS Color 4 writes `rgb(0 255 127 / 0)` and the corpus states the same call
    /// as `rgb(list.slash(0 255 127, 0))` — `rgb/one_arg/alpha.hrx::{in_gamut/transparent,slash_list}` expect
    /// one and the same colour — so the alpha is not an operator applied to the last channel but the second
    /// element of a two-element slash list whose first element is a space list.
    Slash(Vec<Expr>),
    /// a `calc( … )` / `min( … )` / `max( … )` / `clamp( … )` read by `crate::calculation` (tasks 1.4a).
    /// It carries the **finished value** rather than a token tree, because the reading needs the
    /// environment (a bound `$a` is the difference between `calc(1px)` and `calc(calc(1px))`), which an
    /// `Expr` node never has; this variant therefore never goes through `eval`'s operand paths.
    Calculation(Value),
}

/// Sass's **special** calls, matched on the base name after a vendor prefix: their arguments are
/// plain CSS, where `+`/`-` are not arithmetic and an **unbound** `$x` is just text rather than an
/// error (`css/functions/special/prefixed/lowercase::calc/script_like` keeps `-a-calc($c)`; the same
/// family's `calc/interpolation` still evaluates `#{0}`, and `values/calculation/calc/no_operator::
/// variable/unquoted_string` resolves a *bound* `$a`). A function that is merely unknown is **not**
/// special: `css/functions/not_special.hrx` computes `-c-type(1 + 1)` → `-c-type(2)`.
///
/// Grows with the builtin registry (P1.7/P2); `min`/`max`/`clamp` are excluded on purpose — their
/// variables resolve too.
const SPECIAL_CALLS: [&str; 4] = ["url", "calc", "element", "expression"];

/// Strip a vendor prefix from a call name (`-a-calc` → `calc`). A `fn`, not a closure: a closure's
/// elided lifetimes cannot tie the returned `&str` to its argument.
fn base_call_name(name: &str) -> &str {
    name.rsplit('-').next().unwrap_or(name)
}

/// Whether a run calls a special (or namespaced, `progid:`-style) function. The name match ignores
/// case, because the corpus keeps `-C-EXPRESSION($d)` verbatim too
/// (`css/functions/special/prefixed/uppercase::expression/script_like`).
pub fn special_call(toks: &[Tok]) -> bool {
    toks.iter().any(|tok| match tok {
        Tok::Word(word) => match word.strip_suffix('(') {
            Some(name) => {
                SPECIAL_CALLS
                    .iter()
                    .any(|call| base_call_name(name).eq_ignore_ascii_case(call))
                    || name.contains(':')
            }
            None => false,
        },
        _ => false,
    }) || toks.windows(2).any(|pair| {
        matches!(pair[0], Tok::Punct(':')) && matches!(&pair[1], Tok::Word(w) if w.ends_with('('))
    })
}

/// Whether a run **passes named arguments** — `f(1, $w: 30%)`, `color.hwb($hue: 180, …)`. There the `$w`
/// is an argument *label*, not a variable reference, so looking it up invents an `Undefined variable`
/// error for a program Sass accepts (`core_functions/color/hwb/four_args::named`,
/// `core_functions/color/is_powerless/named`; measured as 1,611 in-scope cases).
///
/// The test is lexeme-level because a map literal writes the same shape with the opposite intent:
/// `($z: null)` **is** a reference to `$z` as a key. The lexer distinguishes them — a call's `(` is glued
/// to its callee (`Word("hwb(")`) while a group's `(` stands alone (`Word("(")`) — so an argument starts
/// only after a glued `(` or a `,`, and whitespace is not meaningful here.
///
/// A run holding one is not an expression P1 evaluates at all: it keeps its verbatim text, which also
/// means any *other* reference inside it stays unresolved for now (call evaluation arrives with P4).
pub fn named_argument(toks: &[Tok]) -> bool {
    let meaningful: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_)))
        .collect();
    let opens_argument = |tok: &&Tok| match tok {
        Tok::Word(word) => word.ends_with('(') && word.len() > 1,
        Tok::Punct(',') => true,
        _ => false,
    };
    let names_argument = |tok: &&Tok| matches!(tok, Tok::Var(_));
    let is_colon = |tok: &&Tok| matches!(tok, Tok::Punct(':'));
    meaningful.windows(3).any(|window| {
        opens_argument(&window[0]) && names_argument(&window[1]) && is_colon(&window[2])
    })
}

/// Build an `Expr` from a token run, or `None` when the run holds a shape P1 must not read.
///
/// `env` is only for the calculation path ([`crate::calculation`]), the one reading that resolves a
/// reference while deciding the form of the value it belongs to; every other path stays env-free.
pub fn from_tokens(toks: &[Tok], env: &Env) -> Option<Expr> {
    // A call is tried **first**, because its arguments live inside the parentheses: `math.percentage(1%)`
    // carries a `%`, which the CSS-shorthand protection below exists to keep unread — yet here the callee has
    // a signature and the run *is* an expression. This stays safe because `known_call` only matches a name
    // the registry knows, so `calc(1% + 1px)`, `font: 12px/1.5` and `grid: 1/2` fall through to the
    // protection unchanged.
    if let Some((name, args)) = crate::calls::known_call(toks) {
        return Some(Expr::Call { name, args });
    }
    // A calculation is tried **before** the protection test, which exists for CSS shorthands
    // (`font: 12px/1.5`, `grid: 1/2`) and would otherwise refuse `calc(1% + 1px)` forever: inside
    // `calc( … )` a `/`, `*` or `%` is the calculation's own syntax, not a separator. The rule set is
    // deliberately the one the corpus states (`tasks.md` 1.4a): collapse a single value, drop one nested
    // shell, otherwise keep the expression as written. A `…(` lexeme that is not one of those four bare
    // names falls through untouched, so `-a-calc(` and `url(` keep their old reading.
    if let Some(value) = crate::calculation::value(toks, env) {
        return Some(Expr::Calculation(value));
    }
    if protected(toks) {
        return None;
    }
    if has_dynamic(toks) {
        // Arithmetic is tried first even here, because a product of module constants is arithmetic, not
        // interpolation: `math.$pi * 1e15` → `3141592653589793` (`core_functions/math/variables.hrx::pi`),
        // while the interpolation branch printed the constant beside a literal `* 1e15`.
        // This stays narrow through [`operand_expr`]: a run whose dynamic part is a `$var` or a `#{…}` has no
        // operand reading, so `arithmetic` declines on the count guard and the run takes the same path as
        // before — no golden multiplies a user variable, and this does not invent one.
        arithmetic(toks, env).or_else(|| interpolation(toks, env))
    } else {
        arithmetic(toks, env).or_else(|| concatenation(toks))
    }
}

/// One **operand** of an operation: a registered call, a calculation, or a number lexeme.
///
/// This is deliberately narrower than [`from_tokens`]: the operands of a split run are read here once, so
/// no run ever re-enters its own split, and a **user** variable or interpolation operand is not this path's
/// business — [`crate::operations::arithmetic`] drops the whole run when a span has no reading here, which is
/// how such runs fall back to the interpolation branch in [`from_tokens`]. Widening it to calls and
/// calculations is what makes `math.div(6, 3px) * 1px` an operation at all
/// (`core_functions/math/div.hrx::unit/denominator` → `2`), where before the call lexeme marked the run
/// unreadable and the whole declaration stayed verbatim text.
pub(crate) fn operand_expr(toks: &[Tok], env: &Env) -> Option<Expr> {
    if let Some((name, args)) = crate::calls::known_call(toks) {
        return Some(Expr::Call { name, args });
    }
    if let Some(value) = crate::calculation::value(toks, env) {
        return Some(Expr::Calculation(value));
    }
    let meaningful = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_)))
        .collect::<Vec<&Tok>>();
    match meaningful.as_slice() {
        [Tok::Word(word)] => {
            number_lexeme(word).map(|(value, unit)| Expr::Num(value, Unit::named(unit)))
        }
        // A built-in module constant is an operand of arithmetic: `math.$pi * 1e15` → `3141592653589793`
        // (`core_functions/math/variables.hrx::pi`) — without this arm the product declined and the run
        // printed the constant beside a literal `* 1e15`. A plain `$x` is **not** added: no golden in the
        // corpus multiplies a user variable, so its pairing law is not this function's to invent.
        [Tok::MemberVar(module, name)] => match crate::builtins::module_value(module, name) {
            Some(Value::Number(value, unit)) => Some(Expr::Num(value, unit)),
            _ => None,
        },
        _ => None,
    }
}

/// One argument's expression. Unlike a declaration value, an argument **is** an expression even when it is
/// a lone number (`math.div(6, 3)` → `2`), so the "a bare number is not an expression yet" filter that
/// [`crate::exec::State::evaluated`] applies to whole declaration values does not apply here.
pub fn argument_expr(toks: &[Tok], env: &Env) -> Option<Expr> {
    let divided = release_slashes(toks);
    from_tokens(&divided, env).or_else(|| {
        // A single lexeme inside an argument is read as a number even when the `protected` rule would
        // refuse the same text in a declaration value — that rule exists for CSS shorthands
        // (`font: 12px/1.5`), and `math.percentage(1%)` is not one: the corpus wants
        // `$number: Expected 1% to have no units.` from the body, not an unread value.
        let meaningful = divided
            .iter()
            .filter(|tok| !matches!(tok, Tok::Space(_)))
            .collect::<Vec<&Tok>>();
        match meaningful.as_slice() {
            [Tok::Word(word)] => {
                number_lexeme(word).map(|(value, unit)| Expr::Num(value, Unit::named(unit)))
            }
            // More than one lexeme, so the only reading left is the space-separated one.
            _ => space_run(&divided, env),
        }
    })
}

/// One **space- and/or slash-separated argument**, cut at lexeme boundaries only: the components are token
/// runs, never pieces of a string, which is the difference between reading `18 52 86` as three numbers and
/// `split(' ')`-ing text that happens to contain spaces. Every component must read as an operand or the whole
/// run is unreadable — and that is what keeps the corpus's `missing/*` rows honest: `rgb(none 52 66)` prints
/// as written (`none` is not a value this compiler models yet), it does not become a colour with a hole in it.
///
/// A `/` counts as the **list separator** only when it is a component of its own (`0 255 127 / 0.3`, spaces
/// on both sides). A `/` glued inside a component (`127/0.3`) belongs to the arithmetic that
/// [`release_slashes`] exists to expose, and `+`/`-`/`*` anywhere means this is not a list either — guessing
/// otherwise would turn `rgb(0 255 127 / 0.3)` into a division of `86` by `0.3`.
pub(crate) fn space_run(toks: &[Tok], env: &Env) -> Option<Expr> {
    let parts = crate::calls::cut_at(toks, |tok| matches!(tok, Tok::Space(_)))
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<Vec<Tok>>>();
    let operator = |tok: &Tok| {
        matches!(
            tok,
            Tok::Op('/') | Tok::Op('*') | Tok::Op('+') | Tok::Op('-')
        )
    };
    // The separator is a component **of its own** — `/` with layout either side. Anything else carrying an
    // operator (`127/0.3`, `2 * 3`) is arithmetic, not a list, so the whole run is refused here and stays
    // text.
    let separator = |part: &[Tok]| part.len() == 1 && matches!(part[0], Tok::Op('/'));
    let edge = |part: &Vec<Tok>| separator(part);
    if parts.first().is_some_and(edge) || parts.last().is_some_and(edge) {
        return None;
    }
    if parts
        .iter()
        .any(|part| !separator(part) && part.iter().any(operator))
    {
        return None;
    }
    // Collect the run into groups. Written as a fold over the parts, not a mutable index walk: the depth of
    // the structure is data carried by the accumulator.
    let groups = parts
        .iter()
        .fold(Vec::<Vec<&Vec<Tok>>>::new(), |mut groups, part| {
            if separator(part) {
                groups.push(Vec::new());
            } else if let Some(last) = groups.last_mut() {
                last.push(part);
            } else {
                groups.push(vec![part]);
            }
            groups
        });
    if groups.len() > 2 {
        return None;
    }
    let read = groups
        .iter()
        .map(|group| {
            let values = group
                .iter()
                .map(|part| list_operand(part, env))
                .collect::<Option<Vec<Expr>>>()?;
            match values.len() {
                1 => values.into_iter().next(),
                0 => None,
                _ => Some(Expr::Space(values)),
            }
        })
        .collect::<Option<Vec<Expr>>>()?;
    match read.len() {
        2 => Some(Expr::Slash(read)),
        // A single group only means something when it holds more than one component; a lone lexeme is the
        // caller's business, not this reading's.
        1 if matches!(read.as_slice(), [Expr::Space(..)]) => read.into_iter().next(),
        _ => None,
    }
}

/// One component of a **space run**. Arithmetic reads its operands with [`operand_expr`], where a bare word
/// is not a number and the run declines; a list is a different law — CSS Color 4's `color(srgb 0.2 0.4 0.8)`
/// and a font stack carry bare words as components, and Sass reads one as an unquoted string. Keeping this
/// reading apart is what protects the arithmetic one: `operators/plus::syntax/whitespace/both` writes `c + d`
/// and expects the concatenation `cd`, which only the existing operand path produces, so widening
/// [`operand_expr`] itself regressed that pair.
pub(crate) fn list_operand(toks: &[Tok], env: &Env) -> Option<Expr> {
    operand_expr(toks, env).or_else(|| {
        let meaningful = toks
            .iter()
            .filter(|tok| !matches!(tok, Tok::Space(_)))
            .collect::<Vec<&Tok>>();
        // A word carrying a paren is a call this slice has not read (`url(…`), not a string.
        match meaningful.as_slice() {
            [Tok::Word(word)] if !word.is_empty() && !word.contains('(') => {
                Some(Expr::Text(word.clone()))
            }
            _ => None,
        }
    })
}

/// A call argument's lexemes with every `/` released as its own operator: `Word("1/1px")` becomes
/// `1`, `/`, `1px`, so `math.unit(1/1px)` can reach the division that gives `"px^-1"`
/// (`core_functions/math/unit::one_denominator`).
///
/// This is the context decision 1.4d asks for — `/` means division **here** and in `calc( … )`, while a
/// declaration value keeps it as the list separator (`a {b: 1/2}` → `1/2`, `font: 12px/1.5`) and a URL
/// keeps it inside the literal. Nothing outside an argument is re-cut, and a quoted lexeme is never
/// split, because `"a/b"` is text. `Expr::Call`'s verbatim branch still renders the **original**
/// `arg.toks`, so a run this cannot read comes out byte-for-byte as before.
pub(crate) fn release_slashes(toks: &[Tok]) -> Vec<Tok> {
    let quotable = |word: &str| word.starts_with('"') || word.starts_with('\'');
    toks.iter()
        .flat_map(|tok| match tok {
            Tok::Word(word) if word.contains('/') && !quotable(word) => word
                .split('/')
                .enumerate()
                .flat_map(|(index, part)| {
                    (index > 0)
                        .then_some(Tok::Op('/'))
                        .into_iter()
                        .chain((!part.is_empty()).then(|| Tok::Word(part.to_string())))
                })
                .collect(),
            other => vec![other.clone()],
        })
        .collect()
}

/// The interpolation shape only — what a **selector** may use, because a selector's lexemes
/// must never be read as numbers (`.2xl` is an identifier, not `0.2xl`).
pub fn interpolated(toks: &[Tok], env: &Env) -> Option<Expr> {
    has_dynamic(toks)
        .then(|| interpolation(toks, env))
        .flatten()
}

/// An interpolated run folded into the parts to concatenate: literal text, `$var` reads and
/// nested `#{ … }` bodies. Built by one `fold` so no imperative loop drives the shape.
///
/// A nested `#{ … }` body is read through the same path as a declaration value, `env` included, so
/// `#{calc(1px)}` collapses exactly as a declaration's `calc(1px)` does.
fn interpolation(toks: &[Tok], env: &Env) -> Option<Expr> {
    let parts = toks
        .iter()
        .fold((Vec::new(), String::new()), |(mut parts, mut lit), tok| {
            let flush = |parts: &mut Vec<Expr>, lit: &mut String| {
                if !lit.is_empty() {
                    parts.push(Expr::Text(std::mem::take(lit)));
                }
            };
            match tok {
                Tok::Var(name) => {
                    flush(&mut parts, &mut lit);
                    parts.push(Expr::Var(name.clone()));
                }
                Tok::MemberVar(module, name) => {
                    flush(&mut parts, &mut lit);
                    parts.push(Expr::MemberVar(module.clone(), name.clone()));
                }
                Tok::Interp(inner) => {
                    flush(&mut parts, &mut lit);
                    parts.push(
                        from_tokens(inner, env).map_or(Expr::Text(canon(&render(inner))), |body| {
                            Expr::Concat(vec![body])
                        }),
                    );
                }
                other => lit.push_str(&token_text(other)),
            }
            (parts, lit)
        });
    let (mut parts, lit) = parts;
    if !lit.is_empty() {
        parts.push(Expr::Text(lit));
    }
    if parts.is_empty() {
        return None;
    }
    Some(Expr::Concat(parts))
}

impl BinOp {
    /// The operator's own character, for printing a calculation back out.
    pub(crate) fn symbol(self) -> char {
        match self {
            BinOp::Add => '+',
            BinOp::Sub => '-',
            BinOp::Mul => '*',
            BinOp::Div => '/',
        }
    }

    /// Apply one operator to two evaluated operands, leaving the unit law to `Value`.
    fn apply(self, lhs: &Value, rhs: &Value) -> Result<Value, SassError> {
        match self {
            BinOp::Add => lhs.plus(rhs),
            BinOp::Sub => lhs.minus(rhs),
            BinOp::Mul => lhs.times(rhs),
            BinOp::Div => lhs.divided_by(rhs),
        }
    }
}

impl Expr {
    /// Evaluate bottom-up against the owned environment. Errors are Sass's own (undefined
    /// variable, incompatible units); the caller decides how far they travel down the stream.
    pub fn eval(&self, env: &Env) -> Result<Value, SassError> {
        match self {
            Expr::Num(value, unit) => Ok(Value::Number(*value, unit.clone())),
            Expr::Text(text) => Ok(Value::Raw(text.clone())),
            Expr::Var(name) => env.get(name),
            // A built-in module constant resolves without consulting the environment; anything else named
            // through a dot belongs to a file-loaded module this compiler has not got (`upstream.$a`,
            // `utils.$empty-map` — P5), and guessing there would invent an `Undefined variable` the corpus
            // does not state, so the run keeps its text instead.
            Expr::MemberVar(module, name) => crate::builtins::module_value(module, name)
                .ok_or_else(|| SassError::new("no built-in module member under that name")),
            Expr::Call { name, args } => args
                .iter()
                .map(|arg| {
                    // An argument that is not (yet) an expression is still a **value** in Sass — an
                    // unquoted identifier, a color, a list — so it is carried as text and left to the
                    // builtin's own parameter check, which is what produces the corpus's
                    // `$number: c is not a number.` instead of skipping the call in silence.
                    let value = match argument_expr(&arg.toks, env) {
                        Some(expr) => expr.eval(env)?,
                        // An argument that cannot be turned into an expression **yet** — one carrying `/`
                        // or `*`, i.e. a compound unit like `1/1px` or `1px * 1em`, which Sass does treat as
                        // a number — is not evidence of a type error. The whole call declines and keeps its
                        // verbatim text; claiming `1/1px is not a number.` would be an invented failure
                        // (measured: the first version produced exactly that on 8 compound fixtures).
                        None if protected(&arg.toks) => {
                            return Err(SassError::new("argument needs algebra P1 does not have"));
                        }
                        None => Value::Raw(canon(&render(&arg.toks))),
                    };
                    Ok((arg.label.clone(), value))
                })
                .collect::<Result<Vec<(Option<String>, Value)>, SassError>>()
                .and_then(|given| crate::builtins::evaluate(name, given)),
            Expr::Binary(lhs, op, rhs) => op.apply(&lhs.eval(env)?, &rhs.eval(env)?),
            Expr::Concat(parts) => parts
                .iter()
                .map(|part| part.eval(env).map(|value| value.to_css()))
                .collect::<Result<Vec<String>, SassError>>()
                .map(|text| Value::Raw(canon(&text.concat()))),
            // Each component keeps its own printed form and the separators stay: that is how a space list
            // reads (`rgb(18 52 86)` unconverted prints as written, `values/lists/*` keeps the spaces), and
            // it is what lets a colour constructor take the same value structurally.
            Expr::Space(parts) => parts
                .iter()
                .map(|part| part.eval(env))
                .collect::<Result<Vec<Value>, SassError>>()
                .map(Value::Space),
            Expr::Slash(parts) => parts
                .iter()
                .map(|part| part.eval(env))
                .collect::<Result<Vec<Value>, SassError>>()
                .map(Value::Slash),
            // Reading a calculation already resolved it against the environment — that is why the variant
            // carries a value instead of a token tree — so evaluating it is the identity.
            Expr::Calculation(value) => Ok(value.clone()),
        }
    }
}
