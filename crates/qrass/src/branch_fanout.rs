//! L2 branch fan-out (design D1, manual §2): the `@use`/`@forward` statements of
//! one module body are **independent branches** — disjoint subtrees that share no
//! state — so the part of their instantiation that carries no arena state
//! (canonicalize → read → lex → parse) is orchestrated as a `Local` `flat_map`
//! fan-out stage instead of being interleaved with the recursion:
//!
//! ```text
//! Local::from_iter(branches)
//!     .flat_map(|branch| Local::of((branch.idx, acquire(branch))))  // inline merge
//!     .subscribe(|(idx, outcome)| tx.send((idx, outcome)))          // mpsc terminal
//! ```
//!
//! Two fan-outs run back to back because the filter between them needs `&self`:
//! stage A only canonicalizes (stat probes), so branches already in the
//! [`ModuleBus`](crate::module_bus::ModuleBus) singleton set — or on the cycle
//! stack — are dropped before any file is read. Those are exactly the loads
//! `load_module` aborts at its own cycle and bus checks, so the fan-out spends
//! what the sequential walk would have spent and no more.
//!
//! What deliberately stays sequential, and why: a branch's *instantiation*
//! mutates the shared arena (`std::mem::take(&mut self.env)`, `self.stack.push`,
//! `self.module_envs.insert`, …), so it cannot leave the frame. Manual §14 red
//! line 6 also bans the cross-thread operators `observe_on`/`subscribe_on`, so
//! this fan-out executes **inline** on the current thread by design — and the
//! crate-wide wasm contract (single-threaded `wasm32-unknown-unknown`) rules out
//! worker threads outright. Measured on the `directives` subset (release,
//! `FmtSpan::CLOSE` self busy) the whole fan-out body is 6.95 ms against 32.96 ms
//! of body evaluation and 60.80 ms of load orchestration, i.e. the thread-parallel
//! ceiling was ~5% — the capability an earlier `Shared` spelling preserved cost
//! more in `Send`-bound tax than it could ever pay back. This stage is an
//! architecture commitment, not a performance feature.
//!
//! Ordering: with inline inner sources rxrust merges in outer order (the shape
//! pin in `tests/l2_fanout_shape.rs` observes it, and rxrust's own
//! `flat_map_merges_inner_observables` test pins `vec![1, 11, 2, 12, 3, 13]`), but
//! a *merge-shaped* stage must not let its consumer depend on that, so every result
//! carries its index in the batch and the vector is sorted before use. Errors stay
//! **data** (`Result` in the value channel, `Infallible` in the error channel —
//! manual §14 red line 5): a canonicalize or parse failure must surface at its own
//! `@use` statement in evaluation order, never at fan-out time.
//!
//! Why this stage owns its terminal instead of calling [`crate::drain::drain`]:
//! `drain` short-circuits on the first `Err`, while the fan-out must keep **every**
//! branch's `Result` — parked failures are re-emitted later at their own statement.
//! (A second, historical reason: the chain used to be `Shared`, and `.into_local()`
//! **cannot follow** `flat_map` — it retypes the outer chain to the Local scope
//! while inner observables stay `Shared`, which the merge machinery rejects
//! (`MutRc<MergeAllState<..>>` asked to be an `Observer<Shared…, _>`). Building the
//! chain `Local` from the start removes the mismatch entirely.)

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use rxrust::prelude::*;
use tracing::{Level, span};

use super::EvalState;
use super::eval_module::parent_dir;
use crate::ast::Stmt;
use crate::clock::StopWatch;
use crate::error::SassError;
use crate::loader::{Resolved, canonicalize_with};

/// Cache key for an acquired branch: the directory of the body that wrote the
/// statement plus the URL as written. Two parents in different directories can
/// spell the same URL and mean different files, so the base is part of the key.
pub(crate) type BranchKey = (String, String);

/// One branch's acquired source: canonical path, raw text, and the front-end
/// output — the whole arena-free prefix of instantiating that branch.
pub(crate) struct Prepared {
    pub resolved: Resolved,
    /// Canonical path, keyed exactly the way the bus keys its singletons, so a
    /// cache hit can never disagree with the sequential walk about module
    /// identity.
    pub key: String,
    pub content: String,
    /// Front-end output kept as data: the `.css` passthrough path *ignores* an
    /// `Err` (exotic CSS the SCSS parser cannot read must still pass through),
    /// while a SCSS body propagates it at its own statement.
    pub program: Result<Vec<Stmt>, SassError>,
}

/// One unit of fan-out work: index in the batch, URL as written, base dir of the
/// body that wrote it, and optionally stage A's resolution (so stage B never
/// re-probes the file system for a path it already knows).
#[derive(Clone)]
struct Unit {
    idx: usize,
    url: String,
    base: PathBuf,
    resolved: Option<Resolved>,
}

/// A fan-out result: the batch index plus the unit's own `Result` (error as data).
type Branch<T> = (usize, Result<T, SassError>);

/// The fan-out itself: one `Local` branch per unit, merged, drained through the
/// standard terminal, then reordered by batch index.
fn fan_out<T: 'static>(
    id: &'static str,
    units: Vec<Unit>,
    load_paths: Vec<PathBuf>,
    work: fn(&Unit, &[PathBuf]) -> Result<T, SassError>,
) -> Vec<Branch<T>> {
    let span = span!(
        Level::DEBUG,
        "branch_fanout",
        stage = "fan_out",
        id,
        branches = units.len(),
        failures = tracing::field::Empty,
        elapsed_ms = tracing::field::Empty,
    );
    let _guard = span.enter();
    let started = StopWatch::start();

    // The stage's own terminal: a plain (non-`Send`) closure subscribe is legal on
    // a `Local` chain, and it works here because `Of<T>` hard-codes its error
    // channel to `Infallible`, so every compiler error rides the value channel
    // inside the tuple. Non-timed sources finish during `subscribe`, so the sender
    // drops before `rx.iter()` and no runtime is needed.
    let (tx, rx) = mpsc::channel::<Branch<T>>();
    Local::from_iter(units)
        .flat_map(move |unit: Unit| Local::of((unit.idx, work(&unit, &load_paths))))
        .subscribe(move |item| {
            tx.send(item).ok();
        });
    let mut branches: Vec<Branch<T>> = rx.iter().collect();
    branches.sort_by_key(|(idx, _)| *idx);
    span.record(
        "failures",
        branches.iter().filter(|(_, r)| r.is_err()).count(),
    );
    span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
    branches
}

/// Stage A unit: canonicalize only — no file content is touched.
fn resolve_unit(unit: &Unit, load_paths: &[PathBuf]) -> Result<Resolved, SassError> {
    canonicalize_with(&unit.url, &unit.base, load_paths)
}

/// Stage B unit: read the file and run the front-end over it. This is the **only**
/// acquisition implementation: `load_module` calls it too when the fan-out holds
/// nothing, so a cache miss can never produce a different source than a hit.
fn acquire_unit(unit: &Unit, load_paths: &[PathBuf]) -> Result<Prepared, SassError> {
    let resolved = match &unit.resolved {
        Some(resolved) => Cow::Borrowed(resolved),
        None => Cow::Owned(canonicalize_with(&unit.url, &unit.base, load_paths)?),
    };
    let key = resolved.path.display().to_string();
    let content = read_to_string(&resolved.path, &key)?;
    // File id 0 and the canonical key as source name: the exact arguments
    // `instantiate` used to parse with, so every error locates identically.
    let program = crate::lexer::lex(0, key.clone(), content.clone())
        .and_then(|tokens| crate::parser::parse(0, key.clone(), content.clone(), tokens));
    Ok(Prepared {
        resolved: resolved.as_ref().clone(),
        key,
        content,
        program,
    })
}

/// Acquire one branch inline (no fan-out cache entry for it).
pub(crate) fn acquire(
    url: &str,
    base: &Path,
    load_paths: &[PathBuf],
) -> Result<Prepared, SassError> {
    acquire_unit(
        &Unit {
            idx: 0,
            url: url.to_string(),
            base: base.to_path_buf(),
            resolved: None,
        },
        load_paths,
    )
}

/// `(base dir of the writing body, url as written)` — the key `load_module`
/// looks a fan-out result up under.
pub(crate) fn branch_key(base: &Path, url: &str) -> BranchKey {
    (base.display().to_string(), url.to_string())
}

/// Read an acquired branch from disk, mapping IO failure to a load error. The
/// error locates against the canonical path, exactly as the sequential walk did.
fn read_to_string(path: &Path, name: &str) -> Result<String, SassError> {
    std::fs::read_to_string(path).map_err(|err| SassError::io(err, name))
}

impl EvalState {
    /// Fan out every top-level `@use`/`@forward` branch of `program` and cache the
    /// acquired sources for `load_module` to consume.
    ///
    /// A memoization boundary only: `load_module` still resolves, reads and parses
    /// each module it actually loads exactly once, still in statement order, and
    /// still raises every error from the same call site with the same arguments.
    pub(super) fn prefetch_branches(&mut self, program: &[Stmt]) {
        let span = span!(
            Level::DEBUG,
            "branch_prefetch",
            stage = "fan_out",
            module = %self.name,
            branches = tracing::field::Empty,
            skipped = tracing::field::Empty,
            cached = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        let started = StopWatch::start();
        let base = parent_dir(&self.name);
        let mut units: Vec<Unit> = Vec::new();
        for stmt in program {
            let url = match stmt {
                Stmt::Use(u) => &u.url,
                Stmt::Forward(f) => &f.url,
                _ => continue,
            };
            // Built-in modules (`sass:math`, …) have no source file to acquire.
            if url.starts_with("sass:") {
                continue;
            }
            units.push(Unit {
                idx: units.len(),
                url: url.to_string(),
                base: base.clone(),
                resolved: None,
            });
        }
        span.record("branches", units.len());
        if units.is_empty() {
            span.record("skipped", 0usize);
            span.record("cached", self.prepared.len());
            span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
            return;
        }
        let all = units.clone();
        let paths = self.load_paths.clone();

        // Stage A — canonicalize. The filter below is the only part that needs the
        // arena, so it sits between the two `Local` stages rather than inside a
        // stream closure. Resolutions are parked locally, not in `prepared`: the
        // cache holds whole acquisitions only.
        let mut ready: Vec<Unit> = Vec::new();
        let mut skipped = 0usize;
        let mut failed: Vec<(usize, SassError)> = Vec::new();
        for (idx, outcome) in fan_out::<Resolved>("resolve", units, paths.clone(), resolve_unit) {
            match outcome {
                Ok(resolved) => {
                    let canonical = resolved.path.display().to_string();
                    // Already a known singleton (a diamond's second parent joins
                    // the multicast instead of loading) or on the cycle stack:
                    // `load_module` returns before reading anything, so do not pay
                    // for a read it would discard.
                    if self.bus.is_open(&canonical) || self.stack.contains(&canonical) {
                        skipped += 1;
                        continue;
                    }
                    ready.push(Unit {
                        resolved: Some(resolved),
                        ..all[idx].clone()
                    });
                }
                // Keep the failure as data: `load_module` re-emits it at its own
                // statement position instead of probing the file system again.
                Err(err) => failed.push((idx, err)),
            }
        }
        span.record("skipped", skipped);

        // Stage B — read + lex + parse the surviving branches.
        let mut acquired: Vec<(usize, Result<Prepared, SassError>)> = Vec::new();
        for (idx, outcome) in fan_out::<Prepared>("acquire", ready, paths.clone(), acquire_unit) {
            acquired.push((idx, outcome));
        }
        for (idx, err) in failed {
            acquired.push((idx, Err(err)));
        }
        // Commit in one pass: every `Result` is already in hand, so the map is
        // written once, after both `Local` stages have finished.
        for (idx, outcome) in acquired {
            let unit = &all[idx];
            self.prepared
                .insert(branch_key(&unit.base, &unit.url), outcome);
        }
        span.record("cached", self.prepared.len());
        span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
    }
}
