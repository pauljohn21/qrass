//! The SCSS value model ([`SassValue`]) shared by eval (task group 5) and the
//! builtin library (task group 8). Owned, `Clone` — values move by value through
//! the reactive eval fold; no shared mutable wrappers.
//!
//! Design D2: numbers use `f64` (internal representation is not the contract;
//! only the serialized form is golden-checked). Colors keep an RGBA base with
//! on-demand HSL conversion. Truthiness follows the sass spec: only `null` and
//! `false` are falsy — `0`, `""`, and empty lists are truthy.

use std::fmt;

use crate::ast_expr::ListSep;

// The numeric layer (formatting, CSS serialization, unit conversion) lives in the
// sibling `value_number` module; re-exported here so `crate::value::{format_number,
// number_to_css, convert_number}` — and the local `number_to_css` call below — keep
// resolving unchanged.
pub use crate::value_number::{convert_number, format_number, number_to_css};

// The first-class callable payloads live in the crate-root `value_callable`
// module (declared beside `value_number`); re-exported here so
// `crate::value::{FnRef, FnKind, MixRef, MixKind}` keep resolving.
pub(crate) use crate::value_callable::{MixKind, MixRef, fn_kind_eq, mix_kind_eq};
pub use crate::value_callable::{FnKind, FnRef};

/// A runtime SCSS value.
#[derive(Clone, Debug)]
pub enum SassValue {
    Null,
    Bool(bool),
    /// A dimensioned number; `unit` is `None` for unitless. `%` is a unit.
    Number {
        value: f64,
        unit: Option<String>,
    },
    /// A string; `quoted` preserves whether it came from `"…"`/`'…'` so output
    /// can re-quote it. Unquoted strings are bare identifiers / interpolated text.
    Str {
        text: String,
        quoted: bool,
    },
    Color(Color),
    /// A list with a separator; a map is represented by [`SassValue::Map`].
    /// `bracketed` marks a CSS-`[]` bracketed list literal (`[a b c]`), which
    /// round-trips through serialization and drives `list.is-bracketed`.
    /// `keywords` carries the named part of a rest-parameter (`$args...`) ArgList:
    /// `None` for ordinary lists; `Some(_)` (possibly empty) after `bind_params`
    /// has bound a rest parameter so `meta.keywords`/`keyword-get` can read the
    /// caller-supplied named arguments and `meta.keywords` can tell a real ArgList
    /// apart from a plain list.
    List {
        items: Vec<SassValue>,
        sep: ListSep,
        bracketed: bool,
        keywords: Option<Vec<(String, SassValue)>>,
    },
    /// An ordered key/value map (insertion order preserved for output).
    Map {
        entries: Vec<(SassValue, SassValue)>,
    },
    /// A first-class function value (`meta.get-function` / `meta.call`, task
    /// group 11 round-73 W1). The `Rc` is the identity: two reads of the same
    /// definition compare `==`, a redefinition does not
    /// (`get_function/equality` goldens). CSS serialization is the call text
    /// `get-function("name")` (`meta/inspect/function.hrx`).
    Function(std::rc::Rc<FnRef>),
    /// A first-class mixin value (`meta.get-mixin` / `meta.apply`); serialized
    /// as `get-mixin("name")` (`values/mixins.hrx` goldens).
    Mixin(std::rc::Rc<MixRef>),
}


/// Structural value equality following Sass semantics: two strings are equal when
/// their text matches regardless of quoting (`base` and `'base'` are the same map
/// key / compare equal with `==`); the `quoted` flag only affects serialization.
/// Numbers must share both magnitude and unit; lists additionally the separator.
impl PartialEq for SassValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (
                Self::Number {
                    value: a,
                    unit: ua,
                },
                Self::Number {
                    value: b,
                    unit: ub,
                },
            ) => a == b && ua == ub,
            (Self::Str { text: a, .. }, Self::Str { text: b, .. }) => a == b,
            (Self::Color(a), Self::Color(b)) => a == b,
            // callable values compare the way dart's goldens spell it:
            // builtins by (module, name) — two `get-function(lighten)` reads
            // are `==` (`equality/built_in/same`) while lighten vs darken is
            // not — and user definitions by reference identity, so a later
            // redefinition yields a fresh `Rc` and compares false
            // (`equality/user_defined/redefined`).
            (Self::Function(a), Self::Function(b)) => fn_kind_eq(a, b),
            (Self::Mixin(a), Self::Mixin(b)) => mix_kind_eq(a, b),
            (
                Self::List {
                    items: a,
                    sep: sa,
                    ..
                },
                Self::List {
                    items: b,
                    sep: sb,
                    ..
                },
            ) => a == b && (a.len() <= 1 || sa == sb),
            (Self::Map { entries: a }, Self::Map { entries: b }) => a == b,
            _ => false,
        }
    }
}

/// An 8-bit-per-channel RGBA color. Channels are stored in `0.0..=255.0`,
/// alpha in `0.0..=1.0`. `hex` records the color's *origin*: a color parsed from
/// a `#rrggbb` literal serializes back to hex, while one produced by a function
/// (`rgb()`/`rgba()`/`mix()`/…) serializes as `rgb()`/`rgba()` — matching how
/// the goldens preserve the representation form. `hex` is ignored by equality.
/// Colors parsed from CSS Color 4 literals carry their space and components in
/// `c4`; the RGBA fields then hold the converted sRGB value and `c4` drives
/// both equality and serialization.
#[derive(Clone, Debug)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
    pub hex: bool,
    pub c4: Option<Box<crate::color4::Color4>>,
    /// Present only for a color parsed from a CSS keyword (`red`, `pink`, …): the
    /// name round-trips on serialization until an operation drops it. Excluded
    /// from equality so a named color still compares equal to its hex form.
    pub name: Option<&'static str>,
    /// Marks a color produced by `color.to-space($c, rgb|hsl|hwb)`: it still
    /// carries `c4` (so `.space`/`.channel` report the modern space) but must
    /// serialize with the goldens' **legacy** CSS form (keyword / `#hex` /
    /// `hsl()` / `rgb(%)` / out-of-gamut `hsl()` fallback) rather than the
    /// `color(...)` wrapper. Holds the requested target name (`rgb`/`hsl`/`hwb`),
    /// which drives the format selection; every other path leaves it `None` so
    /// existing serialization is unchanged.
    pub legacy: Option<&'static str>,
    /// Exact, *unclamped* HSL channels carried alongside a `to-space($c, rgb|hsl|hwb)`
    /// result whose source was `hsl`/`hwb`. the goldens compute those legacy colors
    /// through the HSL formula and keeps the channels verbatim (an out-of-gamut
    /// `hwb(20deg 200% -125%)` → `hsl(200, 11.1111111111%, 212.5%)`), which the
    /// RGB round trip cannot recover. Read only by the legacy `hsl()` fallback in
    /// serialization; excluded from equality.
    pub legacy_hsl: Option<Hsl>,
}

impl PartialEq for Color {
    fn eq(&self, other: &Self) -> bool {
        match (&self.c4, &other.c4) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.r == other.r && self.g == other.g && self.b == other.b && self.a == other.a,
            _ => false,
        }
    }
}

/// HSL space: hue in degrees `0..360`, saturation/lightness `0..=100`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsl {
    pub h: f64,
    pub s: f64,
    pub l: f64,
}

impl SassValue {
    /// Sass truthiness: `null` and `false` are false; everything else (incl. `0`)
    /// is true.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, SassValue::Null | SassValue::Bool(false))
    }

    /// Canonical type name used in error messages and `type-of()`.
    pub fn type_name(&self) -> &'static str {
        match self {
            SassValue::Null => "null",
            SassValue::Bool(_) => "bool",
            SassValue::Number { .. } => "number",
            SassValue::Str { .. } => "string",
            SassValue::Color(_) => "color",
            SassValue::List { .. } => "list",
            SassValue::Map { .. } => "map",
            // `type-of` goldens spell them `function` / `mixin`.
            SassValue::Function(_) => "function",
            SassValue::Mixin(_) => "mixin",
        }
    }

    pub fn as_number(&self) -> Option<(f64, Option<&str>)> {
        match self {
            SassValue::Number { value, unit } => Some((*value, unit.as_deref())),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<&Color> {
        match self {
            SassValue::Color(c) => Some(c),
            _ => None,
        }
    }

    /// Coerce to a display string (used by interpolation and `quote()`/`inspect`).
    /// Colors serialize as their CSS form; numbers carry their unit.
    pub fn to_css(&self) -> String {
        self.render(None, false)
    }

    /// Serialize the way `meta.inspect` (dart's `toSource`) does: identical to
    /// [`SassValue::to_css`] except that a nested unbracketed list is wrapped in
    /// parens whenever it would otherwise merge into its parent, so the value's
    /// list structure round-trips (`(1 2) (3 4)` instead of the flat CSS `1 2 3 4`).
    pub fn to_source(&self) -> String {
        self.render(None, true)
    }

    /// Serialize to text. `parent` is the separator of the enclosing list (if any)
    /// and `source` selects the inspect disambiguation above; scalars ignore both.
    fn render(&self, parent: Option<ListSep>, source: bool) -> String {
        match self {
            // The call-text spellings pinned by `meta/inspect/function.hrx`
            // (`get-function("get-function")`) and `values/mixins.hrx`
            // (`get-mixin("a")`) — identical in CSS and inspect form.
            SassValue::Function(f) => format!("get-function(\"{}\")", f.name),
            SassValue::Mixin(m) => format!("get-mixin(\"{}\")", m.name),
            SassValue::Null => "null".into(),
            SassValue::Bool(b) => b.to_string(),
            SassValue::Number { value, unit } => number_to_css(*value, unit.as_deref()),
            SassValue::Str { text, quoted } => {
                if *quoted {
                    format!("\"{}\"", text.replace('"', "\\\""))
                } else {
                    text.clone()
                }
            }
            SassValue::Color(c) => c.to_css(),
            SassValue::List { items, sep, bracketed, .. } => {
                let join = match sep {
                    ListSep::Comma => ", ",
                    ListSep::Slash => " / ",
                    ListSep::Space => " ",
                };
                let inner = items
                    .iter()
                    .map(|v| v.render(Some(*sep), source))
                    .collect::<Vec<_>>()
                    .join(join);
                if *bracketed {
                    format!("[{inner}]")
                } else if source && items.is_empty() {
                    // An empty unbracketed list has no flat form; `toSource` emits
                    // `()` so it stays distinguishable from null/empty string.
                    "()".to_string()
                } else if source && *sep == ListSep::Comma && items.len() == 1 {
                    // A lone comma element is indistinguishable from a scalar once
                    // flattened, so `toSource` keeps it a round-trippable `(x,)`.
                    format!("({inner},)")
                } else if source && list_needs_parens(*sep, items.is_empty(), parent) {
                    format!("({inner})")
                } else {
                    inner
                }
            }
            SassValue::Map { entries } => {
                // A map behaves like a comma-separated list of pairs. In `toSource`
                // a *comma-separated, unbracketed* list value is wrapped in an extra
                // paren layer so its commas cannot merge with the map's: empty `()`
                // -> `(())`, lone comma `(1,)` -> `((1,))`, multi `1, 2` -> `(1, 2)`.
                // Space/slash/bracketed lists are unambiguous and stay as-is; Map and
                // scalar values and CSS mode (source = false) render plainly.
                let inner = entries
                    .iter()
                    .map(|(k, v)| {
                        let value = match v {
                            SassValue::List {
                                sep: ListSep::Comma,
                                bracketed: false,
                                ..
                            } if source =>
                            {
                                format!("({})", v.render(None, true))
                            }
                            other => other.render(None, source),
                        };
                        format!("{}: {}", k.to_css(), value)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({inner})")
            }
        }
    }

    /// Serialize for a `#{…}` interpolation slot. Unlike [`SassValue::to_css`],
    /// quoted strings are emitted **unquoted** (the goldens drop the quotes when a
    /// string is interpolated), and list/map elements recurse through the same
    /// unquoting rule so `#{$a}` where `$a: 'el'` yields `el`, not `"el"`.
    pub fn interp_text(&self) -> String {
        match self {
            SassValue::Null => String::new(),
            SassValue::Str { text, .. } => text.clone(),
            SassValue::List { items, sep, bracketed, .. } => {
                let join = match sep {
                    ListSep::Comma => ", ",
                    ListSep::Slash => " / ",
                    ListSep::Space => " ",
                };
                let inner = items
                    .iter()
                    .map(|v| v.interp_text())
                    .collect::<Vec<_>>()
                    .join(join);
                if *bracketed {
                    format!("[{inner}]")
                } else {
                    inner
                }
            }
            SassValue::Map { entries } => {
                let inner = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.interp_text(), v.interp_text()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({inner})")
            }
            other => other.to_css(),
        }
    }
}

/// Whether an unbracketed nested list of separator `sep` sitting inside a parent
/// list of `parent` must be parenthesized so the structure round-trips.
/// A top-level list (`parent == None`) is never wrapped. A nested list is grouped
/// when it is empty, shares the parent separator, or the parent is a non-comma
/// list holding a comma child (a space/slash run would otherwise absorb it).
fn list_needs_parens(sep: ListSep, empty: bool, parent: Option<ListSep>) -> bool {
    let Some(p) = parent else {
        return false;
    };
    empty || sep == p || (p != ListSep::Comma && sep == ListSep::Comma)
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: 1.0,
            hex: false,
            c4: None,
            name: None,
            legacy: None,
            legacy_hsl: None,
        }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: f64) -> Self {
        Self {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a,
            hex: false,
            c4: None,
            name: None,
            legacy: None,
            legacy_hsl: None,
        }
    }

    /// A color parsed from a CSS keyword: carries its `#rrggbb` value plus the
    /// source `name` so serialization round-trips the keyword (golden behavior).
    pub fn named(hex: &str, name: &'static str) -> Self {
        let h = hex.trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f64;
        Self {
            r: byte(0),
            g: byte(2),
            b: byte(4),
            a: 1.0,
            hex: true,
            c4: None,
            name: Some(name),
            legacy: None,
            legacy_hsl: None,
        }
    }

    /// Build from HSL with alpha 1.0.
    pub fn from_hsl(h: f64, s: f64, l: f64) -> Self {
        Self::from_hsla(h, s, l, 1.0)
    }

    /// HSL → RGB conversion. Hue is degrees `0..360`; saturation and lightness
    /// are on the `0..=100` scale (matching [`Hsl`] and `to_hsl`).
    pub fn from_hsla(h: f64, s: f64, l: f64, a: f64) -> Self {
        let h = ((h % 360.0) + 360.0) % 360.0;
        let s = (s / 100.0).clamp(0.0, 1.0);
        let l = (l / 100.0).clamp(0.0, 1.0);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r1, g1, b1) = match h {
            hh if hh < 60.0 => (c, x, 0.0),
            hh if hh < 120.0 => (x, c, 0.0),
            hh if hh < 180.0 => (0.0, c, x),
            hh if hh < 240.0 => (0.0, x, c),
            hh if hh < 300.0 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let round = |v: f64| (v + m) * 255.0;
        Self {
            r: round(r1),
            g: round(g1),
            b: round(b1),
            a,
            hex: false,
            c4: None,
            name: None,
            legacy: None,
            legacy_hsl: None,
        }
    }

    /// RGB → HSL. Hue in degrees, saturation/lightness in `0..=100`.
    pub fn to_hsl(&self) -> Hsl {
        let r = self.r / 255.0;
        let g = self.g / 255.0;
        let b = self.b / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let l = (max + min) / 2.0;
        let (h, s) = if delta == 0.0 {
            (0.0, 0.0)
        } else {
            let h = if max == r {
                60.0 * (((g - b) / delta) % 6.0)
            } else if max == g {
                60.0 * ((b - r) / delta + 2.0)
            } else {
                60.0 * ((r - g) / delta + 4.0)
            };
            let s = delta / (1.0 - (2.0 * l - 1.0).abs());
            (((h + 360.0) % 360.0), s)
        };
        Hsl {
            h,
            s: s * 100.0,
            l: l * 100.0,
        }
    }

    // CSS serialization (to_css / legacy_to_css) lives in the sibling
    // `crate::color_css` module to keep this file within the size budget.
}

// `format_number`, `number_to_css`, and `convert_number` now live in the sibling
// `value_number` module and are re-exported at the top of this file.

impl fmt::Display for SassValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_css())
    }
}
