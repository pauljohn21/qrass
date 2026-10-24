//! Sass `color.*` — a minimal, verifiable colour kernel: parse named / `#hex` / `rgb()` /
//! `rgba()` sources and expose channel getters plus `hue`/`saturation`/`lightness`/`mix`.
//! Colours are parsed from the argument's own text on demand (no change to `Value::Color`
//! storage), keeping this module self-contained. The wider colour surface (hsl/hwb
//! construction, `adjust`/`change`, gamut mapping, CSS-4 0–1 channels, named re-serialisation)
//! is intentionally out of scope for this step.

use crate::expr::num;
use crate::value::Value;

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Rgba {
    r: f64, // 0..255
    g: f64,
    b: f64,
    a: f64, // 0..1
}

/// `Some` when this domain owns `name`.
pub fn call(name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match name {
        "color.red" | "red" => Some(chan(args, |x| x.r)),
        "color.green" | "green" => Some(chan(args, |x| x.g)),
        "color.blue" | "blue" => Some(chan(args, |x| x.b)),
        "color.alpha" | "alpha" | "color.opacity" | "opacity" => Some(chan(args, |x| x.a)),
        "color.hue" | "hue" => Some(hue_of(args)),
        "color.saturation" | "saturation" => Some(chan(args, |x| hsl(x).1)),
        "color.lightness" | "lightness" => Some(chan(args, |x| hsl(x).2)),
        "color.mix" | "mix" => Some(mix(args)),
        _ => None,
    }
}

fn chan(args: &[Value], f: fn(Rgba) -> f64) -> Result<Value, String> {
    let [a] = args else { return Err("expects one colour argument".into()) };
    Ok(Value::Number(f(parse(a)?), String::new()))
}

fn hue_of(args: &[Value]) -> Result<Value, String> {
    let [a] = args else { return Err("expects one colour argument".into()) };
    let h = hsl(parse(a)?).0;
    Ok(Value::Number(h, "deg".into()))
}

/// Weighted sRGB mix (`color.mix($c1, $c2, $weight)`; weight is c1's share, default 50%).
fn mix(args: &[Value]) -> Result<Value, String> {
    let [a, b] = args else {
        if args.len() == 3 {
            let (x, y) = (parse(&args[0])?, parse(&args[1])?);
            let w = num(&args[2])?.0 / 100.0;
            return Ok(Value::Color(encode(blend(x, y, w))));
        }
        return Err("mix expects 2 or 3 arguments".into());
    };
    Ok(Value::Color(encode(blend(parse(a)?, parse(b)?, 0.5))))
}

fn blend(x: Rgba, y: Rgba, w: f64) -> Rgba {
    let l = |p: f64, q: f64| p * w + q * (1.0 - w);
    Rgba { r: l(x.r, y.r), g: l(x.g, y.g), b: l(x.b, y.b), a: l(x.a, y.a) }
}

/// rgb -> (hue deg, saturation %, lightness %).
fn hsl(c: Rgba) -> (f64, f64, f64) {
    let (r, g, b) = (c.r / 255.0, c.g / 255.0, c.b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l * 100.0);
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let h = (h * 60.0 + 360.0) % 360.0;
    (h, s * 100.0, l * 100.0)
}

/// Parse a colour value from named / `#hex` / `rgb()` / `rgba()` text.
pub(crate) fn parse(v: &Value) -> Result<Rgba, String> {
    let text = match v {
        Value::Color(s) | Value::Raw(s) => s.trim().to_string(),
        other => other.to_css().trim().to_string(),
    };
    if let Some(&rgb) = NAMED.get(&text.to_lowercase().as_str()) {
        let (r, g, b) = rgb;
        return Ok(Rgba { r, g, b, a: 1.0 });
    }
    if let Some(hex) = text.strip_prefix('#') {
        return hex_to_rgba(hex).ok_or_else(|| format!("invalid hex colour \"{text}\""));
    }
    if text.starts_with("rgb") {
        return rgb_fn(&text).ok_or_else(|| format!("invalid rgb colour \"{text}\""));
    }
    Err(format!("unknown colour \"{text}\""))
}

fn hex_to_rgba(hex: &str) -> Option<Rgba> {
    let d = |s: &str| u8::from_str_radix(s, 16).ok().map(|v| v as f64);
    match hex.len() {
        3 | 4 => {
            let cs: Vec<char> = hex.chars().collect();
            let dup = |c: char| format!("{c}{c}");
            Some(Rgba {
                r: d(&dup(cs[0]))?,
                g: d(&dup(cs[1]))?,
                b: d(&dup(cs[2]))?,
                a: cs.get(3).map_or(1.0, |&c| d(&dup(c)).unwrap_or(255.0) / 255.0),
            })
        }
        6 | 8 => {
            let b = |i: usize| d(&hex[i * 2..i * 2 + 2]);
            Some(Rgba { r: b(0)?, g: b(1)?, b: b(2)?, a: if hex.len() == 8 { b(3)? / 255.0 } else { 1.0 } })
        }
        _ => None,
    }
}

fn rgb_fn(text: &str) -> Option<Rgba> {
    let inner = text.split_once('(')?.1.trim_end_matches(')');
    let parts: Vec<f64> = inner.split([',', ' ', '/']).filter(|p| !p.is_empty()).filter_map(|p| p.trim_end_matches('%').parse().ok()).collect();
    if parts.len() < 3 {
        return None;
    }
    Some(Rgba { r: parts[0], g: parts[1], b: parts[2], a: parts.get(3).copied().unwrap_or(1.0) })
}

/// Serialise as `#rrggbb` (opaque) — a stable, testable form for this step.
fn encode(c: Rgba) -> String {
    let h = |v: f64| format!("{:02x}", v.round().clamp(0.0, 255.0) as u8);
    format!("#{}{}{}", h(c.r), h(c.g), h(c.b))
}

use std::collections::HashMap;
use std::sync::LazyLock;

static NAMED: LazyLock<HashMap<&'static str, (f64, f64, f64)>> = LazyLock::new(|| {
    [("black", (0.0, 0.0, 0.0)), ("white", (255.0, 255.0, 255.0)), ("red", (255.0, 0.0, 0.0)),
     ("green", (0.0, 128.0, 0.0)), ("blue", (0.0, 0.0, 255.0)), ("yellow", (255.0, 255.0, 0.0)),
     ("cyan", (0.0, 255.0, 255.0)), ("magenta", (255.0, 0.0, 255.0)), ("gray", (128.0, 128.0, 128.0))]
        .into_iter().collect()
});
