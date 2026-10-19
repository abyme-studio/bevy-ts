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
    Enum { variants: Vec<String> },
    Array { of: Box<FieldType>, len: u32 },
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
            FieldType::Enum { variants } => enum_width(variants.len()),
            FieldType::Array { of, len } => of.size() * *len as usize,
        }
    }

    /// Alignment in bytes. Every field's alignment equals its element width, so one typed-array
    /// view per width indexes every field exactly.
    pub fn align(&self) -> usize {
        match self {
            FieldType::Vec2 | FieldType::Vec3 | FieldType::Vec4 | FieldType::Quat => 4,
            FieldType::Array { of, .. } => of.align(),
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
            FieldType::Enum { variants } => format!("enumOf({})", variants.join(", ")),
            FieldType::Array { of, len } => format!("array({}, {len})", of.ts_name()),
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
            FieldType::Enum { variants } if variants.is_empty() => {
                Err("enumOf needs at least one variant".into())
            }
            FieldType::Array { len: 0, .. } => Err("array length must be at least 1".into()),
            FieldType::Array { of, .. } if matches!(**of, FieldType::Array { .. }) => {
                Err("arrays of arrays are not supported".into())
            }
            FieldType::Array { of, .. } => of.validate(),
            _ => Ok(()),
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
    /// total size rounded up to the largest alignment.
    pub fn c_layout<'a>(fields: impl IntoIterator<Item = (&'a str, &'a FieldType)>) -> Self {
        let mut offset = 0usize;
        let mut align = 1;
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

/// A Rust component that may appear in a TS system's `read`/`write`, with a layout taken from
/// `offset_of!`, `size_of` and `align_of`.
#[derive(Clone, Debug)]
pub struct NativeComponent {
    /// The name TS uses (`Transform`), exported from `bevy-ts/bevy`.
    pub ts_name: String,
    /// Bevy's type path (`bevy_transform::components::transform::Transform`).
    pub type_path: String,
    pub id: ComponentId,
    pub layout: ComponentLayout,
}

/// Rust components with a declared native layout.
#[derive(Resource, Default, Clone, Debug)]
pub struct NativeLayouts {
    by_name: HashMap<String, NativeComponent>,
}

impl NativeLayouts {
    pub fn get(&self, ts_name: &str) -> Option<&NativeComponent> {
        self.by_name.get(ts_name)
    }

    pub fn by_id(&self, id: ComponentId) -> Option<&NativeComponent> {
        self.by_name.values().find(|c| c.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &NativeComponent> {
        self.by_name.values()
    }

    /// Registers a native layout. The caller vouches that `layout` describes `T` exactly; use
    /// [`NativeLayouts::register_transform`] as the template.
    pub fn register<T: Component>(&mut self, world: &mut World, ts_name: &str, layout: ComponentLayout) {
        assert_eq!(layout.size, size_of::<T>(), "native layout size for {ts_name}");
        assert_eq!(layout.align, align_of::<T>(), "native layout align for {ts_name}");
        let id = world.register_component::<T>();
        self.by_name.insert(
            ts_name.to_string(),
            NativeComponent {
                ts_name: ts_name.to_string(),
                type_path: std::any::type_name::<T>().to_string(),
                id,
                layout,
            },
        );
    }

    /// `Transform` as `translation: vec3, rotation: quat, scale: vec3`, offsets from
    /// `offset_of!`. Its Rust layout is not `repr(C)`, so nothing here is assumed.
    pub fn register_transform(&mut self, world: &mut World) {
        use core::mem::offset_of;
        // Each field must be plain f32 lanes for the typed-array views to index it.
        const _: () = assert!(size_of::<Vec3>() == 12 && align_of::<Vec3>() == 4);
        const _: () = assert!(size_of::<Quat>() == 16);
        let layout = ComponentLayout {
            size: size_of::<Transform>(),
            align: align_of::<Transform>(),
            fields: vec![
                FieldLayout {
                    name: "translation".into(),
                    ty: FieldType::Vec3,
                    offset: offset_of!(Transform, translation),
                },
                FieldLayout {
                    name: "rotation".into(),
                    ty: FieldType::Quat,
                    offset: offset_of!(Transform, rotation),
                },
                FieldLayout { name: "scale".into(), ty: FieldType::Vec3, offset: offset_of!(Transform, scale) },
            ],
        };
        self.register::<Transform>(world, "Transform", layout);
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
    fn vectors_align_to_four() {
        let l = layout(&[("a", FieldType::U8), ("q", FieldType::Quat), ("b", FieldType::Vec2)]);
        assert_eq!(offsets(&l), vec![0, 4, 20]);
        assert_eq!((l.size, l.align), (28, 4));
    }

    #[test]
    fn enums_take_the_smallest_width() {
        let small = FieldType::Enum { variants: vec!["a".into(), "b".into()] };
        let wide = FieldType::Enum { variants: (0..300).map(|i| i.to_string()).collect() };
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
        let mut native = NativeLayouts::default();
        native.register_transform(&mut world);
        let t = native.get("Transform").unwrap();
        assert_eq!(t.layout.size, size_of::<Transform>());
        let tr = Transform::from_xyz(1.0, 2.0, 3.0).with_scale(Vec3::splat(4.0));
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(&tr as *const Transform as *const u8, size_of::<Transform>()) };
        let f = |off: usize| f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        let tl = t.layout.field("translation").unwrap().offset;
        let sc = t.layout.field("scale").unwrap().offset;
        assert_eq!((f(tl), f(tl + 4), f(tl + 8)), (1.0, 2.0, 3.0));
        assert_eq!(f(sc), 4.0);
    }
}
