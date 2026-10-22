//! A material's params as one WGSL uniform struct: offsets by WGSL's rules (not bridge's C
//! rules, which components keep), the WGSL declarations appended to the author's file, and
//! param values encoded into the struct's bytes by name.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The bind group every material uses (`MATERIAL_BIND_GROUP_INDEX` in bevy_pbr).
pub const MATERIAL_GROUP: u32 = 3;

/// A param's element type, by the names `bevy-ts/material` exports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Base {
    F32,
    I32,
    U32,
    Vec2f,
    Vec3f,
    Vec4f,
    #[serde(rename = "mat4x4f")]
    Mat4x4f,
    /// A `vec4<f32>` in linear RGBA, written from a hex string or sRGB components.
    Color,
}

/// A param's type: an element, or a fixed array of them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamType {
    Base(Base),
    Array { array: Base, len: u32 },
}

impl Base {
    fn align(self) -> usize {
        match self {
            Base::F32 | Base::I32 | Base::U32 => 4,
            Base::Vec2f => 8,
            Base::Vec3f | Base::Vec4f | Base::Color | Base::Mat4x4f => 16,
        }
    }

    fn size(self) -> usize {
        match self {
            Base::F32 | Base::I32 | Base::U32 => 4,
            Base::Vec2f => 8,
            Base::Vec3f => 12,
            Base::Vec4f | Base::Color => 16,
            Base::Mat4x4f => 64,
        }
    }

    /// Floats (or ints) the value holds.
    fn lanes(self) -> usize {
        match self {
            Base::F32 | Base::I32 | Base::U32 => 1,
            Base::Vec2f => 2,
            Base::Vec3f => 3,
            Base::Vec4f | Base::Color => 4,
            Base::Mat4x4f => 16,
        }
    }

    fn wgsl(self) -> &'static str {
        match self {
            Base::F32 => "f32",
            Base::I32 => "i32",
            Base::U32 => "u32",
            Base::Vec2f => "vec2<f32>",
            Base::Vec3f => "vec3<f32>",
            Base::Vec4f | Base::Color => "vec4<f32>",
            Base::Mat4x4f => "mat4x4<f32>",
        }
    }

    /// An array element's WGSL type. Uniform arrays are strided to 16, which WGSL allows only
    /// for elements of 16-byte alignment, so scalars and `vec2f` widen to a `vec4` (read `.x`).
    fn array_element_wgsl(self) -> &'static str {
        match self {
            Base::F32 | Base::Vec2f => "vec4<f32>",
            Base::I32 => "vec4<i32>",
            Base::U32 => "vec4<u32>",
            other => other.wgsl(),
        }
    }
}

fn round_up(n: usize, to: usize) -> usize {
    n.div_ceil(to) * to
}

impl ParamType {
    pub fn align(&self) -> usize {
        match self {
            ParamType::Base(b) => b.align(),
            ParamType::Array { .. } => 16,
        }
    }

    pub fn size(&self) -> usize {
        match self {
            ParamType::Base(b) => b.size(),
            ParamType::Array { array, len } => Self::stride(*array) * *len as usize,
        }
    }

    /// An array's element stride: its size rounded to 16.
    pub fn stride(element: Base) -> usize {
        round_up(element.size(), 16)
    }

    pub fn wgsl(&self) -> String {
        match self {
            ParamType::Base(b) => b.wgsl().into(),
            ParamType::Array { array, len } => format!("array<{}, {len}>", array.array_element_wgsl()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamField {
    pub name: String,
    pub ty: ParamType,
    pub offset: usize,
}

/// A material's params laid out as one WGSL uniform struct.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ParamsLayout {
    pub fields: Vec<ParamField>,
    /// The uniform buffer's size: the end of the last field rounded to 16.
    pub size: usize,
}

impl ParamsLayout {
    /// Lays out `params` in declaration order by WGSL's uniform rules.
    pub fn new<'a>(params: impl IntoIterator<Item = (&'a str, &'a ParamType)>) -> Result<Self, String> {
        let mut fields = Vec::new();
        let mut end = 0;
        for (name, ty) in params {
            check_name("param", name)?;
            if let ParamType::Array { len: 0, .. } = ty {
                return Err(format!("param {name}: an array needs at least one element"));
            }
            let offset = round_up(end, ty.align());
            end = offset + ty.size();
            fields.push(ParamField { name: name.into(), ty: ty.clone(), offset });
        }
        Ok(ParamsLayout { fields, size: round_up(end, 16) })
    }

    pub fn field(&self, name: &str) -> Option<&ParamField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Default bytes: zeros, with colors (and color array elements) white.
    pub fn defaults(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; self.size];
        let white = |bytes: &mut Vec<u8>, at: usize| (0..4).for_each(|i| put(bytes, at + 4 * i, 1f32.to_le_bytes()));
        for f in &self.fields {
            match &f.ty {
                ParamType::Base(Base::Color) => white(&mut bytes, f.offset),
                ParamType::Array { array: Base::Color, len } => {
                    (0..*len as usize).for_each(|i| white(&mut bytes, f.offset + i * ParamType::stride(Base::Color)));
                }
                _ => {}
            }
        }
        bytes
    }

    /// Writes one param's value into `bytes`.
    pub fn encode(&self, name: &str, value: &Value, bytes: &mut [u8]) -> Result<(), String> {
        let f = self.field(name).ok_or_else(|| format!("no param {name}"))?;
        match &f.ty {
            ParamType::Base(b) => encode_base(name, *b, value, bytes, f.offset),
            ParamType::Array { array, len } => {
                let items = value.as_array().ok_or_else(|| format!("{name} is an array of {len}"))?;
                if items.len() != *len as usize {
                    return Err(format!("{name} has {len} elements, not {}", items.len()));
                }
                let stride = ParamType::stride(*array);
                for (i, v) in items.iter().enumerate() {
                    encode_base(&format!("{name}[{i}]"), *array, v, bytes, f.offset + i * stride)?;
                }
                Ok(())
            }
        }
    }
}

/// WGSL keywords and reserved words: no name the generated code declares may be one.
const RESERVED: &[&str] = &[
    "alias", "break", "case", "const", "const_assert", "continue", "continuing", "default", "diagnostic", "discard", "else", "enable",
    "false", "fn", "for", "if", "let", "loop", "override", "requires", "return", "struct", "switch", "true", "var", "while", "NULL",
    "Self", "abstract", "active", "alignas", "alignof", "as", "asm", "asm_fragment", "async", "attribute", "auto", "await", "become",
    "cast", "catch", "class", "co_await", "co_return", "co_yield", "coherent", "column_major", "common", "compile",
    "compile_fragment", "concept", "const_cast", "consteval", "constexpr", "constinit", "crate", "debugger", "decltype", "delete",
    "demote", "demote_to_helper", "do", "dynamic_cast", "enum", "explicit", "export", "extends", "extern", "external",
    "fallthrough", "filter", "final", "finally", "friend", "from", "fxgroup", "get", "goto", "groupshared", "highp", "impl",
    "implements", "import", "inline", "instanceof", "interface", "layout", "lowp", "macro", "macro_rules", "match", "mediump",
    "meta", "mod", "module", "move", "mut", "mutable", "namespace", "new", "nil", "noexcept", "noinline", "nointerpolation",
    "noperspective", "null", "nullptr", "of", "operator", "package", "packoffset", "partition", "pass", "patch", "pixelfragment",
    "precise", "precision", "premerge", "priv", "protected", "pub", "public", "readonly", "ref", "regardless", "register",
    "reinterpret_cast", "require", "resource", "restrict", "self", "set", "shared", "sizeof", "smooth", "snorm", "static",
    "static_assert", "static_cast", "std", "subroutine", "super", "target", "template", "this", "thread_local", "throw", "trait",
    "try", "type", "typedef", "typeid", "typename", "typeof", "union", "unless", "unorm", "unsafe", "unsized", "use", "using",
    "varying", "virtual", "volatile", "wgsl", "where", "with", "writeonly", "yield",
];

/// Predeclared WGSL types and type aliases a module-scope name may not take.
const PREDECLARED: &[&str] = &[
    "bool", "f16", "f32", "i32", "u32", "vec2", "vec3", "vec4", "vec2f", "vec3f", "vec4f", "vec2i", "vec3i", "vec4i", "vec2u",
    "vec3u", "vec4u", "vec2h", "vec3h", "vec4h", "mat2x2", "mat2x3", "mat2x4", "mat3x2", "mat3x3", "mat3x4", "mat4x2", "mat4x3",
    "mat4x4", "mat2x2f", "mat2x3f", "mat2x4f", "mat3x2f", "mat3x3f", "mat3x4f", "mat4x2f", "mat4x3f", "mat4x4f", "atomic",
    "array", "ptr", "sampler", "sampler_comparison",
];

/// Props `<material kind>` takes itself, so no param or texture may be named one.
pub const RESERVED_PROPS: &[&str] = &["kind", "gltfMaterial"];

/// A name the generated WGSL declares: an identifier, not reserved, not starting with `__`, and
/// not a prop `<material>` takes itself.
fn check_name(what: &str, name: &str) -> Result<(), String> {
    if RESERVED_PROPS.contains(&name) {
        return Err(format!("{what} {name}: <material> takes {name} itself"));
    }
    if !is_ident(name) {
        return Err(format!("{what} {name:?} is not a WGSL identifier"));
    }
    if name.starts_with("__") {
        return Err(format!("{what} {name}: WGSL names may not start with __"));
    }
    if RESERVED.contains(&name) {
        return Err(format!("{what} {name} is a WGSL reserved word"));
    }
    Ok(())
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && c.all(|c| c.is_ascii_alphanumeric() || c == '_') && s != "_"
}

fn put(bytes: &mut [u8], at: usize, v: [u8; 4]) {
    bytes[at..at + 4].copy_from_slice(&v);
}

fn encode_base(name: &str, b: Base, value: &Value, bytes: &mut [u8], at: usize) -> Result<(), String> {
    if b == Base::Color {
        let c = color(value).map_err(|e| format!("{name}: {e}"))?;
        for (i, v) in c.iter().enumerate() {
            put(bytes, at + 4 * i, v.to_le_bytes());
        }
        return Ok(());
    }
    let lanes: Vec<&Value> = match value {
        Value::Array(a) => a.iter().collect(),
        v if b.lanes() == 1 => vec![v],
        _ => return Err(format!("{name} takes {} numbers", b.lanes())),
    };
    if lanes.len() != b.lanes() {
        return Err(format!("{name} takes {} numbers, not {}", b.lanes(), lanes.len()));
    }
    for (i, v) in lanes.into_iter().enumerate() {
        let n = v.as_f64().ok_or_else(|| format!("{name} takes numbers, not {v}"))?;
        let word = match b {
            Base::I32 if n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64 => (n as i32).to_le_bytes(),
            Base::U32 if n.fract() == 0.0 && n >= 0.0 && n <= u32::MAX as f64 => (n as u32).to_le_bytes(),
            Base::I32 | Base::U32 => return Err(format!("{name} takes an integer in range, not {n}")),
            _ => (n as f32).to_le_bytes(),
        };
        put(bytes, at + 4 * i, word);
    }
    Ok(())
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// A color as linear RGBA: `"#f80"`, `"#ff8800"`, `"#ff8800cc"`, or `[r, g, b, a?]` in sRGB.
pub fn color(value: &Value) -> Result<[f32; 4], String> {
    let srgb: [f32; 4] = match value {
        Value::String(s) => {
            let c = bevy::color::Srgba::hex(s).map_err(|e| format!("color {s:?}: {e}"))?;
            [c.red, c.green, c.blue, c.alpha]
        }
        Value::Array(a) if a.len() == 3 || a.len() == 4 => {
            let mut c = [1.0; 4];
            for (i, v) in a.iter().enumerate() {
                c[i] = v.as_f64().ok_or("a color's components are numbers")? as f32;
            }
            c
        }
        _ => return Err(format!("a color is \"#rrggbb\" or [r, g, b, a], not {value}")),
    };
    Ok([srgb_to_linear(srgb[0]), srgb_to_linear(srgb[1]), srgb_to_linear(srgb[2]), srgb[3]])
}

/// What a material binds, in binding order: the params struct (if any), then each texture and
/// its sampler.
#[derive(Clone, Debug, PartialEq)]
pub struct Bindings {
    /// The struct's WGSL name: the material name's last segment plus `Params`.
    struct_name: String,
    params: ParamsLayout,
    textures: Vec<String>,
}

impl Bindings {
    /// Lays out `params` and checks `textures` against them: every name a WGSL identifier that is
    /// not reserved, and no two names among the params, `params`, the struct and each texture and
    /// its `<name>_sampler` the same (params and textures share `<material>`'s props).
    pub fn new<'a>(material: &str, params: impl IntoIterator<Item = (&'a str, &'a ParamType)>, textures: &[String]) -> Result<Self, String> {
        let params = ParamsLayout::new(params)?;
        let struct_name = struct_name(material);
        if struct_name.starts_with("__") {
            return Err(format!("its params struct would be {struct_name}; WGSL names may not start with __"));
        }
        let mut names: std::collections::HashSet<String> = params.fields.iter().map(|f| f.name.clone()).collect();
        names.extend(["params".to_string(), struct_name.clone()]);
        for t in textures {
            check_name("texture", t)?;
            // A texture is a module-scope declaration: it may not shadow a predeclared type.
            if PREDECLARED.contains(&t.as_str()) || t.starts_with("texture_") {
                return Err(format!("texture {t} is a predeclared WGSL name"));
            }
            for n in [t.clone(), format!("{t}_sampler")] {
                if !names.insert(n.clone()) {
                    return Err(format!("texture {t} declares {n}, which a param or another declaration already uses"));
                }
            }
        }
        Ok(Bindings { struct_name, params, textures: textures.to_vec() })
    }

    pub fn struct_name(&self) -> &str {
        &self.struct_name
    }

    pub fn params(&self) -> &ParamsLayout {
        &self.params
    }

    pub fn textures(&self) -> &[String] {
        &self.textures
    }

    pub fn params_binding(&self) -> Option<u32> {
        (!self.params.fields.is_empty()).then_some(0)
    }

    /// A texture's binding; its sampler's is one more.
    pub fn texture_binding(&self, i: usize) -> u32 {
        self.params_binding().map_or(0, |_| 1) + 2 * i as u32
    }

    /// The WGSL declarations of the struct and every binding. Appended after the author's last
    /// line (WGSL declarations are order-free and WESL imports must come first), so the author's
    /// lines keep their numbers.
    pub fn wgsl(&self, material: &str) -> String {
        let mut s = format!("\n// bevy-ts: the params and textures of {material}, generated from its TS schema.\n");
        if let Some(b) = self.params_binding() {
            s.push_str(&format!("struct {} {{\n", self.struct_name));
            for f in &self.params.fields {
                s.push_str(&format!("    {}: {},\n", f.name, f.ty.wgsl()));
            }
            s.push_str("}\n");
            s.push_str(&format!("@group({MATERIAL_GROUP}) @binding({b}) var<uniform> params: {};\n", self.struct_name));
        }
        for (i, t) in self.textures.iter().enumerate() {
            let b = self.texture_binding(i);
            s.push_str(&format!("@group({MATERIAL_GROUP}) @binding({b}) var {t}: texture_2d<f32>;\n"));
            s.push_str(&format!("@group({MATERIAL_GROUP}) @binding({}) var {t}_sampler: sampler;\n", b + 1));
        }
        s
    }
}

/// `game::Glow` → `GlowParams`.
pub fn struct_name(material: &str) -> String {
    let last = material.rsplit("::").next().unwrap_or(material);
    let mut s: String = last.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, 'M');
    }
    s + "Params"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layout(params: &[(&str, ParamType)]) -> ParamsLayout {
        ParamsLayout::new(params.iter().map(|(n, t)| (*n, t))).unwrap()
    }

    fn offsets(l: &ParamsLayout) -> Vec<usize> {
        l.fields.iter().map(|f| f.offset).collect()
    }

    use Base::*;
    use ParamType::Base as B;

    #[test]
    fn glow_is_32_bytes() {
        let l = layout(&[("color", B(Color)), ("pulse", B(F32))]);
        assert_eq!(offsets(&l), [0, 16]);
        assert_eq!(l.size, 32);
    }

    #[test]
    fn vec3f_aligns_to_16_and_a_scalar_packs_after_it() {
        let l = layout(&[("a", B(F32)), ("v", B(Vec3f)), ("b", B(F32)), ("c", B(Vec2f))]);
        assert_eq!(offsets(&l), [0, 16, 28, 32]);
        assert_eq!(l.size, 48);
    }

    #[test]
    fn arrays_stride_to_16() {
        let l = layout(&[("w", ParamType::Array { array: F32, len: 3 }), ("x", B(F32)), ("v", ParamType::Array { array: Vec3f, len: 2 })]);
        assert_eq!(offsets(&l), [0, 48, 64]);
        assert_eq!(l.fields[0].ty.size(), 48);
        assert_eq!(l.size, 96);
        assert_eq!(l.fields[0].ty.wgsl(), "array<vec4<f32>, 3>");
        assert_eq!(l.fields[2].ty.wgsl(), "array<vec3<f32>, 2>");
    }

    #[test]
    fn mat4_is_64_aligned_16() {
        let l = layout(&[("a", B(F32)), ("m", B(Mat4x4f)), ("b", B(U32))]);
        assert_eq!(offsets(&l), [0, 16, 80]);
        assert_eq!(l.size, 96);
    }

    #[test]
    fn struct_size_rounds_to_16() {
        assert_eq!(layout(&[("a", B(F32))]).size, 16);
        assert_eq!(layout(&[]).size, 0);
    }

    #[test]
    fn encodes_by_name() {
        let l = layout(&[("color", B(Color)), ("pulse", B(F32)), ("w", ParamType::Array { array: I32, len: 2 })]);
        let mut b = l.defaults();
        assert_eq!(f32::from_le_bytes(b[0..4].try_into().unwrap()), 1.0);
        l.encode("pulse", &json!(2), &mut b).unwrap();
        l.encode("color", &json!("#000000"), &mut b).unwrap();
        l.encode("w", &json!([-1, 7]), &mut b).unwrap();
        assert_eq!(f32::from_le_bytes(b[16..20].try_into().unwrap()), 2.0);
        assert_eq!(f32::from_le_bytes(b[0..4].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(b[12..16].try_into().unwrap()), 1.0);
        assert_eq!(i32::from_le_bytes(b[32..36].try_into().unwrap()), -1);
        assert_eq!(i32::from_le_bytes(b[48..52].try_into().unwrap()), 7);
        assert!(l.encode("w", &json!([1.5, 2]), &mut b).unwrap_err().contains("integer"));
        assert!(l.encode("pulse", &json!("x"), &mut b).is_err());
        assert!(l.encode("speed", &json!(1), &mut b).unwrap_err().contains("no param speed"));
    }

    #[test]
    fn color_array_elements_default_white_like_a_color() {
        let l = layout(&[("c", B(Color)), ("cs", ParamType::Array { array: Color, len: 2 })]);
        let b = l.defaults();
        let at = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        assert_eq!([at(0), at(12)], [1.0, 1.0]);
        assert_eq!([at(16), at(28), at(32), at(44)], [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn texture_names_are_identifiers_that_do_not_clash() {
        let p = [("pulse", B(F32))];
        let new = |t: &[&str]| Bindings::new("game::Glow", p.iter().map(|(n, t)| (*n, t)), &t.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert!(new(&["noise"]).is_ok());
        assert!(new(&["noise-map"]).unwrap_err().contains("not a WGSL identifier"));
        assert!(new(&["params"]).unwrap_err().contains("params"));
        assert!(new(&["pulse"]).unwrap_err().contains("pulse"));
        assert!(new(&["a", "a_sampler"]).unwrap_err().contains("a_sampler"));
        assert!(new(&["GlowParams"]).unwrap_err().contains("GlowParams"));
        assert!(new(&["loop"]).unwrap_err().contains("reserved"));
        assert!(new(&["__x"]).unwrap_err().contains("__"));
        assert!(new(&["sampler"]).unwrap_err().contains("predeclared"));
        assert!(new(&["texture_2d"]).unwrap_err().contains("predeclared"));
        assert!(Bindings::new("game::__Glow", [], &[]).unwrap_err().contains("__GlowParams"));
        // The spec's scenario: a param named like a texture's sampler.
        let p = [("noise_sampler", B(F32))];
        let e = Bindings::new("game::Glow", p.iter().map(|(n, t)| (*n, t)), &["noise".into()]).unwrap_err();
        assert!(e.contains("noise_sampler"), "{e}");
        assert!(ParamsLayout::new([("loop", &B(F32))]).unwrap_err().contains("reserved"));
        assert!(ParamsLayout::new([("gltfMaterial", &B(F32))]).unwrap_err().contains("<material> takes gltfMaterial itself"));
        assert!(new(&["kind"]).unwrap_err().contains("<material> takes kind itself"));
    }

    #[test]
    fn hex_colors_are_linear() {
        let c = color(&json!("#ff8800")).unwrap();
        assert_eq!(c[0], 1.0);
        assert!((c[1] - 0.246).abs() < 0.01, "{c:?}");
        assert_eq!(c[3], 1.0);
    }

    #[test]
    fn generated_bindings() {
        let p = [("color", B(Color)), ("pulse", B(F32))];
        let b = Bindings::new("game::Glow", p.iter().map(|(n, t)| (*n, t)), &["noise".into()]).unwrap();
        let w = b.wgsl("game::Glow");
        assert!(w.contains("struct GlowParams {\n    color: vec4<f32>,\n    pulse: f32,\n}"), "{w}");
        assert!(w.contains("@group(3) @binding(0) var<uniform> params: GlowParams;"), "{w}");
        assert!(w.contains("@group(3) @binding(1) var noise: texture_2d<f32>;"), "{w}");
        assert!(w.contains("@group(3) @binding(2) var noise_sampler: sampler;"), "{w}");
        assert_eq!(b.params.size, 32);
    }

    #[test]
    fn no_params_means_textures_from_binding_0() {
        let b = Bindings::new("t", [], &["a".into()]).unwrap();
        assert_eq!(b.texture_binding(0), 0);
        assert!(!b.wgsl("t").contains("struct"));
    }

    #[test]
    fn schema_json() {
        let t: ParamType = serde_json::from_value(json!("vec3f")).unwrap();
        assert_eq!(t, B(Vec3f));
        let t: ParamType = serde_json::from_value(json!({ "array": "mat4x4f", "len": 2 })).unwrap();
        assert_eq!(t, ParamType::Array { array: Mat4x4f, len: 2 });
        assert!(ParamsLayout::new([("not ok", &B(F32))]).is_err());
    }
}
