//! A material's params as one WGSL uniform struct: offsets by WGSL's rules (not bridge's C
//! rules, which components keep), the WGSL declarations appended to the author's file, and
//! param values encoded into the struct's bytes by name.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The bind group every 3D material uses (`MATERIAL_BIND_GROUP_INDEX` in bevy_pbr).
pub const MATERIAL_GROUP: u32 = 3;

/// The bind group every 2D material uses (`MATERIAL_2D_BIND_GROUP_INDEX` in bevy_sprite_render).
pub const MATERIAL_2D_GROUP: u32 = 2;

/// Which pipeline a kind draws with: Bevy's 3D mesh materials or its 2D ones. It sets the bind
/// group of the generated declarations and where `get_tag` is imported from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Space {
    #[serde(rename = "2d")]
    D2,
    #[default]
    #[serde(rename = "3d")]
    D3,
}

impl Space {
    pub fn group(self) -> u32 {
        match self {
            Space::D2 => MATERIAL_2D_GROUP,
            Space::D3 => MATERIAL_GROUP,
        }
    }

    /// The import prepended to a kind with instance params, on the author's first line.
    pub fn instance_import(self) -> &'static str {
        match self {
            Space::D2 => INSTANCE_IMPORT_2D,
            Space::D3 => INSTANCE_IMPORT,
        }
    }
}

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

    /// One param's value read back from `bytes`, the inverse of [`encode`](Self::encode): numbers,
    /// number arrays, and colours as `{ LinearRgba: {...} }`, so encoding it again gives the same
    /// bytes.
    pub fn decode(&self, name: &str, bytes: &[u8]) -> Option<Value> {
        let f = self.field(name)?;
        Some(match &f.ty {
            ParamType::Base(b) => decode_base(*b, bytes, f.offset),
            ParamType::Array { array, len } => {
                let stride = ParamType::stride(*array);
                Value::Array((0..*len as usize).map(|i| decode_base(*array, bytes, f.offset + i * stride)).collect())
            }
        })
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
pub const RESERVED_PROPS: &[&str] = &["kind", "asset", "gltfMaterial"];

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

fn word(bytes: &[u8], at: usize) -> [u8; 4] {
    bytes[at..at + 4].try_into().expect("4 bytes")
}

fn decode_base(b: Base, bytes: &[u8], at: usize) -> Value {
    let lane = |i: usize| -> Value {
        let w = word(bytes, at + 4 * i);
        match b {
            Base::I32 => i32::from_le_bytes(w).into(),
            Base::U32 => u32::from_le_bytes(w).into(),
            _ => f64::from(f32::from_le_bytes(w)).into(),
        }
    };
    match b {
        Base::Color => {
            let c: Vec<Value> = (0..4).map(lane).collect();
            serde_json::json!({ "LinearRgba": { "red": c[0], "green": c[1], "blue": c[2], "alpha": c[3] } })
        }
        b if b.lanes() == 1 => lane(0),
        b => Value::Array((0..b.lanes()).map(lane).collect()),
    }
}

/// A color as linear RGBA, in any form `bevy_ts::color` takes (`"#f80"`, `[r, g, b, a?]` in sRGB,
/// `{ Hsla: {...} }`, ...).
pub fn color(value: &Value) -> Result<[f32; 4], String> {
    use bevy::color::ColorToComponents;
    Ok(bevy_ts::color::parse(value)?.to_linear().to_f32_array())
}

/// Bytes of instance params one entity holds: `ScriptInstance`'s size.
pub const INSTANCE_BYTES: usize = 256;

/// The name the prepended import gives Bevy's `mesh_functions::get_tag`.
pub const GET_TAG: &str = "bevy_ts_get_tag";

/// The prepended import, on the author's first line so their lines keep their numbers: WESL
/// imports come before every declaration, so it cannot be appended.
pub const INSTANCE_IMPORT: &str = "import bevy_pbr::render::mesh_functions::get_tag as bevy_ts_get_tag; ";

/// [`INSTANCE_IMPORT`] for a 2D kind: Bevy's 2D `get_tag`.
pub const INSTANCE_IMPORT_2D: &str = "import bevy_sprite_render::mesh2d::functions::get_tag as bevy_ts_get_tag; ";

/// The instance struct's padding field, which fills it to [`INSTANCE_BYTES`].
const PAD: &str = "bevy_ts_pad";

/// What a material binds, in binding order: the params struct (if any), then each texture and
/// its sampler, then the instance array (if the kind has instance params).
#[derive(Clone, Debug, PartialEq)]
pub struct Bindings {
    /// The struct's WGSL name: the material name's last segment plus `Params`.
    struct_name: String,
    params: ParamsLayout,
    textures: Vec<String>,
    /// Per-entity params, laid out as params are, in a storage array indexed by `MeshTag`.
    instance: ParamsLayout,
    space: Space,
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
        Ok(Bindings { struct_name, params, textures: textures.to_vec(), instance: ParamsLayout::default(), space: Space::D3 })
    }

    /// Adds per-entity `instance` params: at most [`INSTANCE_BYTES`], and no name equal to a
    /// param, texture or sampler (they share `<material>`'s props), `instance`, `instances` or
    /// the instance struct.
    pub fn with_instance<'a>(mut self, instance: impl IntoIterator<Item = (&'a str, &'a ParamType)>) -> Result<Self, String> {
        let layout = ParamsLayout::new(instance).map_err(|e| e.replacen("param ", "instance param ", 1))?;
        if layout.fields.is_empty() {
            return Ok(self);
        }
        if layout.size > INSTANCE_BYTES {
            return Err(format!("its instance params take {} bytes; at most {INSTANCE_BYTES} fit", layout.size));
        }
        let struct_name = self.instance_struct();
        let mut taken: Vec<String> = self.params.fields.iter().map(|f| f.name.clone()).collect();
        for t in &self.textures {
            taken.extend([t.clone(), format!("{t}_sampler")]);
        }
        for f in &layout.fields {
            let n = f.name.as_str();
            if taken.iter().any(|t| t == n) {
                return Err(format!("instance param {n}: a param or texture already uses the name"));
            }
            if ["instance", "instances", PAD, GET_TAG, struct_name.as_str()].contains(&n) {
                return Err(format!("instance param {n}: the generated instance code declares {n}"));
            }
        }
        self.instance = layout;
        Ok(self)
    }

    pub fn instance(&self) -> &ParamsLayout {
        &self.instance
    }

    /// The same bindings for a kind of `space`: 3D unless set.
    pub fn in_space(mut self, space: Space) -> Self {
        self.space = space;
        self
    }

    pub fn space(&self) -> Space {
        self.space
    }

    /// Whether the kind has instance params, and so its entities a `ScriptInstance`.
    pub fn has_instance(&self) -> bool {
        !self.instance.fields.is_empty()
    }

    /// `GlowParams` → `GlowInstance`.
    pub fn instance_struct(&self) -> String {
        format!("{}Instance", self.struct_name.strip_suffix("Params").unwrap_or(&self.struct_name))
    }

    /// The instance array's binding, after the textures.
    pub fn instance_binding(&self) -> Option<u32> {
        self.has_instance().then(|| self.texture_binding(self.textures.len()))
    }

    /// The names the generated code declares at module scope.
    pub fn generated_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if self.params_binding().is_some() {
            names.extend(["params".to_string(), self.struct_name.clone()]);
        }
        for t in &self.textures {
            names.push(t.clone());
            names.push(format!("{t}_sampler"));
        }
        if self.has_instance() {
            names.extend([self.instance_struct(), "instances".into(), "instance".into(), GET_TAG.into()]);
        }
        names
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
        let g = self.space.group();
        let mut s = format!("\n// bevy-ts: the params and textures of {material}, generated from its TS schema.\n");
        if let Some(b) = self.params_binding() {
            s.push_str(&format!("struct {} {{\n", self.struct_name));
            for f in &self.params.fields {
                s.push_str(&format!("    {}: {},\n", f.name, f.ty.wgsl()));
            }
            s.push_str("}\n");
            s.push_str(&format!("@group({g}) @binding({b}) var<uniform> params: {};\n", self.struct_name));
        }
        for (i, t) in self.textures.iter().enumerate() {
            let b = self.texture_binding(i);
            s.push_str(&format!("@group({g}) @binding({b}) var {t}: texture_2d<f32>;\n"));
            s.push_str(&format!("@group({g}) @binding({}) var {t}_sampler: sampler;\n", b + 1));
        }
        if let Some(b) = self.instance_binding() {
            let name = self.instance_struct();
            s.push_str(&format!("struct {name} {{\n"));
            for f in &self.instance.fields {
                s.push_str(&format!("    {}: {},\n", f.name, f.ty.wgsl()));
            }
            // Filled to `INSTANCE_BYTES`, the stride of `ScriptInstance` in the buffer.
            let end = self.instance.fields.iter().map(|f| f.offset + f.ty.size()).max().unwrap_or(0);
            if end < INSTANCE_BYTES {
                s.push_str(&format!("    {PAD}: array<u32, {}>,\n", (INSTANCE_BYTES - end) / 4));
            }
            s.push_str("}\n");
            s.push_str(&format!("@group({g}) @binding({b}) var<storage, read> instances: array<{name}>;\n"));
            s.push_str(&format!("fn instance(i: u32) -> {name} {{\n    return instances[{GET_TAG}(i)];\n}}\n"));
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

/// Bytes of a pass's params struct, frame fields included: `ScriptPass`'s size.
pub const PASS_BYTES: usize = 256;

/// The fields the engine writes into every pass's params each frame, first in its struct.
pub const FRAME_FIELDS: [(&str, Base); 4] = [("time", Base::F32), ("texel", Base::F32), ("size", Base::Vec2f), ("view", Base::Vec2f)];

/// Bytes the frame fields take, from offset 0.
pub const FRAME_BYTES: usize = 24;

/// A pass's params as one WGSL uniform struct: the frame fields at 0, 4, 8 and 16, then the
/// kind's params by WGSL's uniform rules, at most [`PASS_BYTES`]. Binds the screen at 0, its
/// sampler at 1 and the struct at 2, all in group 0, as Bevy's `FullscreenMaterial` lays them out.
#[derive(Clone, Debug, PartialEq)]
pub struct PassLayout {
    struct_name: String,
    params: ParamsLayout,
}

impl PassLayout {
    pub fn new<'a>(pass: &str, params: impl IntoIterator<Item = (&'a str, &'a ParamType)>) -> Result<Self, String> {
        let struct_name = struct_name(pass);
        if struct_name.starts_with("__") {
            return Err(format!("its params struct would be {struct_name}; WGSL names may not start with __"));
        }
        let params: Vec<(&str, &ParamType)> = params.into_iter().collect();
        for (n, _) in &params {
            if FRAME_FIELDS.iter().any(|(f, _)| f == n) {
                return Err(format!("param {n}: the engine writes {n} every frame; name it something else"));
            }
            if ["screen", "screen_sampler", "params", struct_name.as_str()].contains(n) {
                return Err(format!("param {n}: the generated pass code declares {n}"));
            }
            if *n == "kind" {
                return Err("param kind: <pass> takes kind itself".into());
            }
        }
        let frame: Vec<(&str, ParamType)> = FRAME_FIELDS.iter().map(|(n, b)| (*n, ParamType::Base(*b))).collect();
        let layout = ParamsLayout::new(frame.iter().map(|(n, t)| (*n, t)).chain(params))?;
        if layout.size > PASS_BYTES {
            return Err(format!("its params take {} bytes with the frame fields; at most {PASS_BYTES} fit", layout.size));
        }
        Ok(PassLayout { struct_name, params: layout })
    }

    /// The whole struct, frame fields first.
    pub fn params(&self) -> &ParamsLayout {
        &self.params
    }

    /// The kind's own params, without the frame fields.
    pub fn own(&self) -> impl Iterator<Item = &ParamField> {
        self.params.fields.iter().skip(FRAME_FIELDS.len())
    }

    /// Whether `name` is one of the kind's own params.
    pub fn has(&self, name: &str) -> bool {
        self.own().any(|f| f.name == name)
    }

    pub fn struct_name(&self) -> &str {
        &self.struct_name
    }

    /// The names the generated code declares at module scope.
    pub fn generated_names(&self) -> Vec<String> {
        ["screen", "screen_sampler", "params", self.struct_name.as_str()].map(String::from).to_vec()
    }

    /// The struct and the bindings, appended after the author's last line as a material's are.
    pub fn wgsl(&self, pass: &str) -> String {
        let mut s = format!("\n// bevy-ts: the params and screen of {pass}, generated from its TS schema.\n");
        s.push_str(&format!("struct {} {{\n", self.struct_name));
        for f in &self.params.fields {
            s.push_str(&format!("    {}: {},\n", f.name, f.ty.wgsl()));
        }
        s.push_str("}\n");
        s.push_str("@group(0) @binding(0) var screen: texture_2d<f32>;\n");
        s.push_str("@group(0) @binding(1) var screen_sampler: sampler;\n");
        s.push_str(&format!("@group(0) @binding(2) var<uniform> params: {};\n", self.struct_name));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_pass_puts_the_frame_fields_first_then_its_params() {
        let p = [("amount", B(F32)), ("tint", B(Color))];
        let l = PassLayout::new("game::Dusk", p.iter().map(|(n, t)| (*n, t))).unwrap();
        assert_eq!(offsets(l.params()), [0, 4, 8, 16, 24, 32]);
        assert_eq!(l.params().size, 48);
        assert_eq!(l.own().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["amount", "tint"]);
        let w = l.wgsl("game::Dusk");
        assert!(w.contains("struct DuskParams {\n    time: f32,\n    texel: f32,\n    size: vec2<f32>,\n    view: vec2<f32>,\n    amount: f32,\n    tint: vec4<f32>,\n}"), "{w}");
        assert!(w.contains("@group(0) @binding(0) var screen: texture_2d<f32>;"), "{w}");
        assert!(w.contains("@group(0) @binding(1) var screen_sampler: sampler;"), "{w}");
        assert!(w.contains("@group(0) @binding(2) var<uniform> params: DuskParams;"), "{w}");
        assert_eq!(PassLayout::new("game::Dusk", []).unwrap().params().size, 32);
    }

    #[test]
    fn a_pass_refuses_reserved_names_and_more_than_256_bytes() {
        let new = |p: &[(&str, ParamType)]| PassLayout::new("game::Dusk", p.iter().map(|(n, t)| (*n, t)));
        for f in ["time", "texel", "size", "view"] {
            assert!(new(&[(f, B(F32))]).unwrap_err().contains("the engine writes"), "{f}");
        }
        for g in ["screen", "screen_sampler", "params", "DuskParams"] {
            assert!(new(&[(g, B(F32))]).unwrap_err().contains("generated pass code"), "{g}");
        }
        assert!(new(&[("kind", B(F32))]).unwrap_err().contains("<pass> takes kind"));
        assert!(new(&[("loop", B(F32))]).unwrap_err().contains("reserved"));
        // 32 bytes of frame fields and padding, then 14 vec4s fill 256 exactly; 15 do not fit.
        assert!(new(&[("a", ParamType::Array { array: Vec4f, len: 14 })]).is_ok());
        let e = new(&[("a", ParamType::Array { array: Vec4f, len: 15 })]).unwrap_err();
        assert!(e.contains("272 bytes with the frame fields; at most 256"), "{e}");
    }

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
    fn decodes_every_type_back_to_its_bytes() {
        let l = layout(&[
            ("f", B(F32)),
            ("i", B(I32)),
            ("u", B(U32)),
            ("v2", B(Vec2f)),
            ("v3", B(Vec3f)),
            ("v4", B(Vec4f)),
            ("m", B(Mat4x4f)),
            ("c", B(Color)),
            ("fs", ParamType::Array { array: F32, len: 2 }),
            ("is", ParamType::Array { array: I32, len: 2 }),
            ("v2s", ParamType::Array { array: Vec2f, len: 2 }),
            ("cs", ParamType::Array { array: Color, len: 2 }),
            ("ms", ParamType::Array { array: Mat4x4f, len: 1 }),
        ]);
        let mut b = l.defaults();
        let values = [
            ("f", json!(0.1)),
            ("i", json!(-7)),
            ("u", json!(4000000000u32)),
            ("v2", json!([1.5, -2])),
            ("v3", json!([0.3, 0.2, 0.1])),
            ("v4", json!([1, 2, 3, 4])),
            ("m", json!((0..16).map(|i| i as f64 / 3.0).collect::<Vec<_>>())),
            ("c", json!("#ff8800")),
            ("fs", json!([0.25, 1e-3])),
            ("is", json!([i32::MIN, i32::MAX])),
            ("v2s", json!([[1, 2], [3, 4]])),
            ("cs", json!(["#123456", [0.5, 0.5, 0.5, 0.2]])),
            ("ms", json!([(0..16).map(f64::from).collect::<Vec<_>>()])),
        ];
        for (n, v) in &values {
            l.encode(n, v, &mut b).unwrap();
        }
        // Zeros, as the padding between array elements is in `b`: encode writes no padding.
        let mut again = vec![0u8; l.size];
        for f in &l.fields {
            let v = l.decode(&f.name, &b).unwrap();
            l.encode(&f.name, &v, &mut again).unwrap_or_else(|e| panic!("{}: {e}", f.name));
        }
        for f in &l.fields {
            let r = f.offset..f.offset + f.ty.size();
            assert_eq!(b[r.clone()], again[r], "{} round-trips", f.name);
        }
        assert_eq!(l.decode("i", &b), Some(json!(-7)));
        assert_eq!(l.decode("u", &b), Some(json!(4000000000u32)));
        assert_eq!(l.decode("v2", &b), Some(json!([1.5, -2.0])));
        assert_eq!(l.decode("c", &b).unwrap()["LinearRgba"]["red"], json!(1.0));
        assert_eq!(l.decode("nope", &b), None);
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
        assert!(new(&["asset"]).unwrap_err().contains("<material> takes asset itself"));
        assert!(ParamsLayout::new([("asset", &B(F32))]).unwrap_err().contains("<material> takes asset itself"));
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
    fn instance_params_append_a_struct_of_256_bytes_and_a_storage_array() {
        let p = [("color", B(Color))];
        let i = [("hover", B(F32)), ("tint", B(Vec3f))];
        let b = Bindings::new("game::Glow", p.iter().map(|(n, t)| (*n, t)), &["noise".into()]).unwrap().with_instance(i.iter().map(|(n, t)| (*n, t))).unwrap();
        assert_eq!(b.instance_binding(), Some(3));
        assert_eq!(offsets(b.instance()), [0, 16]);
        let w = b.wgsl("game::Glow");
        // hover at 0, tint 16..28: 228 bytes of padding make 256.
        assert!(w.contains("struct GlowInstance {\n    hover: f32,\n    tint: vec3<f32>,\n    bevy_ts_pad: array<u32, 57>,\n}"), "{w}");
        assert!(w.contains("@group(3) @binding(3) var<storage, read> instances: array<GlowInstance>;"), "{w}");
        assert!(w.contains("fn instance(i: u32) -> GlowInstance {\n    return instances[bevy_ts_get_tag(i)];\n}"), "{w}");
        assert!(b.generated_names().contains(&"instances".to_string()));
        // Exactly 256 bytes: no padding field.
        let full = [("m", ParamType::Array { array: Mat4x4f, len: 4 })];
        let w = Bindings::new("k", [], &[]).unwrap().with_instance(full.iter().map(|(n, t)| (*n, t))).unwrap().wgsl("k");
        assert!(!w.contains("bevy_ts_pad"), "{w}");
        assert_eq!(Bindings::new("k", [], &[]).unwrap().with_instance([]).unwrap().instance_binding(), None);
    }

    #[test]
    fn a_2d_kind_binds_at_group_2_and_imports_the_2d_get_tag() {
        let p = [("color", B(Color))];
        let i = [("seed", B(F32))];
        let b = Bindings::new("game::Planet", p.iter().map(|(n, t)| (*n, t)), &["noise".into()]).unwrap().with_instance(i.iter().map(|(n, t)| (*n, t))).unwrap();
        assert_eq!(b.space(), Space::D3);
        let b = b.in_space(Space::D2);
        let w = b.wgsl("game::Planet");
        assert!(w.contains("@group(2) @binding(0) var<uniform> params: PlanetParams;"), "{w}");
        assert!(w.contains("@group(2) @binding(1) var noise: texture_2d<f32>;"), "{w}");
        assert!(w.contains("@group(2) @binding(2) var noise_sampler: sampler;"), "{w}");
        assert!(w.contains("@group(2) @binding(3) var<storage, read> instances: array<PlanetInstance>;"), "{w}");
        assert!(!w.contains("@group(3)"), "{w}");
        assert_eq!(Space::D2.instance_import(), "import bevy_sprite_render::mesh2d::functions::get_tag as bevy_ts_get_tag; ");
        assert_eq!(Space::D3.instance_import(), INSTANCE_IMPORT);
    }

    #[test]
    fn instance_names_and_size_are_checked() {
        let p = [("pulse", B(F32))];
        let new = |i: &[(&str, ParamType)]| {
            Bindings::new("game::Glow", p.iter().map(|(n, t)| (*n, t)), &["noise".into()]).unwrap().with_instance(i.iter().map(|(n, t)| (*n, t)))
        };
        assert!(new(&[("hover", B(F32))]).is_ok());
        for taken in ["pulse", "noise", "noise_sampler"] {
            assert!(new(&[(taken, B(F32))]).unwrap_err().contains("already uses"), "{taken}");
        }
        for generated in ["instance", "instances", "GlowInstance", "bevy_ts_pad", "bevy_ts_get_tag"] {
            assert!(new(&[(generated, B(F32))]).unwrap_err().contains("generated instance code"), "{generated}");
        }
        assert!(new(&[("asset", B(F32))]).unwrap_err().contains("<material> takes asset itself"));
        assert!(new(&[("loop", B(F32))]).unwrap_err().contains("instance param loop is a WGSL reserved word"));
        let e = new(&[("m", ParamType::Array { array: Mat4x4f, len: 5 })]).unwrap_err();
        assert!(e.contains("320 bytes; at most 256"), "{e}");
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
