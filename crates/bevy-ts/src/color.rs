//! The one colour parser: every colour bevy-ts takes (TSX props, UI and intrinsic colours, script
//! material params, recipe params) goes through [`parse`]. Forms: a hex string (`#rgb`, `#rgba`,
//! `#rrggbb`, `#rrggbbaa`), an sRGB array `[r, g, b, a?]`, and Bevy's reflected `Color` shape
//! (`{ Srgba: { red, green, blue, alpha } }`, `{ Hsla: {...} }`, every variant). `alpha` may be
//! left out and is 1; every other channel is required, since a colour is a leaf, not merged.

use bevy::color::{Color, Srgba};
use bevy::reflect::serde::TypedReflectDeserializer;
use bevy::reflect::enums::VariantInfo;
use bevy::reflect::{FromReflect, TypeInfo, TypeRegistry};
use serde::de::DeserializeSeed;
use serde_json::{Map, Value};
use std::sync::OnceLock;

const FORMS: &str = "a colour is \"#rrggbb[aa]\", [r, g, b, a?] or { Srgba: { red, green, blue, alpha? } } (any Color variant)";

/// `Color` and its variants' types, registered once for reading the reflected shape.
fn registry() -> &'static TypeRegistry {
    static REGISTRY: OnceLock<TypeRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut r = TypeRegistry::new();
        r.register::<Color>();
        r
    })
}

/// Parses a colour in any form.
pub fn parse(v: &Value) -> Result<Color, String> {
    match v {
        Value::String(s) => Srgba::hex(s).map(Color::from).map_err(|e| format!("colour {s:?}: {e}")),
        Value::Array(a) if a.len() == 3 || a.len() == 4 => {
            let mut c = [1.0f32; 4];
            for (i, x) in a.iter().enumerate() {
                c[i] = x.as_f64().ok_or_else(|| format!("colour {v}: channels are numbers"))? as f32;
            }
            Ok(Color::srgba(c[0], c[1], c[2], c[3]))
        }
        Value::Object(o) if o.len() == 1 => reflected(o),
        _ => Err(format!("{FORMS}, not {v}")),
    }
}

/// Bevy's reflected shape, `{ Variant: { channel: n, ... } }`, with `alpha` 1 when left out.
fn reflected(o: &Map<String, Value>) -> Result<Color, String> {
    let registry = registry();
    let reg = registry.get(std::any::TypeId::of::<Color>()).expect("registered");
    let TypeInfo::Enum(info) = reg.type_info() else { unreachable!("Color is an enum") };
    let (variant, channels) = o.iter().next().expect("one key");
    let names = || info.variant_names().join(", ");
    let Some(VariantInfo::Tuple(t)) = info.variant(variant) else {
        return Err(format!("Color has no variant {variant} (variants: {})", names()));
    };
    let inner = t.field_at(0).expect("a newtype variant").type_info().and_then(|i| i.as_struct().ok()).expect("a colour struct");
    let Value::Object(given) = channels else { return Err(format!("Color::{variant} takes an object of channels, not {channels}")) };
    if let Some(k) = given.keys().find(|k| inner.field(k).is_none()) {
        return Err(format!("Color::{variant} has no channel {k} (channels: {})", inner.field_names().join(", ")));
    }
    let missing: Vec<&str> = inner.field_names().iter().copied().filter(|f| *f != "alpha" && !given.contains_key(*f)).collect();
    if !missing.is_empty() {
        return Err(format!("Color::{variant} is missing {}", missing.join(" and ")));
    }
    let mut given = given.clone();
    given.entry("alpha").or_insert(Value::from(1.0));
    let whole = Value::Object(Map::from_iter([(variant.clone(), Value::Object(given))]));
    let value = TypedReflectDeserializer::new(reg, registry).deserialize(whole).map_err(|e| format!("Color::{variant}: {e}"))?;
    Color::from_reflect(&*value).ok_or_else(|| format!("Color::{variant}: not a colour"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::color::{ColorToComponents, Hsla, LinearRgba};
    use serde_json::json;

    fn srgba(v: Value) -> [f32; 4] {
        parse(&v).unwrap().to_srgba().to_f32_array()
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 3e-3)
    }

    #[test]
    fn hex_in_every_length() {
        assert_eq!(srgba(json!("#f80")), [1.0, 0.53333336, 0.0, 1.0]);
        assert_eq!(srgba(json!("#f808")), [1.0, 0.53333336, 0.0, 0.53333336]);
        assert_eq!(srgba(json!("#ff8800")), [1.0, 0.53333336, 0.0, 1.0]);
        assert_eq!(srgba(json!("#ff880080"))[3], 128.0 / 255.0);
        assert!(parse(&json!("#ggg")).unwrap_err().contains("#ggg"));
    }

    #[test]
    fn arrays_are_srgb_with_alpha_one() {
        assert_eq!(srgba(json!([1, 0.5, 0])), [1.0, 0.5, 0.0, 1.0]);
        assert_eq!(srgba(json!([1, 0.5, 0, 0.25])), [1.0, 0.5, 0.0, 0.25]);
        assert!(parse(&json!([1, 0.5])).is_err());
        assert!(parse(&json!([1, "a", 0])).unwrap_err().contains("numbers"));
    }

    #[test]
    fn every_reflected_variant_reads_with_alpha_one() {
        assert_eq!(parse(&json!({ "Srgba": { "red": 1, "green": 0.5, "blue": 0 } })).unwrap(), Color::srgb(1.0, 0.5, 0.0));
        assert_eq!(
            parse(&json!({ "LinearRgba": { "red": 1, "green": 0.5, "blue": 0, "alpha": 0.5 } })).unwrap(),
            Color::LinearRgba(LinearRgba::new(1.0, 0.5, 0.0, 0.5))
        );
        assert_eq!(parse(&json!({ "Hsla": { "hue": 30, "saturation": 1, "lightness": 0.5 } })).unwrap(), Color::Hsla(Hsla::new(30.0, 1.0, 0.5, 1.0)));
        assert!(parse(&json!({ "Oklcha": { "lightness": 0.7, "chroma": 0.1, "hue": 40 } })).is_ok());
        assert!(parse(&json!({ "Xyza": { "x": 0.5, "y": 0.5, "z": 0.5 } })).is_ok());
        // hsl(30, 1, 0.5) is #ff8000, within 8-bit rounding.
        assert!(close(srgba(json!({ "Hsla": { "hue": 30, "saturation": 1, "lightness": 0.5 } })), srgba(json!("#ff8000"))));
        assert!(close(parse(&json!({ "Srgba": { "red": 1, "green": 0.5, "blue": 0 } })).unwrap().to_linear().to_f32_array(), Color::srgb(1.0, 0.5, 0.0).to_linear().to_f32_array()));
    }

    #[test]
    fn a_missing_channel_is_named() {
        let e = parse(&json!({ "Srgba": { "red": 1 } })).unwrap_err();
        assert!(e.contains("green and blue"), "{e}");
        let e = parse(&json!({ "Srgba": { "red": 1, "green": 1, "blue": 1, "alhpa": 1 } })).unwrap_err();
        assert!(e.contains("alhpa"), "{e}");
        let e = parse(&json!({ "Rgb": { "red": 1 } })).unwrap_err();
        assert!(e.contains("Srgba") && e.contains("Rgb"), "{e}");
        assert!(parse(&json!(3)).unwrap_err().contains("colour"));
    }
}
