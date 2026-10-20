//! `meta.load-css($url, $with: …)` for [`EvalState`] — a sibling `impl` split
//! of `eval_module` for the 500-line budget (round 80). Loads the target
//! module, splices its CSS into the output, and hands it the `$with:`
//! configuration; only the CSS joins — no namespace binds (`load_css/error/
//! member.hrx`: `$c` and `other.$c` both stay Undefined in the caller).
//!
//! The load-css error family spells several failures differently from the
//! `@use … with` goldens, and every wording below is read off the golden
//! tables: `$a was not declared with !default…` (check_config_site's
//! load_css branch), `_other.scss was already loaded, so it can't be
//! configured using "with".` (direct target; forwarded distribution keeps
//! `This module was…`), `Built-in module sass:color can't be configured.`,
//! and `The variable $a-b was configured twice.`.

use crate::ast::Include;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::{Ev, EvalState};

impl EvalState {
    pub(super) fn run_load_css(&mut self, inc: &Include, out: &mut Vec<String>) -> Ev<()> {
        let raw = self.collect_args(&inc.args)?;
        let url = match raw.positional().first() {
            Some(SassValue::Str { text, .. }) => text.clone(),
            None => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    "Missing argument $url.",
                ));
            }
            Some(other) => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$url: {} is not a string.", other.to_css()),
                ));
            }
        };
        let entry = inc.span;

        // `$with:` — named first, then the second positional (dart's order).
        let with_v = raw
            .named()
            .get("with")
            .cloned()
            .or_else(|| raw.positional().get(1).cloned());
        let site = self.open_config_site();
        let mut view: Vec<(String, usize)> = Vec::new();
        match &with_v {
            None | Some(SassValue::Null) => {}
            Some(SassValue::Map { entries }) => {
                let mut seen: Vec<String> = Vec::new();
                for (k, v) in entries {
                    let name = match k {
                        SassValue::Str { text, .. } => text.clone(),
                        other => {
                            return Err(SassError::new(
                                ErrorKind::Builtins,
                                format!("$with key: {} is not a string.", other.to_css()),
                            ));
                        }
                    };
                    let key = crate::parser::config_key(&name);
                    if seen.contains(&key) {
                        return Err(SassError::new(
                            ErrorKind::Config,
                            format!("The variable ${name} was configured twice."),
                        ));
                    }
                    seen.push(key.clone());
                    let _ = key;
                    view.push(self.offer_value(name, v.clone(), site, entry));
                }
            }
            Some(other) => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$with: {} is not a map.", other.to_css()),
                ));
            }
        }

        // A built-in module has no `!default` to claim: configuring it fails
        // before anything loads (`load_css/error/with` `core_module`).
        if url.starts_with("sass:") && !view.is_empty() {
            return Err(SassError::new(
                ErrorKind::Config,
                format!("Built-in module {url} can't be configured."),
            ));
        }

        // Already-loaded with a live configuration: the direct target is named
        // by file (`_other.scss was already loaded…`); forwarded distribution
        // keeps the generic spelling, which `load_module` raises for the
        // nested frames.
        if !view.is_empty() && !url.starts_with("sass:") {
            let base = crate::eval::eval_module::parent_dir(&self.name);
            if let Ok(resolved) =
                crate::loader::canonicalize_with(&url, &base, &self.load_paths)
            {
                let key = resolved.path.display().to_string();
                if self.bus.is_open(&key) {
                    let module = self.bus.share(&key)?;
                    if view.iter().any(|(name, idx)| {
                        !self.config_offers[*idx].consumed
                            && module.configurable.iter().any(|c| c == name)
                    }) {
                        let file = key.rsplit('/').next().unwrap_or(&key).to_string();
                        return Err(SassError::new(
                            ErrorKind::Config,
                            format!(
                                "{file} was already loaded, so it can't be configured using \"with\"."
                            ),
                        ));
                    }
                }
            }
        }

        let link = self.link_to(&inc.span, "@use");
        let (_module, _ran) = self
            .load_module(&url, view, out, true, link, entry)
            .map_err(|err| self.attach_frames(err))?;
        // No binding: `@use`s stay private to the loaded module.
        self.check_config_site(site, true)
    }
}
