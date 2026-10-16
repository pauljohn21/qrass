//! CSS serialization for [`Color`] values. Split out of `value.rs` to keep that
//! module within the source-size budget; these are inherent `impl Color` methods
//! defined in a sibling module (allowed because every `Color` field is `pub`).

use crate::value::{format_number, Color};

impl Color {
    /// CSS serialization honoring origin: opaque colors from a hex literal print
    /// as `#rrggbb`, while function-produced colors print as `rgb(r, g, b)`; any
    /// color with alpha `< 1.0` prints as `rgba(r, g, b, a)`.
    pub fn to_css(&self) -> String {
        if let Some(fmt) = self.legacy {
            return self.legacy_to_css(fmt);
        }
        if let Some(c4) = &self.c4 {
            return c4.to_css();
        }
        let clamp = |v: f64| (v.round().clamp(0.0, 255.0)) as u8;
        let (r, g, b) = (clamp(self.r), clamp(self.g), clamp(self.b));
        if self.a >= 1.0 {
            if let Some(n) = self.name {
                return n.to_string();
            }
            if self.hex {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("rgb({r}, {g}, {b})")
            }
        } else {
            format!("rgba({r}, {g}, {b}, {})", format_number(self.a))
        }
    }

    /// the goldens' legacy CSS form for a `to-space($c, rgb|hsl|hwb)` result.
    /// `fmt` is the target space name. Reproduced from the goldens:
    /// - `hsl` → always `hsl(h, s%, l%)` / `hsla(..)` (hue 0..360, s/l percents,
    ///   unclamped so out-of-gamut yields e.g. `hsl(212, 300%, 50%)`).
    /// - `rgb`/`hwb` → a keyword if the integer channels hit a named color, else
    ///   `#hex` when the channels are exact 0..255 integers; otherwise an
    ///   `rgb(%…)` / `rgba(%…)` form while in gamut, falling back to `hsl()`
    ///   when a channel leaves `[0, 255]` (a legacy rgb() cannot hold it).
    ///   `hwb` uses the `hsl()` fallback for any non-integer in-gamut value.
    fn legacy_to_css(&self, fmt: &str) -> String {
        let opaque = self.a >= 1.0;
        // A channel is "integral" when it sits within a hair of a whole byte.
        // The Color 4 round trip (via the XYZ hub) perturbs exact 0/255 by ~1e-5,
        // so the tolerance must clear that noise while still rejecting genuine
        // fractional channels such as a 50% gray (127.5, half a byte off).
        let int_of = |v: f64| -> Option<u8> {
            let r = v.round();
            if (v - r).abs() < 1e-4 && (r >= 0.0) && (r <= 255.0) {
                Some(r as u8)
            } else {
                None
            }
        };
        let in_gamut = self.r >= -1e-4
            && self.g >= -1e-4
            && self.b >= -1e-4
            && self.r <= 255.0001
            && self.g <= 255.0001
            && self.b <= 255.0001;
        let pct = |v: f64| format!("{}%", format_number(v / 255.0 * 100.0));

        if fmt != "hsl" {
            if let (Some(r), Some(g), Some(b)) =
                (int_of(self.r), int_of(self.g), int_of(self.b))
            {
                if opaque {
                    if let Some(n) = crate::parser::named_colors::named_for_rgb(r, g, b) {
                        return n.to_string();
                    }
                    return format!("#{r:02x}{g:02x}{b:02x}");
                }
                return format!("rgba({r}, {g}, {b}, {})", format_number(self.a));
            }
            if in_gamut && fmt == "rgb" {
                let ps = [pct(self.r), pct(self.g), pct(self.b)];
                return if opaque {
                    format!("rgb({}, {}, {})", ps[0], ps[1], ps[2])
                } else {
                    format!("rgba({}, {}, {}, {})", ps[0], ps[1], ps[2], format_number(self.a))
                };
            }
        }
        // hsl form (explicit hsl target, or in-gamut `hwb`, or out-of-gamut).
        // A carried verbatim HSL triple (an hsl/hwb `to-space` source) wins:
        // the goldens emit its formula channels exactly, out of gamut included.
        // Otherwise the triple is recomputed from RGB. The XYZ hub perturbs a
        // neutral's channels by ~1e-5; left raw that tiny spread makes HSL's
        // `s = delta / (1 - |2l - 1|)` blow up at the l = 0/1 ends (white →
        // `hsl(_, 175%, 100%)`). Snap a near-equal triple to a clean neutral so
        // white/black/gray emit `hsl(0, 0%, L%)` like the goldens.
        let hsl = match self.legacy_hsl {
            Some(h) => h,
            None => {
                let clean = |v: f64| {
                    let r = v.round();
                    if (v - r).abs() < 1e-4 {
                        r
                    } else {
                        v
                    }
                };
                let (cr, cg, cb) = (clean(self.r), clean(self.g), clean(self.b));
                let spread = cr.max(cg).max(cb) - cr.min(cg).min(cb);
                if spread < 1e-3 {
                    let avg = (cr + cg + cb) / 3.0;
                    crate::value::Hsl { h: 0.0, s: 0.0, l: avg / 255.0 * 100.0 }
                } else {
                    self.to_hsl()
                }
            }
        };
        // the goldens' HSL color normalizes a negative saturation (produced by an
        // out-of-range lightness in the HWB→HSL inversion) by taking `abs(s)` and
        // rotating the hue 180°: `hsl(20, -11.11%, 212.5%)` emits `hsl(200,
        // 11.11%, 212.5%)`.
        let (hn, sn) = if hsl.s < 0.0 {
            (hsl.h + 180.0, -hsl.s)
        } else {
            (hsl.h, hsl.s)
        };
        let h = format_number(((hn % 360.0) + 360.0) % 360.0);
        let s = format!("{}%", format_number(sn));
        let l = format!("{}%", format_number(hsl.l));
        if opaque {
            format!("hsl({h}, {s}, {l})")
        } else {
            format!("hsla({h}, {s}, {l}, {})", format_number(self.a))
        }
    }
}
