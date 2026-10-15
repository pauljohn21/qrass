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
    List {
        items: Vec<SassValue>,
        sep: ListSep,
    },
    /// An ordered key/value map (insertion order preserved for output).
    Map {
        entries: Vec<(SassValue, SassValue)>,
    },
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
            (
                Self::List {
                    items: a,
                    sep: sa,
                },
                Self::List {
                    items: b,
                    sep: sb,
                },
            ) => sa == sb && a == b,
            (Self::Map { entries: a }, Self::Map { entries: b }) => a == b,
            _ => false,
        }
    }
}

/// An 8-bit-per-channel RGBA color. Channels are stored in `0.0..=255.0`,
/// alpha in `0.0..=1.0`. `hex` records the color's *origin*: a color parsed from
/// a `#rrggbb` literal serializes back to hex, while one produced by a function
/// (`rgb()`/`rgba()`/`mix()`/…) serializes as `rgb()`/`rgba()` — matching how
/// dart-sass preserves the representation form. `hex` is ignored by equality.
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
    pub hex: bool,
}

impl PartialEq for Color {
    fn eq(&self, other: &Self) -> bool {
        self.r == other.r && self.g == other.g && self.b == other.b && self.a == other.a
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
        match self {
            SassValue::Null => "null".into(),
            SassValue::Bool(b) => b.to_string(),
            SassValue::Number { value, unit } => match unit {
                Some(u) => format!("{}{u}", format_number(*value)),
                None => format_number(*value),
            },
            SassValue::Str { text, quoted } => {
                if *quoted {
                    format!("\"{}\"", text.replace('"', "\\\""))
                } else {
                    text.clone()
                }
            }
            SassValue::Color(c) => c.to_css(),
            SassValue::List { items, sep } => {
                let join = match sep {
                    ListSep::Comma => ", ",
                    ListSep::Slash => " / ",
                    ListSep::Space => " ",
                };
                items
                    .iter()
                    .map(|v| v.to_css())
                    .collect::<Vec<_>>()
                    .join(join)
            }
            SassValue::Map { entries } => {
                let inner = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.to_css(), v.to_css()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({inner})")
            }
        }
    }

    /// Serialize for a `#{…}` interpolation slot. Unlike [`SassValue::to_css`],
    /// quoted strings are emitted **unquoted** (dart-sass drops the quotes when a
    /// string is interpolated), and list/map elements recurse through the same
    /// unquoting rule so `#{$a}` where `$a: 'el'` yields `el`, not `"el"`.
    pub fn interp_text(&self) -> String {
        match self {
            SassValue::Null => String::new(),
            SassValue::Str { text, .. } => text.clone(),
            SassValue::List { items, sep } => {
                let join = match sep {
                    ListSep::Comma => ", ",
                    ListSep::Slash => " / ",
                    ListSep::Space => " ",
                };
                items
                    .iter()
                    .map(|v| v.interp_text())
                    .collect::<Vec<_>>()
                    .join(join)
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

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: 1.0,
            hex: false,
        }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: f64) -> Self {
        Self {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a,
            hex: false,
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

    /// CSS serialization honoring origin: opaque colors from a hex literal print
    /// as `#rrggbb`, while function-produced colors print as `rgb(r, g, b)`; any
    /// color with alpha `< 1.0` prints as `rgba(r, g, b, a)`.
    pub fn to_css(&self) -> String {
        let clamp = |v: f64| (v.round().clamp(0.0, 255.0)) as u8;
        let (r, g, b) = (clamp(self.r), clamp(self.g), clamp(self.b));
        if self.a >= 1.0 {
            if self.hex {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("rgb({r}, {g}, {b})")
            }
        } else {
            format!("rgba({r}, {g}, {b}, {})", format_number(self.a))
        }
    }
}

/// Serialize a number the way sass does: integral values print without a
/// fractional part; otherwise up to 10 significant digits with trailing zeros
/// trimmed. (Full precision behavior is golden-checked in task group 10.)
pub fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return if v.is_nan() {
            "NaN".into()
        } else if v > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if v == v.round() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let mut s = format!("{v:.10}");
    while s.contains('.') && (s.ends_with('0')) {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// Unit conversion table (design D2): map a number in `from` to `to`, following
/// sass's absolute-unit groups. Returns the converted magnitude if convertible.
pub fn convert_number(value: f64, from: Option<&str>, to: Option<&str>) -> Option<f64> {
    if from == to {
        return Some(value);
    }
    let (Some(from), Some(to)) = (from, to) else {
        return None;
    };
    let base = |u: &str| -> Option<(u8, f64)> {
        // (group, factor to group's base unit)
        Some(match u {
            "in" => (0, 1.0),
            "px" => (0, 1.0 / 96.0),
            "pt" => (0, 1.0 / 72.0),
            "pc" => (0, 1.0 / 6.0),
            "cm" => (0, 1.0 / 2.54),
            "mm" => (0, 1.0 / 25.4),
            "q" => (0, 1.0 / 101.6),
            "deg" => (1, 1.0),
            "grad" => (1, 0.9),
            "rad" => (1, 180.0 / std::f64::consts::PI),
            "turn" => (1, 360.0),
            "s" => (2, 1.0),
            "ms" => (2, 0.001),
            "hz" => (3, 1.0),
            "khz" => (3, 1000.0),
            "dpi" => (4, 1.0),
            "dpcm" => (4, 2.54),
            "dppx" => (4, 96.0),
            _ => return None,
        })
    };
    let (gf, vf) = base(from)?;
    let (gt, vt) = base(to)?;
    if gf != gt {
        return None;
    }
    // value × factor_into_base ÷ factor_into_base_of_target
    Some(value * vf / vt)
}

impl fmt::Display for SassValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_css())
    }
}
