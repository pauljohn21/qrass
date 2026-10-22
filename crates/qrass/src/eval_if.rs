//! Three-valued symbolic execution for the modern `if(…)` clause calls
//! (behavior reverse-engineered from the `expressions/if/*.hrx` goldens).
//!
//! A condition simplifies to `True` / `False` (all terms evaluable) or `Css`
//! (some term stays undetermined — raw CSS like `css(…)`/`var(…)`, already
//! re-rendered with normalized whitespace). The first definitely-true clause
//! short-circuits to its evaluated value; the first undetermined clause freezes
//! CSS output: this and every later clause render verbatim into an
//! `if(…; …)` CSS function call. `and`/`or` chains drop decided terms
//! (with short-circuit) and a `Paren` alone left by an `and`-chain drop
//! unwraps, matching the sass-spec goldens.

use crate::ast_expr::{IfCall, IfClause, IfCond, Join};
use crate::value::SassValue;

use super::{Ev, EvalState};

/// Outcome of simplifying one condition node.
enum Tri {
    True,
    False,
    /// Rendered CSS text of the still-symbolic condition.
    Css(String),
}

impl EvalState {
    /// Evaluate a modern `if()` to a value: a Sass result, or an unquoted
    /// string holding the rebuilt CSS `if(…)` call.
    pub(crate) fn eval_if(&mut self, call: &IfCall) -> Ev<SassValue> {
        // clause scan as a fold: `Done` latches the short-circuit value, the
        // frozen flag rides the accumulator (the imperative early-return shape)
        enum Walk {
            Done(SassValue),
            Going(Vec<String>, bool),
        }
        let walk = call
            .clauses
            .iter()
            .try_fold(Walk::Going(Vec::new(), false), |w, clause| -> Ev<Walk> {
                Ok(match w {
                    Walk::Done(v) => Walk::Done(v),
                    Walk::Going(out, true) => {
                        let mut out = out;
                        out.push(self.render_clause(clause)?);
                        Walk::Going(out, true)
                    }
                    Walk::Going(out, false) => {
                        let tri = clause
                            .cond
                            .as_ref()
                            .map_or(Ok(Tri::True), |c| self.simp_cond(c))?;
                        match tri {
                            Tri::True => Walk::Done(self.eval_expr(&clause.value)?),
                            Tri::False => Walk::Going(out, false),
                            Tri::Css(text) => {
                                let mut out = out;
                                let value = self.arm_value(clause)?;
                                out.push(format!("{text}: {value}"));
                                Walk::Going(out, true)
                            }
                        }
                    }
                })
            })?;
        match walk {
            Walk::Done(v) => Ok(v),
            Walk::Going(out, true) => Ok(SassValue::Str {
                text: format!("if({})", out.join("; ")),
                quoted: false,
            }),
            // No arm matched and no else: the call evaluates to null.
            Walk::Going(_, false) => Ok(SassValue::Null),
        }
    }

    // ─── Condition simplification ───────────────────────────────────────────

    fn simp_cond(&mut self, cond: &IfCond) -> Ev<Tri> {
        Ok(match cond {
            IfCond::Sass(e) => {
                if self.eval_expr(e)?.is_truthy() {
                    Tri::True
                } else {
                    Tri::False
                }
            }
            IfCond::Css(pieces) => Tri::Css(self.render_interp(pieces)?),
            IfCond::Not(inner) => match self.simp_cond(inner)? {
                Tri::True => Tri::False,
                Tri::False => Tri::True,
                Tri::Css(t) => Tri::Css(format!("not {t}")),
            },
            IfCond::Paren(inner) => match self.simp_cond(inner)? {
                Tri::Css(t) => Tri::Css(format!("({t})")),
                other => other,
            },
            IfCond::Chain { parts, joins } => self.simp_chain(parts, joins)?,
        })
    }

    /// Simplify one chain level. `all_or` chains fall through to the first
    /// true term; `and`/adjacent chains die on the first false term. Terms
    /// with a decided truth value drop out of the rendered remainder.
    #[allow(clippy::type_complexity)]
    fn simp_chain(&mut self, parts: &[IfCond], joins: &[Join]) -> Ev<Tri> {
        let all_or = joins.iter().all(|j| matches!(j, Join::Or));
        // Survivors: (original node, rendered text, join toward the next part),
        // folded with a latched `done` for the chain-deciding term.
        let (kept, done) = parts.iter().enumerate().try_fold(
            (Vec::new(), None::<Tri>),
            |mut acc, (i, part)| -> Ev<(Vec<(&IfCond, String, Option<Join>)>, Option<Tri>)> {
                if acc.1.is_none() {
                    let tri = self.simp_cond(part)?;
                    let join = joins.get(i).copied();
                    match tri {
                        // or-chain: a true term decides the whole chain.
                        Tri::True if all_or => acc.1 = Some(Tri::True),
                        // and-chain: a false term kills the whole chain.
                        Tri::False if !all_or => acc.1 = Some(Tri::False),
                        // Decided terms drop out; their join disappears with them.
                        Tri::True | Tri::False => {}
                        Tri::Css(t) => acc.0.push((part, t, join)),
                    }
                }
                Ok(acc)
            },
        )?;
        if let Some(t) = done {
            return Ok(t);
        }
        if kept.is_empty() {
            // All parts dropped: an or-chain went false, an and-chain true.
            return Ok(match all_or {
                true => Tri::False,
                false => Tri::True,
            });
        }
        // An and-chain reduced to one surviving Paren unwraps (golden
        // `sass(true) and (…)` → the parenthesized group renders bare).
        let unwrapped = match (kept.len() == 1 && parts.len() > 1, kept[0].0) {
            (true, IfCond::Paren(inner)) => Some(self.simp_cond(inner)?),
            _ => None,
        }
        .filter(|t| matches!(t, Tri::Css(_)));
        if let Some(t) = unwrapped {
            return Ok(t);
        }
        let text = kept
            .iter()
            .skip(1)
            .enumerate()
            .fold(kept[0].1.clone(), |mut text, (w, (_, t, _))| {
                let sep = match kept[w].2 {
                    Some(Join::Or) => " or ",
                    Some(Join::Adjacent) => " ",
                    Some(Join::And) => " and ",
                    // Trailing join slots are `None` mid-chain only after drops;
                    // fall back to the chain's dominant connective.
                    None if all_or => " or ",
                    None => " and ",
                };
                text.push_str(sep);
                text.push_str(t);
                text
            });
        Ok(Tri::Css(text))
    }

    // ─── Frozen-mode rendering ──────────────────────────────────────────────

    /// One surviving clause as `cond: value` / `else: value`.
    fn render_clause(&mut self, clause: &IfClause) -> Ev<String> {
        let value = self.arm_value(clause)?;
        Ok(match &clause.cond {
            None => format!("else: {value}"),
            Some(c) => format!("{}: {value}", self.simp_cond_render(c)?),
        })
    }

    /// Render a later clause's condition without short-circuiting: decided
    /// sass terms still simplify away (their truth is known), remaining sass
    /// terms evaluate to their CSS value.
    fn simp_cond_render(&mut self, cond: &IfCond) -> Ev<String> {
        Ok(match self.simp_cond(cond)? {
            Tri::Css(t) => t,
            Tri::True => "true".to_string(),
            Tri::False => "false".to_string(),
        })
    }

    /// A clause arm's value, evaluated and serialized as CSS text.
    fn arm_value(&mut self, clause: &IfClause) -> Ev<String> {
        let v = self.eval_expr(&clause.value)?;
        Ok(v.to_css())
    }
}
