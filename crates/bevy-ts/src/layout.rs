//! Native layouts: the field types TS can declare, the C layout rules Rust computes, and the
//! registry of Rust components that may cross into TS systems (`offset_of!` layouts only).

use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A field type, as declared in TS and carried in the manifest.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FieldType {
    F32,
    F64,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    Quat,
    Entity,
    /// A fieldless enum. TS-declared enums store the variant index in the smallest width; a Rust
    /// enum carries `repr`, the bytes this build stores per variant, read at startup.
    Enum {
        variants: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repr: Option<EnumRepr>,
    },
    Array { of: Box<FieldType>, len: u32 },
    /// Bevy's `Timer` as plain data, every `Duration` in seconds: the fields of [`timer_layout`].
    Timer,
    /// Bevy's `Stopwatch` as plain data: the fields of [`stopwatch_layout`].
    Stopwatch,
    /// A heap field: Rust's `String`, in the component's own bytes (components only).
    #[serde(rename = "string")]
    Str,
    /// A heap field: Rust's `Vec<T>` of a number kind, `bool` or `string` (components only).
    List { of: Box<FieldType> },
}

/// A heap field's storage: a `String` or a `Vec<T>`, 24 bytes aligned to 8.
const HEAP_SIZE: usize = 24;
const _: () = assert!(size_of::<String>() == HEAP_SIZE && align_of::<String>() == 8);
const _: () = assert!(size_of::<Vec<f64>>() == HEAP_SIZE && align_of::<Vec<u8>>() == 8);

/// A `Timer` field: 24 bytes aligned to 8. `mode` is `TimerMode`, `Once` 0 and `Repeating` 1.
pub fn timer_layout() -> &'static ComponentLayout {
    static L: std::sync::LazyLock<ComponentLayout> = std::sync::LazyLock::new(|| {
        let mode = FieldType::Enum { variants: vec!["Once".into(), "Repeating".into()], repr: None };
        ComponentLayout::c_layout([
            ("elapsed", &FieldType::F64),
            ("duration", &FieldType::F64),
            ("times_finished_this_tick", &FieldType::U32),
            ("mode", &mode),
            ("paused", &FieldType::Bool),
            ("finished", &FieldType::Bool),
        ])
    });
    &L
}

/// A `Stopwatch` field: 16 bytes aligned to 8.
pub fn stopwatch_layout() -> &'static ComponentLayout {
    static L: std::sync::LazyLock<ComponentLayout> =
        std::sync::LazyLock::new(|| ComponentLayout::c_layout([("elapsed", &FieldType::F64), ("paused", &FieldType::Bool)]));
    &L
}

impl FieldType {
    /// Size in bytes.
    pub fn size(&self) -> usize {
        match self {
            FieldType::I8 | FieldType::U8 | FieldType::Bool => 1,
            FieldType::I16 | FieldType::U16 => 2,
            FieldType::F32 | FieldType::I32 | FieldType::U32 => 4,
            FieldType::F64 | FieldType::Entity => 8,
            FieldType::Vec2 => 8,
            FieldType::Vec3 => 12,
            FieldType::Vec4 | FieldType::Quat => 16,
            FieldType::Enum { repr: Some(r), .. } => r.width as usize,
            FieldType::Enum { variants, .. } => enum_width(variants.len()),
            FieldType::Array { of, len } => of.size() * *len as usize,
            FieldType::Timer | FieldType::Stopwatch => self.fields().unwrap().size,
            FieldType::Str | FieldType::List { .. } => HEAP_SIZE,
        }
    }

    /// Whether the value lives on the heap (`String`, `Vec<T>`): a component holding one gets a
    /// drop fn, a deep clone and a header (`crate::heap`).
    pub fn is_heap(&self) -> bool {
        matches!(self, FieldType::Str | FieldType::List { .. })
    }

    /// Alignment in bytes. Every field's alignment equals its element width, so one typed-array
    /// view per width indexes every field exactly.
    pub fn align(&self) -> usize {
        match self {
            FieldType::Vec2 | FieldType::Vec3 | FieldType::Vec4 | FieldType::Quat => 4,
            FieldType::Array { of, .. } => of.align(),
            FieldType::Timer | FieldType::Stopwatch | FieldType::Str | FieldType::List { .. } => 8,
            other => other.size(),
        }
    }

    /// The type's name as TS writes it, used in errors and schemas.
    pub fn ts_name(&self) -> String {
        match self {
            FieldType::F32 => "f32".into(),
            FieldType::F64 => "f64".into(),
            FieldType::I8 => "i8".into(),
            FieldType::U8 => "u8".into(),
            FieldType::I16 => "i16".into(),
            FieldType::U16 => "u16".into(),
            FieldType::I32 => "i32".into(),
            FieldType::U32 => "u32".into(),
            FieldType::Bool => "bool".into(),
            FieldType::Vec2 => "vec2".into(),
            FieldType::Vec3 => "vec3".into(),
            FieldType::Vec4 => "vec4".into(),
            FieldType::Quat => "quat".into(),
            FieldType::Entity => "entity".into(),
            FieldType::Enum { variants, .. } => format!("enumOf({})", variants.join(", ")),
            FieldType::Array { of, len } => format!("array({}, {len})", of.ts_name()),
            FieldType::Timer => "Timer".into(),
            FieldType::Stopwatch => "Stopwatch".into(),
            FieldType::Str => "string".into(),
            FieldType::List { of } => format!("{}[]", of.ts_name()),
        }
    }

    /// A struct-like type's own fields (`Timer`, `Stopwatch`), read and written as an object of them.
    pub fn fields(&self) -> Option<&'static ComponentLayout> {
        match self {
            FieldType::Timer => Some(timer_layout()),
            FieldType::Stopwatch => Some(stopwatch_layout()),
            _ => None,
        }
    }

    /// Number of scalar lanes for vector types, `None` for others.
    pub fn lanes(&self) -> Option<usize> {
        match self {
            FieldType::Vec2 => Some(2),
            FieldType::Vec3 => Some(3),
            FieldType::Vec4 | FieldType::Quat => Some(4),
            _ => None,
        }
    }

    /// Checks the type is one this change supports (enums non-empty, arrays non-empty and of
    /// scalars or vectors).
    pub fn validate(&self) -> Result<(), String> {
        match self {
            FieldType::Enum { variants, .. } if variants.is_empty() => {
                Err("enumOf needs at least one variant".into())
            }
            FieldType::Array { len: 0, .. } => Err("array length must be at least 1".into()),
            FieldType::Array { of, .. } if matches!(**of, FieldType::Array { .. }) => {
                Err("arrays of arrays are not supported".into())
            }
            FieldType::Array { of, .. } if of.fields().is_some() || of.is_heap() => Err(format!("arrays of {} are not supported", of.ts_name())),
            FieldType::Array { of, .. } => of.validate(),
            FieldType::List { of } => match **of {
                FieldType::F32 | FieldType::F64 | FieldType::I8 | FieldType::U8 | FieldType::I16 | FieldType::U16 | FieldType::I32 | FieldType::U32 | FieldType::Bool | FieldType::Str => Ok(()),
                ref other => Err(format!("{}[] is not supported: a list holds numbers (f32, f64, i8..u32), bool or string", other.ts_name())),
            },
            _ => Ok(()),
        }
    }
}

/// How a Rust enum stores its variants: `values[i]` is variant `i`'s bytes, little-endian, in
/// `width` bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnumRepr {
    pub width: u8,
    pub values: Vec<u32>,
}

impl FieldType {
    /// The stored value of variant `index`.
    pub fn enum_value(&self, index: usize) -> Option<u32> {
        match self {
            FieldType::Enum { repr: Some(r), .. } => r.values.get(index).copied(),
            FieldType::Enum { variants, .. } => (index < variants.len()).then_some(index as u32),
            _ => None,
        }
    }

    /// The variant index a stored value stands for.
    pub fn enum_index(&self, value: u32) -> Option<usize> {
        match self {
            FieldType::Enum { repr: Some(r), .. } => r.values.iter().position(|v| *v == value),
            FieldType::Enum { variants, .. } => ((value as usize) < variants.len()).then_some(value as usize),
            _ => None,
        }
    }

    /// Whether TS may only write this field through an accessor that keeps the value valid
    /// (`bool`, enums, `entity`), never as a raw array.
    pub fn accessor_only(&self) -> bool {
        match self {
            FieldType::Bool | FieldType::Entity | FieldType::Enum { .. } | FieldType::Timer | FieldType::Stopwatch | FieldType::Str | FieldType::List { .. } => true,
            FieldType::Array { of, .. } => of.accessor_only(),
            _ => false,
        }
    }
}

fn enum_width(variants: usize) -> usize {
    if variants <= u8::MAX as usize + 1 {
        1
    } else if variants <= u16::MAX as usize + 1 {
        2
    } else {
        4
    }
}

/// One field of a computed layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldLayout {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: FieldType,
    pub offset: usize,
}

/// A component's native layout: C struct rules, fields in declaration order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentLayout {
    pub size: usize,
    pub align: usize,
    pub fields: Vec<FieldLayout>,
}

impl ComponentLayout {
    /// Lays out `fields` with C rules: each field at an offset aligned to its own alignment,
    /// total size rounded up to the largest alignment. A layout with a heap field starts with
    /// [`crate::heap::HEADER`] bytes Rust owns; one without has none.
    pub fn c_layout<'a>(fields: impl IntoIterator<Item = (&'a str, &'a FieldType)>) -> Self {
        let fields: Vec<_> = fields.into_iter().collect();
        let header = fields.iter().any(|(_, t)| t.is_heap());
        let mut offset = if header { crate::heap::HEADER } else { 0 };
        let mut align = if header { 8 } else { 1 };
        let mut out = Vec::new();
        for (name, ty) in fields {
            let a = ty.align();
            align = align.max(a);
            offset = offset.next_multiple_of(a);
            out.push(FieldLayout { name: name.to_string(), ty: ty.clone(), offset });
            offset += ty.size();
        }
        ComponentLayout { size: offset.next_multiple_of(align), align, fields: out }
    }

    pub fn field(&self, name: &str) -> Option<&FieldLayout> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Whether a field is a heap field, so the value starts with a header.
    pub fn has_heap(&self) -> bool {
        self.fields.iter().any(|f| f.ty.is_heap())
    }

    pub fn std_layout(&self) -> std::alloc::Layout {
        std::alloc::Layout::from_size_align(self.size, self.align)
            .expect("component layouts are valid")
            .pad_to_align()
    }

    /// True when two layouts store the same fields at the same offsets.
    pub fn same_shape(&self, other: &ComponentLayout) -> bool {
        self == other
    }
}

/// Where a native layout came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeSource {
    /// Offsets read from reflection at startup (native decision 5).
    Reflect,
    /// `#[derive(NativeLayout)]`: offsets from `offset_of!`.
    OffsetOf,
}

/// A layout-tier Rust component: a column in TS systems.
#[derive(Clone, Debug)]
pub struct NativeComponent {
    /// The name TS uses: the reflected type path.
    pub ts_name: String,
    /// The type path's last segment (`Transform`), which also finds it when unambiguous.
    pub short_name: String,
    /// Bevy's type path (`bevy_transform::components::transform::Transform`).
    pub type_path: String,
    pub id: ComponentId,
    pub layout: ComponentLayout,
    /// A whole default value's bytes (padding included), for elements and spawns.
    pub defaults: Vec<u8>,
    /// False for components declared immutable: TS never writes them.
    pub writable: bool,
    /// Fields TS cannot read (`name: why`): never exposed, never written. A type with holes
    /// offers no raw `q.tables` writes.
    pub holes: Vec<String>,
    /// Holes reflection can write (every hole but an opaque one): TSX props, never columns.
    pub reflect_holes: Vec<ReflectHole>,
    /// Whether a whole value may be inserted from bytes (spawn, insert, TSX): false when holes
    /// may own heap data.
    pub insertable: bool,
    pub source: NativeSource,
    /// A resource (`ReflectResource`, or `ts_resource_layout`): reachable only as `Res`/`ResMut`.
    pub resource: bool,
}

/// A hole of a layout-tier component that TSX writes through reflection (a light's `color`).
#[derive(Clone, Debug, PartialEq)]
pub struct ReflectHole {
    pub name: String,
    /// The field's reflected type path, for its TS type.
    pub type_path: String,
}

/// A registered reflected component that is not a column, and why.
#[derive(Clone, Debug, Serialize)]
pub struct ReflectTier {
    #[serde(rename = "typePath")]
    pub type_path: String,
    #[serde(skip)]
    pub id: ComponentId,
    pub reason: String,
    /// False when no value could be had yet: it is tiered again once a row exists.
    #[serde(skip)]
    pub sampled: bool,
    pub resource: bool,
}

/// The native layout registry (bridge decision 4, native N1): every layout-tier Rust component,
/// filled from reflection at startup and by `NativeLayout` derives, plus the reflect-tier ones
/// with their reasons.
#[derive(Resource, Default, Clone, Debug)]
pub struct NativeLayouts {
    by_path: HashMap<String, NativeComponent>,
    /// Short name to type paths, for lookups by short name.
    short: HashMap<String, Vec<String>>,
    reflect_tier: HashMap<String, ReflectTier>,
    /// Type paths of tiered components that are tuple structs of one field (`WorldAssetRoot`):
    /// TSX gives that field as the prop `value`.
    newtypes: std::collections::HashSet<String>,
    /// Set once the registry has been filled from the type registry.
    pub filled: bool,
}

impl NativeLayouts {
    fn path_of<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        if self.by_path.contains_key(name) || self.reflect_tier.contains_key(name) {
            return Some(name);
        }
        match self.short.get(name).map(Vec::as_slice) {
            Some([one]) => Some(one.as_str()),
            _ => None,
        }
    }

    /// A layout-tier component by type path, or by short name when only one type has it.
    pub fn get(&self, name: &str) -> Option<&NativeComponent> {
        self.path_of(name).and_then(|p| self.by_path.get(p))
    }

    /// A reflect-tier component by type path or unambiguous short name, with the reason.
    pub fn reflect_tier(&self, name: &str) -> Option<&ReflectTier> {
        self.path_of(name).and_then(|p| self.reflect_tier.get(p))
    }

    /// Whether the component is a tuple struct of one field.
    pub fn is_newtype(&self, name: &str) -> bool {
        self.path_of(name).is_some_and(|p| self.newtypes.contains(p))
    }

    pub(crate) fn mark_newtype(&mut self, path: &str) {
        self.newtypes.insert(path.to_string());
    }

    /// Short names more than one tiered type shares, with their paths.
    pub fn ambiguous(&self, name: &str) -> Option<&[String]> {
        self.short.get(name).filter(|v| v.len() > 1).map(Vec::as_slice)
    }

    pub fn by_id(&self, id: ComponentId) -> Option<&NativeComponent> {
        self.by_path.values().find(|c| c.id == id)
    }

    /// Layout-tier components, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = &NativeComponent> {
        self.by_path.values()
    }

    /// Reflect-tier components, in no particular order.
    pub fn reflect_tiered(&self) -> impl Iterator<Item = &ReflectTier> {
        self.reflect_tier.values()
    }

    fn index_short(&mut self, short: &str, path: &str) {
        let v = self.short.entry(short.to_string()).or_default();
        if !v.iter().any(|p| p == path) {
            v.push(path.to_string());
            v.sort();
        }
    }

    /// Records a layout-tier component, replacing a reflect-tier record of the same type.
    pub fn insert(&mut self, c: NativeComponent) {
        self.reflect_tier.remove(&c.type_path);
        self.index_short(&c.short_name.clone(), &c.type_path.clone());
        self.by_path.insert(c.type_path.clone(), c);
    }

    pub(crate) fn set_reflect_tier(&mut self, path: &str, short: &str, id: ComponentId, reason: String, sampled: bool, resource: bool) {
        self.index_short(short, path);
        self.reflect_tier.insert(path.to_string(), ReflectTier { type_path: path.to_string(), id, reason, sampled, resource });
    }

    /// Sets each component's TS name: its short name when no other tiered type shares it, else
    /// its type path. Lookups accept either.
    pub fn assign_names(&mut self) {
        for c in self.by_path.values_mut() {
            let unique = self.short.get(&c.short_name).is_some_and(|v| v.len() == 1);
            c.ts_name = if unique { c.short_name.clone() } else { c.type_path.clone() };
        }
    }

    pub(crate) fn forget_reflect_tier(&mut self, path: &str) {
        self.reflect_tier.remove(path);
    }

    /// Registers a layout for a component without `Reflect` (`#[derive(NativeLayout)]`). The
    /// caller vouches that `layout` describes `T` exactly: offsets from `offset_of!`, every field
    /// plain data. `defaults` is a whole value's bytes, or zeroes when `None`.
    pub fn register<T: Component>(&mut self, world: &mut World, layout: ComponentLayout, defaults: Option<Vec<u8>>) {
        self.register_as::<T>(world, layout, defaults, false);
    }

    /// [`Self::register`] for a resource: reachable as `Res`/`ResMut`, never as query data.
    pub fn register_resource<T: Resource>(&mut self, world: &mut World, layout: ComponentLayout, defaults: Option<Vec<u8>>) {
        self.register_as::<T>(world, layout, defaults, true);
    }

    fn register_as<T: Component>(&mut self, world: &mut World, layout: ComponentLayout, defaults: Option<Vec<u8>>, resource: bool) {
        let path = std::any::type_name::<T>().to_string();
        assert_eq!(layout.size, size_of::<T>(), "native layout size for {path}");
        assert_eq!(layout.align, align_of::<T>(), "native layout align for {path}");
        for f in &layout.fields {
            assert!(f.offset + f.ty.size() <= layout.size, "native layout field {path}.{} lies outside the value", f.name);
        }
        let defaults = defaults.unwrap_or_else(|| vec![0; layout.size]);
        assert_eq!(defaults.len(), layout.size, "native layout defaults for {path}");
        let id = world.register_component::<T>();
        let writable = world.components().get_info(id).is_none_or(|i| i.mutable());
        let short = path.rsplit("::").next().unwrap_or(&path).to_string();
        self.insert(NativeComponent {
            ts_name: path.clone(),
            short_name: short,
            type_path: path,
            id,
            layout,
            defaults,
            writable,
            holes: Vec::new(),
            reflect_holes: Vec::new(),
            insertable: true,
            source: NativeSource::OffsetOf,
            resource,
        });
        self.assign_names();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(fields: &[(&str, FieldType)]) -> ComponentLayout {
        ComponentLayout::c_layout(fields.iter().map(|(n, t)| (*n, t)))
    }

    fn offsets(l: &ComponentLayout) -> Vec<usize> {
        l.fields.iter().map(|f| f.offset).collect()
    }

    #[test]
    fn mixed_component() {
        let l = layout(&[("alive", FieldType::Bool), ("hp", FieldType::F32), ("pos", FieldType::Vec3)]);
        assert_eq!((l.size, l.align), (20, 4));
        assert_eq!(offsets(&l), vec![0, 4, 8]);
    }

    #[test]
    fn a_heap_field_puts_a_header_first() {
        let l = layout(&[("alive", FieldType::Bool), ("label", FieldType::Str), ("hp", FieldType::F32)]);
        assert_eq!(offsets(&l), vec![8, 16, 40]);
        assert_eq!((l.size, l.align), (48, 8));
        assert!(l.has_heap());
        let plain = layout(&[("alive", FieldType::Bool), ("hp", FieldType::F32), ("pos", FieldType::Vec3)]);
        assert!(!plain.has_heap());
        assert!(FieldType::Array { of: Box::new(FieldType::Str), len: 2 }.validate().is_err());
        let list = |of: FieldType| FieldType::List { of: Box::new(of) };
        assert!(list(FieldType::U32).validate().is_ok() && list(FieldType::Str).validate().is_ok());
        assert!(list(FieldType::Vec3).validate().is_err() && list(list(FieldType::U8)).validate().is_err());
        assert_eq!((list(FieldType::F32).size(), list(FieldType::F32).align(), list(FieldType::F32).ts_name()), (24, 8, "f32[]".into()));
    }

    #[test]
    fn vectors_align_to_four() {
        let l = layout(&[("a", FieldType::U8), ("q", FieldType::Quat), ("b", FieldType::Vec2)]);
        assert_eq!(offsets(&l), vec![0, 4, 20]);
        assert_eq!((l.size, l.align), (28, 4));
    }

    #[test]
    fn enums_take_the_smallest_width() {
        let small = FieldType::Enum { variants: vec!["a".into(), "b".into()], repr: None };
        let wide = FieldType::Enum { variants: (0..300).map(|i| i.to_string()).collect(), repr: None };
        assert_eq!(small.size(), 1);
        assert_eq!(wide.size(), 2);
        let l = layout(&[("s", small), ("w", wide), ("x", FieldType::U8)]);
        assert_eq!(offsets(&l), vec![0, 2, 4]);
        assert_eq!((l.size, l.align), (6, 2));
    }

    #[test]
    fn arrays_align_to_their_element() {
        let l = layout(&[
            ("flag", FieldType::U8),
            ("xs", FieldType::Array { of: Box::new(FieldType::I16), len: 3 }),
            ("d", FieldType::F64),
        ]);
        assert_eq!(offsets(&l), vec![0, 2, 8]);
        assert_eq!((l.size, l.align), (16, 8));
    }

    #[test]
    fn timer_and_stopwatch_layouts() {
        let t = timer_layout();
        assert_eq!((t.size, t.align), (24, 8));
        assert_eq!(offsets(t), vec![0, 8, 16, 20, 21, 22]);
        assert_eq!((FieldType::Timer.size(), FieldType::Timer.align()), (24, 8));
        let s = stopwatch_layout();
        assert_eq!((s.size, s.align), (16, 8));
        assert_eq!(offsets(s), vec![0, 8]);
        let l = layout(&[("hp", FieldType::F32), ("cooldown", FieldType::Timer)]);
        assert_eq!(offsets(&l), vec![0, 8]);
        assert_eq!((l.size, l.align), (32, 8));
    }

    #[test]
    fn entity_is_u64_aligned_to_eight() {
        let l = layout(&[("hp", FieldType::F32), ("target", FieldType::Entity)]);
        assert_eq!(offsets(&l), vec![0, 8]);
        assert_eq!((l.size, l.align), (16, 8));
    }

    #[test]
    fn every_offset_is_a_multiple_of_its_element_width() {
        let l = layout(&[
            ("a", FieldType::Bool),
            ("b", FieldType::F64),
            ("c", FieldType::I16),
            ("d", FieldType::Vec3),
            ("e", FieldType::U8),
            ("f", FieldType::Entity),
        ]);
        for f in &l.fields {
            let width = match &f.ty {
                FieldType::Vec3 => 4,
                t => t.align(),
            };
            assert_eq!(f.offset % width, 0, "{}", f.name);
        }
    }

    #[test]
    fn transform_layout_matches_rust() {
        let mut world = World::new();
        let registry = AppTypeRegistry::default();
        registry.write().register::<Transform>();
        world.insert_resource(registry);
        crate::layout_reflect::fill(&mut world);
        let native = world.resource::<NativeLayouts>();
        let t = native.get("Transform").unwrap();
        assert_eq!(t.layout.size, size_of::<Transform>());
        use core::mem::offset_of;
        assert_eq!(t.layout.field("translation").unwrap().offset, offset_of!(Transform, translation));
        assert_eq!(t.layout.field("rotation").unwrap().offset, offset_of!(Transform, rotation));
        assert_eq!(t.layout.field("scale").unwrap().offset, offset_of!(Transform, scale));
        let tr = Transform::from_xyz(1.0, 2.0, 3.0).with_scale(Vec3::splat(4.0));
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(&tr as *const Transform as *const u8, size_of::<Transform>()) };
        let f = |off: usize| f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        let tl = t.layout.field("translation").unwrap().offset;
        let sc = t.layout.field("scale").unwrap().offset;
        assert_eq!((f(tl), f(tl + 4), f(tl + 8)), (1.0, 2.0, 3.0));
        assert_eq!(f(sc), 4.0);
        // The default's bytes: identity rotation, unit scale.
        let d = |off: usize| f32::from_le_bytes(t.defaults[off..off + 4].try_into().unwrap());
        assert_eq!(d(t.layout.field("rotation").unwrap().offset + 12), 1.0);
        assert_eq!(d(sc), 1.0);
    }
}
