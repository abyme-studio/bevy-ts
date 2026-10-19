//! Native layout bytes to and from JSON values, little-endian. Used for defaults, migration,
//! the plugin API's encoding and the protocol's reads and edits.

use crate::layout::{ComponentLayout, FieldType};
use crate::manifest::{ComponentDecl, FieldHints};
use bevy::prelude::Entity;
use serde_json::{Map, Value, json};

/// A JSON value that does not fit a field type.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueError {
    /// The path written, `.field`, `.field.x` or `.field[2]`.
    pub path: String,
    pub expected: String,
    pub message: String,
}

impl std::fmt::Display for ValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} expects {}: {}", self.path, self.expected, self.message)
    }
}

fn err(path: &str, ty: &FieldType, message: impl Into<String>) -> ValueError {
    ValueError { path: path.to_string(), expected: ty.ts_name(), message: message.into() }
}

fn as_number(path: &str, ty: &FieldType, v: &Value) -> Result<f64, ValueError> {
    v.as_f64().ok_or_else(|| err(path, ty, format!("got {v}")))
}

fn as_int(path: &str, ty: &FieldType, v: &Value, min: f64, max: f64) -> Result<i64, ValueError> {
    let n = as_number(path, ty, v)?;
    if n.fract() != 0.0 || n < min || n > max {
        return Err(err(path, ty, format!("{n} is not an integer in {min}..={max}")));
    }
    Ok(n as i64)
}

/// Encodes one value of `ty` into `out` (exactly `ty.size()` bytes).
pub fn encode_field(path: &str, ty: &FieldType, v: &Value, out: &mut [u8]) -> Result<(), ValueError> {
    debug_assert_eq!(out.len(), ty.size());
    match ty {
        FieldType::F32 => out.copy_from_slice(&(as_number(path, ty, v)? as f32).to_le_bytes()),
        FieldType::F64 => out.copy_from_slice(&as_number(path, ty, v)?.to_le_bytes()),
        FieldType::I8 => out.copy_from_slice(&(as_int(path, ty, v, i8::MIN as f64, i8::MAX as f64)? as i8).to_le_bytes()),
        FieldType::U8 => out.copy_from_slice(&(as_int(path, ty, v, 0.0, u8::MAX as f64)? as u8).to_le_bytes()),
        FieldType::I16 => {
            out.copy_from_slice(&(as_int(path, ty, v, i16::MIN as f64, i16::MAX as f64)? as i16).to_le_bytes())
        }
        FieldType::U16 => out.copy_from_slice(&(as_int(path, ty, v, 0.0, u16::MAX as f64)? as u16).to_le_bytes()),
        FieldType::I32 => {
            out.copy_from_slice(&(as_int(path, ty, v, i32::MIN as f64, i32::MAX as f64)? as i32).to_le_bytes())
        }
        FieldType::U32 => out.copy_from_slice(&(as_int(path, ty, v, 0.0, u32::MAX as f64)? as u32).to_le_bytes()),
        FieldType::Bool => out[0] = v.as_bool().ok_or_else(|| err(path, ty, format!("got {v}")))? as u8,
        FieldType::Vec2 | FieldType::Vec3 | FieldType::Vec4 | FieldType::Quat => {
            let lanes = ty.lanes().unwrap();
            let names = ["x", "y", "z", "w"];
            let values: Vec<f64> = match v {
                Value::Array(a) if a.len() == lanes => {
                    a.iter().map(|x| as_number(path, ty, x)).collect::<Result<_, _>>()?
                }
                Value::Object(o) => names[..lanes]
                    .iter()
                    .map(|n| o.get(*n).ok_or_else(|| err(path, ty, format!("missing {n}"))).and_then(|x| as_number(path, ty, x)))
                    .collect::<Result<_, _>>()?,
                _ => return Err(err(path, ty, format!("got {v}"))),
            };
            for (i, x) in values.iter().enumerate() {
                out[i * 4..i * 4 + 4].copy_from_slice(&(*x as f32).to_le_bytes());
            }
        }
        FieldType::Entity => {
            let bits = match v {
                Value::Null => Entity::PLACEHOLDER.to_bits(),
                Value::Number(n) => n.as_u64().ok_or_else(|| err(path, ty, format!("got {v}")))?,
                Value::String(s) => s.parse().map_err(|_| err(path, ty, format!("got {v}")))?,
                _ => return Err(err(path, ty, format!("got {v}"))),
            };
            Entity::try_from_bits(bits).ok_or_else(|| err(path, ty, format!("{bits} is not an entity")))?;
            out.copy_from_slice(&bits.to_le_bytes());
        }
        FieldType::Enum { variants } => {
            let index = match v {
                Value::String(s) => variants
                    .iter()
                    .position(|x| x == s)
                    .ok_or_else(|| err(path, ty, format!("unknown variant {s:?}")))?,
                Value::Number(_) => {
                    let i = as_int(path, ty, v, 0.0, (variants.len() - 1) as f64)?;
                    i as usize
                }
                _ => return Err(err(path, ty, format!("got {v}"))),
            };
            out.copy_from_slice(&(index as u32).to_le_bytes()[..out.len()]);
        }
        FieldType::Array { of, len } => {
            let a = v.as_array().filter(|a| a.len() == *len as usize).ok_or_else(|| err(path, ty, format!("got {v}")))?;
            let w = of.size();
            for (i, x) in a.iter().enumerate() {
                encode_field(&format!("{path}[{i}]"), of, x, &mut out[i * w..(i + 1) * w])?;
            }
        }
    }
    Ok(())
}

/// Decodes one value of `ty` from exactly `ty.size()` bytes.
pub fn decode_field(ty: &FieldType, b: &[u8]) -> Value {
    let f32_at = |i: usize| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    match ty {
        FieldType::F32 => json!(f32_at(0)),
        FieldType::F64 => json!(f64::from_le_bytes(b[..8].try_into().unwrap())),
        FieldType::I8 => json!(b[0] as i8),
        FieldType::U8 => json!(b[0]),
        FieldType::I16 => json!(i16::from_le_bytes(b[..2].try_into().unwrap())),
        FieldType::U16 => json!(u16::from_le_bytes(b[..2].try_into().unwrap())),
        FieldType::I32 => json!(i32::from_le_bytes(b[..4].try_into().unwrap())),
        FieldType::U32 => json!(u32::from_le_bytes(b[..4].try_into().unwrap())),
        FieldType::Bool => json!(b[0] != 0),
        FieldType::Vec2 | FieldType::Vec3 | FieldType::Vec4 | FieldType::Quat => {
            Value::Array((0..ty.lanes().unwrap()).map(|i| json!(f32_at(i))).collect())
        }
        FieldType::Entity => json!(u64::from_le_bytes(b[..8].try_into().unwrap())),
        FieldType::Enum { variants } => {
            let mut raw = [0u8; 4];
            raw[..b.len()].copy_from_slice(b);
            let i = u32::from_le_bytes(raw) as usize;
            variants.get(i).map(|s| json!(s)).unwrap_or(json!(i))
        }
        FieldType::Array { of, len } => {
            let w = of.size();
            Value::Array((0..*len as usize).map(|i| decode_field(of, &b[i * w..(i + 1) * w])).collect())
        }
    }
}

/// Decodes a whole component into a JSON object of its fields.
pub fn decode_component(layout: &ComponentLayout, bytes: &[u8]) -> Value {
    let mut m = Map::new();
    for f in &layout.fields {
        m.insert(f.name.clone(), decode_field(&f.ty, &bytes[f.offset..f.offset + f.ty.size()]));
    }
    Value::Object(m)
}

/// The zero value of a field, as JSON.
pub fn zero(ty: &FieldType) -> Value {
    decode_field(ty, &vec![0u8; ty.size()])
}

/// Encodes a map of field names to values into a component's native bytes, filling every field
/// the map leaves out with its declared default (zero if none). Unknown fields are errors.
pub fn encode_component(decl: &ComponentDecl, layout: &ComponentLayout, values: &Map<String, Value>) -> Result<Vec<u8>, ValueError> {
    for key in values.keys() {
        if layout.field(key).is_none() {
            return Err(ValueError {
                path: format!(".{key}"),
                expected: "a declared field".into(),
                message: format!("{} has no field {key}", decl.name),
            });
        }
    }
    let mut out = vec![0u8; layout.size];
    for f in &layout.fields {
        let path = format!(".{}", f.name);
        let slot = &mut out[f.offset..f.offset + f.ty.size()];
        if let Some(v) = values.get(&f.name) {
            encode_field(&path, &f.ty, v, slot)?;
        } else if let Some(d) = decl.field(&f.name).and_then(|d| d.default.as_ref()) {
            encode_field(&path, &f.ty, d, slot)?;
        }
    }
    Ok(out)
}

/// Checks a numeric value against editor hints.
pub fn check_hints(path: &str, ty: &FieldType, hints: &FieldHints, v: &Value) -> Result<(), ValueError> {
    if let Some(n) = v.as_f64() {
        if let Some(min) = hints.min
            && n < min
        {
            return Err(err(path, ty, format!("{n} is below the minimum {min}")));
        }
        if let Some(max) = hints.max
            && n > max
        {
            return Err(err(path, ty, format!("{n} is above the maximum {max}")));
        }
    }
    Ok(())
}

/// A resolved edit path into a component: the byte range and the type there.
#[derive(Clone, Debug, PartialEq)]
pub struct PathTarget {
    pub offset: usize,
    pub ty: FieldType,
    /// The top-level field the path is in.
    pub field: String,
}

/// Resolves `.field`, `.field.x|y|z|w` and `.field[i]`.
pub fn resolve_path(layout: &ComponentLayout, path: &str) -> Result<PathTarget, ValueError> {
    let bad = |message: String| ValueError { path: path.to_string(), expected: "a field path".into(), message };
    let rest = path.strip_prefix('.').ok_or_else(|| bad("paths start with '.'".into()))?;
    let end = rest.find(['.', '[']).unwrap_or(rest.len());
    let (name, mut rest) = rest.split_at(end);
    let f = layout.field(name).ok_or_else(|| bad(format!("no field {name}")))?;
    let mut target = PathTarget { offset: f.offset, ty: f.ty.clone(), field: name.to_string() };
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            let (lane, r2) = r.split_at(end);
            let lanes = target.ty.lanes().ok_or_else(|| bad(format!("{} has no lanes", target.ty.ts_name())))?;
            let i = ["x", "y", "z", "w"].iter().position(|x| *x == lane).filter(|i| *i < lanes);
            let i = i.ok_or_else(|| bad(format!("no lane {lane} on {}", target.ty.ts_name())))?;
            target = PathTarget { offset: target.offset + i * 4, ty: FieldType::F32, field: target.field };
            rest = r2;
        } else if let Some(r) = rest.strip_prefix('[') {
            let close = r.find(']').ok_or_else(|| bad("unclosed [".into()))?;
            let i: usize = r[..close].parse().map_err(|_| bad(format!("bad index {}", &r[..close])))?;
            let FieldType::Array { of, len } = &target.ty else {
                return Err(bad(format!("{} is not an array", target.ty.ts_name())));
            };
            if i >= *len as usize {
                return Err(bad(format!("index {i} out of 0..{len}")));
            }
            let of = (**of).clone();
            target = PathTarget { offset: target.offset + i * of.size(), ty: of, field: target.field };
            rest = &r[close + 1..];
        } else {
            return Err(bad(format!("unexpected {rest}")));
        }
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::FieldDecl;

    fn health() -> ComponentDecl {
        ComponentDecl {
            name: "game::Health".into(),
            fields: vec![
                FieldDecl { name: "hp".into(), ty: FieldType::F32, default: None, hints: Default::default() },
                FieldDecl { name: "max".into(), ty: FieldType::F32, default: Some(json!(100)), hints: Default::default() },
            ],
            location: None,
        }
    }

    #[test]
    fn encodes_a_partial_value_with_defaults() {
        let d = health();
        let bytes = encode_component(&d, &d.layout(), json!({"hp": 50}).as_object().unwrap()).unwrap();
        assert_eq!(bytes.len(), 8);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 50.0);
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 100.0);
    }

    #[test]
    fn type_errors_name_the_field() {
        let d = health();
        let e = encode_component(&d, &d.layout(), json!({"hp": "wide"}).as_object().unwrap()).unwrap_err();
        assert_eq!(e.path, ".hp");
        assert_eq!(e.expected, "f32");
    }

    #[test]
    fn paths() {
        let l = ComponentLayout::c_layout([
            ("a", &FieldType::Bool),
            ("p", &FieldType::Vec3),
            ("xs", &FieldType::Array { of: Box::new(FieldType::U16), len: 4 }),
        ]);
        assert_eq!(resolve_path(&l, ".p.y").unwrap().offset, 8);
        assert_eq!(resolve_path(&l, ".xs[2]").unwrap(), PathTarget { offset: 20, ty: FieldType::U16, field: "xs".into() });
        assert!(resolve_path(&l, ".p.w").is_err());
        assert!(resolve_path(&l, ".nope").is_err());
    }

    #[test]
    fn round_trip() {
        let ty = FieldType::Enum { variants: vec!["idle".into(), "run".into()] };
        let mut b = [0u8; 1];
        encode_field(".s", &ty, &json!("run"), &mut b).unwrap();
        assert_eq!(decode_field(&ty, &b), json!("run"));
        let mut v = [0u8; 12];
        encode_field(".p", &FieldType::Vec3, &json!({"x":1,"y":2,"z":3}), &mut v).unwrap();
        assert_eq!(decode_field(&FieldType::Vec3, &v), json!([1.0, 2.0, 3.0]));
    }
}
