//! Control-flow expansion for [`EvalState`] (task 7.3): `@if`/`@else`, `@each`
//! (list & map destructuring), `@for` (to/through) and `@while`. Each construct
//! binds its variables in a fresh scope and *eagerly* re-runs the branch/loop body
//! through [`EvalState::collect_stmt`], so loop indices are captured per iteration
//! and any declarations fold into the enclosing rule's buffer.

use crate::ast::{Each, For, If, While};
use crate::env::AssignFlags;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::{Ev, EvalState};

/// Safety cap on `@while` iterations to turn an infinite loop into an error.
const WHILE_LIMIT: u32 = 100_000;

impl EvalState {
    /// `@if`/`@else if`/`@else`: run the first branch whose condition is truthy
    /// (a `None` condition is the trailing `@else`).
    pub(super) fn run_if(
        &mut self,
        i: &If,
        sel_ctx: &[String],
        decls: &mut Vec<crate::css::Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        for clause in &i.clauses {
            let take = match &clause.cond {
                None => true,
                Some(cond) => self.eval_expr(cond)?.is_truthy(),
            };
            if take {
                self.env.enter();
                for s in &clause.body {
                    self.collect_stmt(s, sel_ctx, decls, out)?;
                }
                self.env.leave();
                return Ok(());
            }
        }
        Ok(())
    }

    /// `@each $a, $b in list`: iterate a list (or map as key/value pairs),
    /// destructuring each element across the declared variables.
    pub(super) fn run_each(
        &mut self,
        e: &Each,
        sel_ctx: &[String],
        decls: &mut Vec<crate::css::Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        let value = self.eval_expr(&e.list)?;
        for binding in each_elements(&value, e.vars.len()) {
            self.env.enter();
            for (var, val) in e.vars.iter().zip(binding) {
                self.env.define(var, val, AssignFlags::default());
            }
            for s in &e.body {
                self.collect_stmt(s, sel_ctx, decls, out)?;
            }
            self.env.leave();
        }
        Ok(())
    }

    /// `@for $i from <a> to|through <b>`: step by ±1 over the integer range.
    pub(super) fn run_for(
        &mut self,
        f: &For,
        sel_ctx: &[String],
        decls: &mut Vec<crate::css::Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        let from = int_bound(self.eval_expr(&f.from)?)?;
        let to = int_bound(self.eval_expr(&f.to)?)?;
        let last = if f.inclusive {
            to
        } else {
            to + if from <= to { -1 } else { 1 }
        };
        let step = if from <= last { 1 } else { -1 };
        let mut i = from;
        while (step > 0 && i <= last) || (step < 0 && i >= last) {
            self.env.enter();
            self.env.define(
                &f.var,
                SassValue::Number {
                    value: i as f64,
                    unit: None,
                },
                AssignFlags::default(),
            );
            for s in &f.body {
                self.collect_stmt(s, sel_ctx, decls, out)?;
            }
            self.env.leave();
            i += step;
        }
        Ok(())
    }

    /// `@while cond { … }`: repeat while the condition stays truthy. The whole
    /// loop shares one scope so a counter updated in the body (`$i: $i - 1`)
    /// persists across iterations and the re-tested condition observes it.
    pub(super) fn run_while(
        &mut self,
        w: &While,
        sel_ctx: &[String],
        decls: &mut Vec<crate::css::Item>,
        out: &mut Vec<String>,
    ) -> Ev<()> {
        let mut guard = 0u32;
        self.env.enter();
        while self.eval_expr(&w.cond)?.is_truthy() {
            guard += 1;
            if guard > WHILE_LIMIT {
                self.env.leave();
                return Err(SassError::new(
                    ErrorKind::Eval,
                    "@while loop exceeded the maximum number of iterations",
                ));
            }
            for s in &w.body {
                self.collect_stmt(s, sel_ctx, decls, out)?;
            }
        }
        self.env.leave();
        Ok(())
    }
}

/// Integer bounds for `@for` (numbers must be integers, per the sass spec).
pub(crate) fn int_bound(v: SassValue) -> Result<i64, SassError> {
    let SassValue::Number { value, .. } = v else {
        return Err(SassError::new(
            ErrorKind::Eval,
            "@for bounds must be numbers",
        ));
    };
    if value.fract() != 0.0 {
        return Err(SassError::new(
            ErrorKind::Eval,
            "@for bounds must be integers",
        ));
    }
    Ok(value as i64)
}

/// Produce one binding tuple per iteration of `@each`.
pub(crate) fn each_elements(value: &SassValue, var_count: usize) -> Vec<Vec<SassValue>> {
    match value {
        SassValue::Map { entries } => entries
            .iter()
            .map(|(k, v)| vec![k.clone(), v.clone()])
            .collect(),
        SassValue::List { items, .. } => items
            .iter()
            .map(|item| destructure(item, var_count))
            .collect(),
        scalar => vec![vec![scalar.clone()]],
    }
}

/// Flatten an `@each` element into `var_count` values: a list splits its items
/// (padded with `null`), a scalar binds the first variable alone.
fn destructure(item: &SassValue, var_count: usize) -> Vec<SassValue> {
    if var_count <= 1 {
        return vec![item.clone()];
    }
    if let SassValue::List { items, .. } = item {
        let mut row = items.clone();
        row.resize(var_count, SassValue::Null);
        return row;
    }
    let mut row = vec![item.clone()];
    row.resize(var_count, SassValue::Null);
    row
}
