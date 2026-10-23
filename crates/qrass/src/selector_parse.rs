//! Selector parser (Round 23 Phase 1): turns selector source text into the owned
//! [`SelectorList`] of [`crate::selector_ast`], so the `selector.*` builtins and
//! `@extend` can reason about structure instead of string hacks.
//!
//! This is a small, self-contained string→AST utility (not one of the rxrust
//! pipeline stages): it operates on an already-materialized selector string, so a
//! cursor over `Vec<char>` is the honest shape. Every loop advances `self.pos` or
//! breaks/returns — no zero-consumption path can push a node — so it cannot spin
//! or balloon memory on malformed input.
//!
//! Two error classes mirror what the builtins surface:
//! - [`SelParseError::Parent`]: a `&` parent selector appears where only a real
//!   selector is allowed (`selector.is-superselector("&", ...)` etc.);
//! - [`SelParseError::Truncated`]: the input ends mid-selector or has trailing
//!   junk (including a dangling combinator like `a >`, which is NOT silently
//!   downgraded to `a`).

use crate::selector_ast::{
    Combinator, Complex, Component, Compound, Ns, PseudoArg, SelectorList, Simple,
};

/// Why a selector failed to parse. The builtins turn these into the exact
/// the goldens' wording (`expected more input.`, `Parent selectors aren't allowed
/// here.`); the parser only classifies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelParseError {
    Parent,
    Truncated,
}

/// Pseudo-classes whose argument is a selector list (parsed recursively).
const SELECTOR_PSEUDOS: &[&str] = &[
    "not",
    "is",
    "where",
    "matches",
    "any",
    "current",
    "has",
    "host",
    "host-context",
    "slotted",
];

/// Functional pseudo-classes whose argument is `<an+b> [of <selector>]`.
const NTH_PSEUDOS: &[&str] = &[
    "nth-child",
    "nth-last-child",
    "nth-of-type",
    "nth-last-of-type",
];

/// Single-colon pseudo-elements (legacy spelling) that must be treated as
/// elements for subsumption/ordering, distinguished from pseudo-classes.
const LEGACY_PSEUDO_ELEMENTS: &[&str] = &[
    "before",
    "after",
    "first-line",
    "first-letter",
    "selection",
    "backdrop",
    "marker",
    "placeholder",
    "grammar-error",
    "spelling-error",
];

/// Parse a comma-separated selector list.
pub fn parse_list(src: &str) -> Result<SelectorList, SelParseError> {
    let mut parser = SelParser {
        chars: src.chars().collect(),
        pos: 0,
    };
    parser.parse_list()
}

/// Parse exactly one complex selector (no commas).
pub fn parse_complex_str(src: &str) -> Result<Complex, SelParseError> {
    let mut parser = SelParser {
        chars: src.chars().collect(),
        pos: 0,
    };
    parser.skip_ws();
    let complex = parser.parse_complex()?;
    parser.skip_ws();
    if parser.peek().is_some() {
        return Err(SelParseError::Truncated);
    }
    Ok(complex)
}

struct SelParser {
    chars: Vec<char>,
    pos: usize,
}

impl SelParser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn parse_list(&mut self) -> Result<SelectorList, SelParseError> {
        let mut complexes = Vec::new();
        loop {
            self.skip_ws();
            complexes.push(self.parse_complex()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some(_) => return Err(SelParseError::Truncated),
                None => break,
            }
        }
        Ok(SelectorList::new(complexes))
    }

    fn parse_complex(&mut self) -> Result<Complex, SelParseError> {
        let mut components: Vec<Component> = Vec::new();
        let mut trailing: Vec<Combinator> = Vec::new();
        loop {
            // Collect the full run of combinators at the cursor. A valid selector
            // has at most one; the bogus goldens stack them (`.c ~ ~ .d`, `> + .c`).
            let mut chain: Vec<Combinator> = Vec::new();
            while let Some(combinator) = self.parse_combinator() {
                chain.push(combinator);
            }
            // No compound after the chain: the combinators dangle (`a >`, or a lone
            // `+`). Keep them verbatim rather than erroring or silently downgrading.
            if !is_compound_start(self.peek().unwrap_or(' ')) {
                trailing = chain;
                break;
            }
            let compound = self.parse_compound()?;
            let combinator = if components.is_empty() {
                // A leading combinator (`> c`) is retained on the first component so
                // the superselector engine can reject the bogus chain; a descendant
                // opening is meaningless and stays `None`.
                chain.first().copied()
            } else {
                // No explicit combinator between two compounds is a descendant.
                Some(chain.first().copied().unwrap_or(Combinator::Descendant))
            };
            let extras: Vec<Combinator> = chain.iter().skip(1).copied().collect();
            components.push(Component {
                combinator,
                extras,
                compound,
            });
            // Continue only when another compound or combinator follows; a comma or
            // the end terminates the complex (any stray junk is caught by the list).
            self.skip_ws();
            if matches!(self.peek(), Some('>') | Some('+') | Some('~')) || self.peek().is_some_and(is_compound_start) {
                continue;
            }
            break;
        }
        if components.is_empty() && trailing.is_empty() {
            return Err(SelParseError::Truncated);
        }
        Ok(Complex {
            components,
            trailing,
        })
    }

    /// Skip whitespace and consume an explicit combinator symbol if present.
    /// Returns `None` for a descendant (which is either an unmarked space or the
    /// very first component).
    fn parse_combinator(&mut self) -> Option<Combinator> {
        self.skip_ws();
        let combinator = match self.peek() {
            Some('>') => Some(Combinator::Child),
            Some('+') => Some(Combinator::NextSibling),
            Some('~') => Some(Combinator::Sibling),
            _ => None,
        };
        if combinator.is_some() {
            self.pos += 1;
            self.skip_ws();
        }
        combinator
    }

    fn parse_compound(&mut self) -> Result<Compound, SelParseError> {
        let mut simples = Vec::new();
        loop {
            match self.peek() {
                Some('.') => {
                    self.pos += 1;
                    simples.push(Simple::Class(self.parse_name()?));
                }
                Some('#') => {
                    self.pos += 1;
                    simples.push(Simple::Id(self.parse_name()?));
                }
                Some('%') => {
                    self.pos += 1;
                    simples.push(Simple::Placeholder(self.parse_name()?));
                }
                Some('[') => simples.push(self.parse_attribute()?),
                Some(':') => simples.push(self.parse_pseudo()?),
                Some('&') => return Err(SelParseError::Parent),
                Some(ch) if ch == '*' || ch == '|' || is_ident_start(ch) => {
                    simples.push(self.parse_qualified()?);
                }
                _ => break,
            }
        }
        if simples.is_empty() {
            return Err(SelParseError::Truncated);
        }
        Ok(Compound::new(simples))
    }

    /// A namespace-qualified type/universal selector, or a bare one. `*`, `|`,
    /// and an identifier all funnel through here so `*|*`, `|*`, `ns|*`, `|c`,
    /// `*`, and `c` are all covered by the same path.
    fn parse_qualified(&mut self) -> Result<Simple, SelParseError> {
        let ns = match self.peek() {
            Some('|') => {
                self.pos += 1;
                Ns::None
            }
            Some('*') => {
                self.pos += 1;
                if self.peek() == Some('|') {
                    self.pos += 1;
                    Ns::Wild
                } else {
                    // Bare universal: `*` already consumed.
                    return Ok(Simple::Universal { ns: Ns::Bare });
                }
            }
            Some(ch) if is_ident_start(ch) => {
                let name = self.parse_name()?;
                if self.peek() == Some('|') {
                    self.pos += 1;
                    Ns::Named(name)
                } else {
                    return Ok(Simple::Type { ns: Ns::Bare, name });
                }
            }
            _ => return Err(SelParseError::Truncated),
        };
        // After a namespace bar, expect `*` or an identifier.
        match self.peek() {
            Some('*') => {
                self.pos += 1;
                Ok(Simple::Universal { ns })
            }
            Some(ch) if is_ident_start(ch) => Ok(Simple::Type {
                ns,
                name: self.parse_name()?,
            }),
            _ => Err(SelParseError::Truncated),
        }
    }

    fn parse_attribute(&mut self) -> Result<Simple, SelParseError> {
        // self.peek() == '['
        self.pos += 1;
        let start = self.pos;
        let mut depth = 1i32;
        while let Some(ch) = self.peek() {
            self.pos += 1;
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return Err(SelParseError::Truncated);
        }
        // self.pos is just past the closing ']'; inner excludes both brackets.
        let inner: String = self.chars[start..self.pos - 1].iter().collect();
        Ok(Simple::Attribute(crate::selector_attr::parse_attribute(
            &inner,
        )))
    }

    fn parse_pseudo(&mut self) -> Result<Simple, SelParseError> {
        self.pos += 1; // first ':'
        let double = self.peek() == Some(':');
        if double {
            self.pos += 1;
        }
        let name = self.parse_name()?;
        let arg = if self.peek() == Some('(') {
            self.parse_pseudo_arg(&name)?
        } else {
            PseudoArg::None
        };
        if double {
            Ok(Simple::PseudoElement {
                name,
                arg,
                legacy_colon: false,
            })
        } else if LEGACY_PSEUDO_ELEMENTS.contains(&name.as_str()) {
            Ok(Simple::PseudoElement {
                name,
                arg,
                legacy_colon: true,
            })
        } else {
            Ok(Simple::PseudoClass { name, arg })
        }
    }

    fn parse_pseudo_arg(&mut self, name: &str) -> Result<PseudoArg, SelParseError> {
        let raw = self.take_balanced()?;
        if is_nth_pseudo(name) {
            return match split_nth(&raw) {
                Some((anbn, of_src)) => {
                    let of_src = of_src.trim();
                    let of = if of_src.is_empty() {
                        None
                    } else {
                        Some(parse_list(of_src)?)
                    };
                    Ok(PseudoArg::Nth {
                        anbn: canonical_anbn(&anbn),
                        of,
                    })
                }
                None => Ok(PseudoArg::Nth {
                    anbn: canonical_anbn(&raw),
                    of: None,
                }),
            };
        }
        if is_selector_pseudo(name) {
            return Ok(PseudoArg::Selectors(parse_list(raw.trim())?));
        }
        Ok(PseudoArg::Text(raw))
    }

    /// Consume a balanced `(...)` group and return its inner text (without the
    /// outer parentheses). `[`/`]` are tracked too so a `[...]` argument does not
    /// confuse the depth counter.
    fn take_balanced(&mut self) -> Result<String, SelParseError> {
        self.pos += 1; // '('
        let mut depth = 1i32;
        let mut out = String::new();
        while let Some(ch) = self.peek() {
            self.pos += 1;
            match ch {
                '(' | '[' => {
                    depth += 1;
                    out.push(ch);
                }
                ')' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                    out.push(ch);
                }
                _ => out.push(ch),
            }
        }
        Err(SelParseError::Truncated)
    }

    fn parse_name(&mut self) -> Result<String, SelParseError> {
        let mut out = String::new();
        while let Some(ch) = self.peek() {
            if ch == '\\' {
                self.pos += 1;
                out.push('\\');
                if let Some(esc) = self.peek() {
                    out.push(esc);
                    self.pos += 1;
                }
            } else if is_ident_char(ch) {
                out.push(ch);
                self.pos += 1;
            } else {
                break;
            }
        }
        if out.is_empty() {
            return Err(SelParseError::Truncated);
        }
        Ok(out)
    }
}

/// The `of` keyword in `nth-* of <sel>` only counts when fully surrounded by
/// whitespace, so `:nth-child(offset)` is not split. Returns the `<an+b>` before
/// the first such window and the selector source after it.
fn split_nth(raw: &str) -> Option<(String, String)> {
    let chars: Vec<char> = raw.chars().collect();
    let mut depth = 0i32;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            '(' | '[' => {
                depth += 1;
                index += 1;
            }
            ')' | ']' => {
                depth -= 1;
                index += 1;
            }
            ' ' => {
                if depth == 0 && is_of_window(&chars, index) {
                    let anbn: String = chars[..index].iter().collect();
                    let of: String = chars[index + 4..].iter().collect();
                    return Some((anbn, of));
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

/// The canonical `<an+b>` text: internal whitespace is dropped (`2n + 1` ->
/// `2n+1`), matching the spec's normalized rendering.
fn canonical_anbn(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_whitespace()).collect()
}

/// True when `chars[index..index+4]` is exactly the four-char window `" of "`
/// (whitespace-delimited, case-insensitive on the letters).
fn is_of_window(chars: &[char], index: usize) -> bool {
    index + 4 <= chars.len()
        && chars[index] == ' '
        && chars[index + 1].eq_ignore_ascii_case(&'o')
        && chars[index + 2].eq_ignore_ascii_case(&'f')
        && chars[index + 3] == ' '
}

fn is_compound_start(ch: char) -> bool {
    matches!(ch, '.' | '#' | '%' | '[' | ':' | '&' | '*' | '|') || is_ident_start(ch)
}

/// Whether `name` is an `<an+b> [of <selector>]` pseudo, tolerating a vendor
/// prefix (`-pfx-nth-child` counts as `nth-child`).
fn is_nth_pseudo(name: &str) -> bool {
    NTH_PSEUDOS
        .iter()
        .any(|p| name == *p || name.ends_with(&format!("-{p}")))
}

/// Whether `name` is a functional pseudo whose argument is a selector list,
/// tolerating a vendor prefix (`-pfx-is`, `-moz-where`, …).
fn is_selector_pseudo(name: &str) -> bool {
    SELECTOR_PSEUDOS
        .iter()
        .any(|p| name == *p || name.ends_with(&format!("-{p}")))
}

fn is_ident_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_' || ch == '-' || ch == '\\' || (ch as u32) > 0x7F
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '\\' || (ch as u32) > 0x7F
}
