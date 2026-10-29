//! `color.to-space` — moving a colour between the colour spaces [`crate::color::space`] models.
//!
//! Split out of the domain root because it is neither a constructor nor an axis operation: it reads a colour,
//! asks one question of the conversion table, and produces a colour in a different space. The two legacy
//! targets (`rgb`, `hsl`) are answered by the representations that already print them, which is what the
//! corpus distinguishes: the same `color(srgb 0.2 0.4 0.8)` becomes `#3366cc` under `rgb` and
//! `hsl(220, 60%, 50%)` under `hsl` (`to_space/srgb/{rgb,hsl}.hrx::middle`).

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::Space;
use crate::value::Value;

/// `color.to-space($color, $space)`. Registered as a **named** two-parameter builtin so `$color:` and
/// `$space:` bind, and variadic so an over-long call reports its own arity instead of being misread.
pub(super) static COLOR_TO_SPACE: Builtin = Builtin {
    name: "color.to-space",
    params: &["color", "space"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: to_space,
};

/// The target space as the call named it. A legacy name wins over the space table because the two produce
/// different **forms**: `color.to-space($c, rgb)` must read `#3366cc`, not `color(srgb 0.2 0.4 0.8)`
/// (`to_space/srgb/rgb.hrx::middle`).
fn target(name: &str) -> Option<fn(&crate::color::Color) -> crate::color::Color> {
    match name.to_ascii_lowercase().replace('_', "-").as_str() {
        "rgb" => Some(|color| color.as_legacy_rgb()),
        "hsl" => Some(|color| color.as_legacy_hsl()),
        _ => None,
    }
}

fn to_space(args: Vec<Value>) -> Result<Value, SassError> {
    let decline = || SassError::new("not a colour space this slice can convert to");
    let [source, name] = args.as_slice() else {
        return Err(decline());
    };
    let (Value::Color(color), Value::Raw(text)) = (source, name) else {
        return Err(decline());
    };
    // `hwb` is deliberately absent: the fixtures that convert to it print through a module mixin
    // (`to_space/srgb/hwb.hrx`), so nothing here states the form, and a colour is not guessed a printing.
    if let Some(legacy) = target(text) {
        return Ok(Value::Color(legacy(&color)));
    }
    let space = Space::parse(text).ok_or_else(decline)?;
    Ok(Value::Color(color.converted(space)))
}
