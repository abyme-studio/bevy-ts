//! Native layout bytes to and from JSON values, little-endian. Used for defaults, migration,
//! the plugin API's encoding and the protocol's reads and edits. A heap field (`string`) is a
//! JSON string; its value lives in a [`HeapBytes`], never in loose bytes.

use crate::heap::{HeapBytes, HeapShape, HeapValue, List};
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

/// A heap field's value from JSON: a string, or an array of the list's item type.
pub fn heap_value(path: &str, ty: &FieldType, v: &Value) -> Result<HeapValue, ValueError> {
    match ty {
        FieldType::Str => v.as_str().map(|s| HeapValue::Str(s.to_string())).ok_or_else(|| err(path, ty, format!("got {v}"))),
        FieldType::List { of } => {
            let a = v.as_array().ok_or_else(|| err(path, ty, format!("got {v}")))?;
            // Each item checked as a field of the item type, then read back as its Rust value.
            fn items<T>(path: &str, of: &FieldType, a: &[Value], read: impl Fn(&[u8]) -> T) -> Result<Vec<T>, ValueError> {
                let mut b = vec![0u8; of.size()];
                a.iter()
                    .enumerate()
                    .map(|(i, x)| encode_field(&format!("{path}[{i}]"), of, x, &mut b).map(|_| read(&b)))
                    .collect()
            }
            let list = match **of {
                FieldType::F32 => List::F32(items(path, of, a, |b| f32::from_le_bytes(b.try_into().unwrap()))?),
                FieldType::F64 => List::F64(items(path, of, a, |b| f64::from_le_bytes(b.try_into().unwrap()))?),
                FieldType::I8 => List::I8(items(path, of, a, |b| b[0] as i8)?),
                FieldType::U8 => List::U8(items(path, of, a, |b| b[0])?),
                FieldType::I16 => List::I16(items(path, of, a, |b| i16::from_le_bytes(b.try_into().unwrap()))?),
                FieldType::U16 => List::U16(items(path, of, a, |b| u16::from_le_bytes(b.try_into().unwrap()))?),
                FieldType::I32 => List::I32(items(path, of, a, |b| i32::from_le_bytes(b.try_into().unwrap()))?),
                FieldType::U32 => List::U32(items(path, of, a, |b| u32::from_le_bytes(b.try_into().unwrap()))?),
                FieldType::Bool => List::Bool(items(path, of, a, |b| b[0] != 0)?),
                FieldType::Str => List::Str(
                    a.iter()
                        .enumerate()
                        .map(|(i, x)| x.as_str().map(str::to_string).ok_or_else(|| err(&format!("{path}[{i}]"), of, format!("got {x}"))))
                        .collect::<Result<_, _>>()?,
                ),
                _ => return Err(err(path, ty, "not a list of numbers, bool or string")),
            };
            Ok(HeapValue::List(list))
        }
        _ => Err(err(path, ty, "not a heap field")),
    }
}

/// A heap field's value as JSON.
pub fn heap_json(v: &HeapValue) -> Value {
    match v {
        HeapValue::Str(s) => json!(s),
        HeapValue::List(l) => match l {
            List::F32(x) => json!(x),
            List::F64(x) => json!(x),
            List::I8(x) => json!(x),
            List::U8(x) => json!(x),
            List::I16(x) => json!(x),
            List::U16(x) => json!(x),
            List::I32(x) => json!(x),
            List::U32(x) => json!(x),
            List::Bool(x) => json!(x),
            List::Str(x) => json!(x),
        },
    }
}

/// Encodes one value of `ty` into `out` (exactly `ty.size()` bytes). A heap type is only checked:
/// its value is built by [`encode_component`] or written whole by its owner, never into loose
/// bytes.
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
        FieldType::Enum { variants, .. } => {
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
            let stored = ty.enum_value(index).expect("index checked against variants");
            out.copy_from_slice(&stored.to_le_bytes()[..out.len()]);
        }
        FieldType::Array { of, len } => {
            let a = v.as_array().filter(|a| a.len() == *len as usize).ok_or_else(|| err(path, ty, format!("got {v}")))?;
            let w = of.size();
            for (i, x) in a.iter().enumerate() {
                encode_field(&format!("{path}[{i}]"), of, x, &mut out[i * w..(i + 1) * w])?;
            }
        }
        FieldType::Str | FieldType::List { .. } => {
            heap_value(path, ty, v)?;
        }
        // An object of its fields; a field left out is zero.
        FieldType::Timer | FieldType::Stopwatch => {
            let layout = ty.fields().unwrap();
            let o = v.as_object().ok_or_else(|| err(path, ty, format!("got {v}")))?;
            if let Some(k) = o.keys().find(|k| layout.field(k).is_none()) {
                return Err(err(path, ty, format!("{} has no field {k}", ty.ts_name())));
            }
            out.fill(0);
            for f in &layout.fields {
                if let Some(x) = o.get(&f.name) {
                    encode_field(&format!("{path}.{}", f.name), &f.ty, x, &mut out[f.offset..f.offset + f.ty.size()])?;
                }
            }
        }
    }
    Ok(())
}

/// Decodes one value of `ty` from exactly `ty.size()` bytes. For a heap type they are the slot of
/// a live value (a column's or a [`HeapBytes`]'), whose `String` is read in place.
///
/// # Safety
/// `b` is a live value of `ty`: for a heap type, or a struct holding one, a column's or a
/// [`HeapBytes`]' bytes, never copied or zeroed ones. Bytes of any other type may be anything.
pub unsafe fn decode_field(ty: &FieldType, b: &[u8]) -> Value {
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
        FieldType::Enum { variants, .. } => {
            let mut raw = [0u8; 4];
            raw[..b.len()].copy_from_slice(b);
            let stored = u32::from_le_bytes(raw);
            ty.enum_index(stored).and_then(|i| variants.get(i)).map(|s| json!(s)).unwrap_or(json!(stored))
        }
        FieldType::Array { of, len } => {
            let w = of.size();
            // SAFETY: an array's items are plain (`validate` refuses heap items).
            Value::Array((0..*len as usize).map(|i| unsafe { decode_field(of, &b[i * w..(i + 1) * w]) }).collect())
        }
        // SAFETY: a timer's fields are plain.
        FieldType::Timer | FieldType::Stopwatch => unsafe { decode_component(ty.fields().unwrap(), b) },
        FieldType::Str | FieldType::List { .. } => {
            assert_eq!(b.len(), ty.size(), "a heap slot");
            let kind = crate::heap::HeapKind::of(ty).expect("a heap field");
            // SAFETY: the bytes are a live value's slot (this function's contract).
            heap_json(&unsafe { crate::heap::read(b.as_ptr(), 0, kind) })
        }
    }
}

/// Decodes a whole component into a JSON object of its fields.
///
/// # Safety
/// With a heap field, `bytes` is a live value of `layout` (a column's or a [`HeapBytes`]'); see
/// [`decode_field`]. A layout with none takes any bytes. [`HeapBytes::decode`] is the safe form.
pub unsafe fn decode_component(layout: &ComponentLayout, bytes: &[u8]) -> Value {
    let mut m = Map::new();
    for f in &layout.fields {
        // SAFETY: each field of a live value is live (this function's contract).
        m.insert(f.name.clone(), unsafe { decode_field(&f.ty, &bytes[f.offset..f.offset + f.ty.size()]) });
    }
    Value::Object(m)
}

/// The zero value of a field, as JSON: `""` for a string, `[]` for a list.
pub fn zero(ty: &FieldType) -> Value {
    match ty {
        FieldType::Str => json!(""),
        FieldType::List { .. } => json!([]),
        // SAFETY: not a heap type, so any bytes are a value.
        _ => unsafe { decode_field(ty, &vec![0u8; ty.size()]) },
    }
}

/// One field's value, encoded to write over a live component's field: bytes for a plain field, an
/// owned value for a heap field (`string`), which replaces the old one.
#[derive(Debug, PartialEq)]
pub enum FieldValue {
    Bytes(Vec<u8>),
    Heap(HeapValue),
}

impl FieldValue {
    /// Encodes `v` as a value of `ty`.
    pub fn encode(path: &str, ty: &FieldType, v: &Value) -> Result<FieldValue, ValueError> {
        if ty.is_heap() {
            return heap_value(path, ty, v).map(FieldValue::Heap);
        }
        let mut b = vec![0u8; ty.size()];
        encode_field(path, ty, v, &mut b)?;
        Ok(FieldValue::Bytes(b))
    }

    /// Whether the field at `p + offset` already holds this value.
    ///
    /// # Safety
    /// `p` is a live value of a component with this field at `offset`.
    pub unsafe fn same_at(&self, p: *const u8, offset: usize) -> bool {
        match self {
            // SAFETY: the field's bytes lie inside the value (the caller's contract).
            FieldValue::Bytes(b) => unsafe { std::slice::from_raw_parts(p.add(offset), b.len()) == b.as_slice() },
            // SAFETY: the slot holds a live value of its kind.
            FieldValue::Heap(h) => unsafe { crate::heap::read(p, offset, h.kind()) == *h },
        }
    }

    /// Writes the value over the field at `p + offset`; a heap value replaces (and frees) the old.
    ///
    /// # Safety
    /// As [`Self::same_at`], with write access.
    pub unsafe fn write_at(self, p: *mut u8, offset: usize) {
        match self {
            // SAFETY: as above.
            FieldValue::Bytes(b) => unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), p.add(offset), b.len()) },
            // SAFETY: as above.
            FieldValue::Heap(h) => unsafe { crate::heap::replace(p, offset, h) },
        }
    }
}

/// Encodes a map of field names to values into a component's native bytes, filling every field
/// the map leaves out with its declared default (zero if none). Unknown fields are errors. A
/// component with a heap field gets its header and owns its heap values.
pub fn encode_component(decl: &ComponentDecl, layout: &ComponentLayout, values: &Map<String, Value>) -> Result<HeapBytes, ValueError> {
    for key in values.keys() {
        if layout.field(key).is_none() {
            return Err(ValueError {
                path: format!(".{key}"),
                expected: "a declared field".into(),
                message: format!("{} has no field {key}", decl.name),
            });
        }
    }
    let mut out = HeapBytes::new(layout.size, HeapShape::of(&decl.name, layout));
    for f in &layout.fields {
        let path = format!(".{}", f.name);
        let Some(v) = values.get(&f.name).or_else(|| decl.field(&f.name).and_then(|d| d.default.as_ref())) else { continue };
        if f.ty.is_heap() {
            out.set(f.offset, heap_value(&path, &f.ty, v)?);
        } else {
            encode_field(&path, &f.ty, v, out.field_mut(f.offset, f.ty.size()))?;
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

/// Resolves `.field`, `.field.x|y|z|w`, `.field[i]` and a timer's or stopwatch's `.field.elapsed`.
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
            if let Some(sub) = target.ty.fields() {
                let f = sub.field(lane).ok_or_else(|| bad(format!("no field {lane} on {}", target.ty.ts_name())))?;
                target = PathTarget { offset: target.offset + f.offset, ty: f.ty.clone(), field: target.field };
                rest = r2;
                continue;
            }
            let lanes = target.ty.lanes().ok_or_else(|| bad(format!("{} has no lanes", target.ty.ts_name())))?;
            let i = ["x", "y", "z", "w"].iter().position(|x| *x == lane).filter(|i| *i < lanes);
            let i = i.ok_or_else(|| bad(format!("no lane {lane} on {}", target.ty.ts_name())))?;
            target = PathTarget { offset: target.offset + i * 4, ty: FieldType::F32, field: target.field };
            rest = r2;
        } else if let Some(r) = rest.strip_prefix('[') {
            let close = r.find(']').ok_or_else(|| bad("unclosed [".into()))?;
            let i: usize = r[..close].parse().map_err(|_| bad(format!("bad index {}", &r[..close])))?;
            if let FieldType::List { .. } = &target.ty {
                return Err(bad(format!("{0} is a list: write it whole, with the path .{0}", target.field)));
            }
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
            ..Default::default()
        }
    }

    #[test]
    fn encodes_a_partial_value_with_defaults() {
        let d = health();
        let bytes = encode_component(&d, &d.layout(), json!({"hp": 50}).as_object().unwrap()).unwrap().into_plain().unwrap();
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
    fn a_timer_is_an_object_of_its_fields() {
        let l = ComponentLayout::c_layout([("hp", &FieldType::F32), ("cooldown", &FieldType::Timer)]);
        let t = resolve_path(&l, ".cooldown.duration").unwrap();
        assert_eq!((t.offset, t.ty), (16, FieldType::F64));
        let mut b = [0u8; 24];
        encode_field(".cooldown", &FieldType::Timer, &json!({"duration": 2.0, "mode": "Repeating"}), &mut b).unwrap();
        let v = unsafe { decode_field(&FieldType::Timer, &b) };
        assert_eq!(v["duration"], json!(2.0));
        assert_eq!(v["mode"], json!("Repeating"));
        assert_eq!(v["finished"], json!(false));
        assert!(encode_field(".cooldown", &FieldType::Timer, &json!({"nope": 1}), &mut b).is_err());
    }

    #[test]
    fn a_string_field_round_trips_and_owns_its_value() {
        let d = ComponentDecl {
            name: "value::tests::Tag".into(),
            fields: vec![
                FieldDecl { name: "hp".into(), ty: FieldType::F32, default: None, hints: Default::default() },
                FieldDecl { name: "label".into(), ty: FieldType::Str, default: Some(json!("crate")), hints: Default::default() },
            ],
            ..Default::default()
        };
        let l = d.layout();
        let shape = HeapShape::of(&d.name, &l).unwrap();
        let a = encode_component(&d, &l, json!({"hp": 2}).as_object().unwrap()).unwrap();
        let b = encode_component(&d, &l, json!({"label": "door"}).as_object().unwrap()).unwrap();
        assert_eq!(a.decode(&l), json!({"hp": 2.0, "label": "crate"}));
        assert_eq!(b.decode(&l), json!({"hp": 0.0, "label": "door"}));
        assert_eq!(shape.live(), 2);
        drop((a, b));
        assert_eq!(shape.live(), 0);
        let e = encode_component(&d, &l, json!({"label": 3}).as_object().unwrap()).unwrap_err();
        assert_eq!((e.path.as_str(), e.expected.as_str()), (".label", "string"));
        assert_eq!(shape.live(), 0);
        assert_eq!(zero(&FieldType::Str), json!(""));
    }

    #[test]
    fn a_list_field_checks_its_items_and_is_written_whole() {
        let list = |of: FieldType| FieldType::List { of: Box::new(of) };
        let d = ComponentDecl {
            name: "value::tests::Scores".into(),
            fields: vec![
                FieldDecl { name: "scores".into(), ty: list(FieldType::U32), default: Some(json!([1, 2])), hints: Default::default() },
                FieldDecl { name: "names".into(), ty: list(FieldType::Str), default: None, hints: Default::default() },
            ],
            ..Default::default()
        };
        let l = d.layout();
        let b = encode_component(&d, &l, json!({"names": ["a", "b"]}).as_object().unwrap()).unwrap();
        assert_eq!(b.decode(&l), json!({"scores": [1, 2], "names": ["a", "b"]}));
        let e = encode_component(&d, &l, json!({"scores": [1, -1]}).as_object().unwrap()).unwrap_err();
        assert_eq!(e.path, ".scores[1]");
        assert!(resolve_path(&l, ".scores[0]").unwrap_err().message.contains("write it whole, with the path .scores"));
        assert_eq!(zero(&list(FieldType::F32)), json!([]));
    }

    #[test]
    fn round_trip() {
        let ty = FieldType::Enum { variants: vec!["idle".into(), "run".into()], repr: None };
        let mut b = [0u8; 1];
        encode_field(".s", &ty, &json!("run"), &mut b).unwrap();
        assert_eq!(unsafe { decode_field(&ty, &b) }, json!("run"));
        let mut v = [0u8; 12];
        encode_field(".p", &FieldType::Vec3, &json!({"x":1,"y":2,"z":3}), &mut v).unwrap();
        assert_eq!(unsafe { decode_field(&FieldType::Vec3, &v) }, json!([1.0, 2.0, 3.0]));
    }
}
