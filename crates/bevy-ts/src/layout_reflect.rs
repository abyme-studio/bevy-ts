//! Native layouts read from reflection (native decision 5): a value of the type is taken, and
//! each reflected leaf's offset is its reference's address minus the value's address. A derived
//! `Reflect` hands out references into the value itself, so this works whatever the `repr`, for
//! private fields and inside glam's SIMD types.

use crate::layout::{ComponentLayout, EnumRepr, FieldLayout, FieldType, NativeComponent, NativeLayouts, NativeSource, ReflectHole};
use bevy::ecs::component::ComponentId;
use bevy::ecs::reflect::{ReflectComponent, ReflectFromWorld};
use bevy::prelude::*;
use bevy::reflect::enums::{DynamicEnum, DynamicVariant, VariantInfo};
use bevy::reflect::{GetPath, PartialReflect, ReflectRef, TypeInfo, TypeRegistration};
use bevy::prelude::ReflectDefault;
use std::any::TypeId;

/// One leaf found by the walk.
struct Leaf {
    name: String,
    /// The reflect path to it, for enum probing (`""` for the value itself).
    path: String,
    offset: usize,
    /// Bytes the leaf's Rust value occupies.
    size: usize,
    ty: LeafType,
}

enum LeafType {
    Plain(FieldType),
    /// A fieldless enum; its bytes are probed afterwards.
    Enum(Vec<String>),
}

/// A known plain-data type by `TypeId`.
fn plain(id: TypeId) -> Option<FieldType> {
    use bevy::math::{Quat, Vec2, Vec3, Vec3A, Vec4};
    let t = [
        (TypeId::of::<f32>(), FieldType::F32),
        (TypeId::of::<f64>(), FieldType::F64),
        (TypeId::of::<i8>(), FieldType::I8),
        (TypeId::of::<u8>(), FieldType::U8),
        (TypeId::of::<i16>(), FieldType::I16),
        (TypeId::of::<u16>(), FieldType::U16),
        (TypeId::of::<i32>(), FieldType::I32),
        (TypeId::of::<u32>(), FieldType::U32),
        (TypeId::of::<bool>(), FieldType::Bool),
        (TypeId::of::<Vec2>(), FieldType::Vec2),
        (TypeId::of::<Vec3>(), FieldType::Vec3),
        (TypeId::of::<Vec3A>(), FieldType::Vec3),
        (TypeId::of::<Vec4>(), FieldType::Vec4),
        (TypeId::of::<Quat>(), FieldType::Quat),
    ];
    if id == TypeId::of::<Entity>() {
        return entity_is_bits().then_some(FieldType::Entity);
    }
    t.into_iter().find(|(t, _)| *t == id).map(|(_, f)| f)
}

/// Whether an `Entity` stores exactly `to_bits()` little-endian in this build, which is what the
/// `entity` field type reads and writes.
fn entity_is_bits() -> bool {
    let e = Entity::from_bits((7u64 << 32) | 42);
    let bytes: [u8; 8] = unsafe { std::mem::transmute_copy(&e) };
    size_of::<Entity>() == 8 && u64::from_le_bytes(bytes) == e.to_bits()
}

fn type_name(v: &dyn PartialReflect) -> String {
    v.get_represented_type_info().map_or_else(|| v.reflect_type_path().to_string(), |i| i.type_path().to_string())
}

fn addr(v: &dyn PartialReflect) -> usize {
    v as *const dyn PartialReflect as *const u8 as usize
}

/// A fieldless enum's variant names, or `None` when a variant carries data.
fn unit_variants(v: &dyn PartialReflect) -> Option<Vec<String>> {
    let Some(TypeInfo::Enum(info)) = v.get_represented_type_info() else { return None };
    info.iter().all(|x| matches!(x, VariantInfo::Unit(_))).then(|| info.iter().map(|x| x.name().to_string()).collect())
}

/// Why an opaque type cannot be written through `reflect_value`, if it cannot.
fn opaque_gap(type_path: &str) -> Option<&'static str> {
    #[cfg(feature = "reflect")]
    return crate::reflect_value::uncovered(type_path);
    #[cfg(not(feature = "reflect"))]
    return {
        let _ = type_path;
        Some("reflect_value is built with the reflect feature")
    };
}

/// What one top-level field turned out to be.
enum Found {
    Leaf(Leaf),
    /// A field bevy-ts cannot read: never exposed, never written. Its span still counts for the
    /// bounds and overlap checks.
    Hole { name: String, offset: usize, size: usize, why: String, writable: Option<String> },
}

fn leaf_of(v: &dyn PartialReflect, base: usize, name: String, path: String) -> Found {
    let offset = addr(v).wrapping_sub(base);
    let size = size_of_val(v);
    // Every hole but an opaque one is written through reflection: its type path, for its TS type.
    let hole = |why: String| Found::Hole { name: name.clone(), offset, size, why, writable: Some(type_name(v)) };
    if let Some(ty) = v.try_as_reflect().and_then(|r| plain(r.as_any().type_id())) {
        return Found::Leaf(Leaf { name, path, offset, size, ty: LeafType::Plain(ty) });
    }
    match v.reflect_ref() {
        ReflectRef::Enum(_) => match unit_variants(v) {
            Some(variants) if variants.len() >= 2 => Found::Leaf(Leaf { name, path, offset, size, ty: LeafType::Enum(variants) }),
            Some(_) => hole(format!("a single-variant enum ({})", type_name(v))),
            None => hole(format!("an enum with data ({})", type_name(v))),
        },
        ReflectRef::Array(a) if !a.is_empty() => {
            let first = a.get(0).expect("non-empty array");
            let Some(of) = first
                .try_as_reflect()
                .and_then(|r| plain(r.as_any().type_id()))
                .filter(|t| !matches!(t, FieldType::Entity | FieldType::Bool))
            else {
                return hole(format!("an array of {}", type_name(first)));
            };
            let stride = size_of_val(first);
            let contiguous = (0..a.len()).all(|i| addr(a.get(i).unwrap()).wrapping_sub(addr(first)) == i * stride);
            if stride != of.size() || !contiguous {
                return hole(format!("an array of {} that is not packed", type_name(first)));
            }
            let ty = FieldType::Array { of: Box::new(of), len: a.len() as u32 };
            Found::Leaf(Leaf { name, path, offset, size, ty: LeafType::Plain(ty) })
        }
        // An opaque hole is written by its reflected serde, unless reflect_value cannot cover it.
        ReflectRef::Opaque(_) => match opaque_gap(&type_name(v)) {
            Some(why) => Found::Hole { name, offset, size, why: format!("{} is not covered: {why}", type_name(v)), writable: None },
            None => hole(format!("opaque to reflection ({})", type_name(v))),
        },
        _ => hole(format!("not plain data ({})", type_name(v))),
    }
}

/// The top-level fields of a component value (glam vectors count as one leaf).
fn fields(value: &dyn PartialReflect) -> Result<Vec<Found>, String> {
    let base = addr(value);
    let mut out = Vec::new();
    match value.reflect_ref() {
        ReflectRef::Struct(s) => {
            for i in 0..s.field_len() {
                let name = s.name_at(i).unwrap().to_string();
                out.push(leaf_of(s.field_at(i).unwrap(), base, name.clone(), name));
            }
        }
        // A newtype's one field is `value` in TS (`ZIndex(2)` is `{ value: 2 }`); a wider tuple
        // struct's fields keep their indices.
        ReflectRef::TupleStruct(s) => {
            for i in 0..s.field_len() {
                let name = if s.field_len() == 1 { "value".into() } else { i.to_string() };
                out.push(leaf_of(s.field(i).unwrap(), base, name, format!(".{i}")));
            }
        }
        // A fieldless enum component (`RigidBody`) is one field named `value`.
        ReflectRef::Enum(_) => out.push(leaf_of(value, base, "value".into(), String::new())),
        _ => return Err(format!("{} is not a struct", type_name(value))),
    }
    Ok(out)
}

fn bytes_of(v: &dyn Reflect) -> Vec<u8> {
    let size = size_of_val(v);
    // SAFETY: `size_of_val` bytes from the value's address are the value. Padding is read as
    // whatever it holds, which only ever ends up compared or copied, never interpreted.
    unsafe { std::slice::from_raw_parts(v as *const dyn Reflect as *const u8, size) }.to_vec()
}

/// Reads each variant's stored bytes by applying it to the owned value and comparing snapshots.
fn probe_enum(value: &mut dyn Reflect, leaf: &Leaf, variants: &[String]) -> Result<EnumRepr, String> {
    if !matches!(leaf.size, 1 | 2 | 4) {
        return Err(format!("field {}: enum is {} bytes", leaf.name, leaf.size));
    }
    let mut values = Vec::new();
    let original = bytes_of(value);
    for v in variants {
        let target: &mut dyn PartialReflect = if leaf.path.is_empty() {
            value.as_partial_reflect_mut()
        } else {
            value.reflect_path_mut(leaf.path.as_str()).map_err(|e| format!("field {}: {e}", leaf.name))?
        };
        target
            .try_apply(&DynamicEnum::new(v.clone(), DynamicVariant::Unit))
            .map_err(|e| format!("field {}: setting {v}: {e}", leaf.name))?;
        let snap = bytes_of(value);
        let changed_outside = snap
            .iter()
            .zip(&original)
            .enumerate()
            .any(|(i, (a, b))| a != b && !(leaf.offset..leaf.offset + leaf.size).contains(&i));
        if changed_outside {
            return Err(format!("field {}: setting {v} changed bytes outside the field", leaf.name));
        }
        let mut raw = [0u8; 4];
        raw[..leaf.size].copy_from_slice(&snap[leaf.offset..leaf.offset + leaf.size]);
        values.push(u32::from_le_bytes(raw));
    }
    // Put the original variant back so the value's bytes are its default again.
    let mut sorted = values.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != values.len() {
        return Err(format!("field {}: two variants store the same bytes", leaf.name));
    }
    restore(value, &original);
    Ok(EnumRepr { width: leaf.size as u8, values })
}

fn restore(value: &mut dyn Reflect, bytes: &[u8]) {
    // SAFETY: `bytes` is a snapshot of this same value taken while it held a valid variant, and
    // the value has no heap data (every leaf is plain data, checked before probing).
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), value as *mut dyn Reflect as *mut u8, bytes.len()) };
}

/// A tiered value: its layout over the known leaves, its holes, and its bytes (its defaults).
pub struct Tiered {
    pub layout: ComponentLayout,
    /// `name: why` per hole.
    pub holes: Vec<String>,
    pub reflect_holes: Vec<ReflectHole>,
    pub defaults: Vec<u8>,
}

/// Tiers one value: its layout when it has a known leaf and the guards hold, or why not.
pub fn layout_of(value: &mut dyn Reflect) -> Result<Tiered, String> {
    let size = size_of_val(value);
    let align = std::mem::align_of_val(value);
    let found = fields(value.as_partial_reflect())?;
    let mut leaves = Vec::new();
    let mut holes = Vec::new();
    let mut reflect_holes = Vec::new();
    let mut spans = Vec::new();
    for f in found {
        let (name, offset, size_f) = match &f {
            Found::Leaf(l) => (l.name.clone(), l.offset, l.size),
            Found::Hole { name, offset, size, .. } => (name.clone(), *offset, *size),
        };
        if offset.checked_add(size_f).is_none_or(|end| end > size) {
            return Err(format!("field {name} lies outside the value"));
        }
        if size_f > 0 {
            spans.push((offset, offset + size_f, name));
        }
        match f {
            Found::Leaf(l) => leaves.push(l),
            Found::Hole { name, why, writable, .. } => {
                if let Some(type_path) = writable {
                    reflect_holes.push(ReflectHole { name: name.clone(), type_path });
                }
                holes.push(format!("{name}: {why}"));
            }
        }
    }
    if leaves.is_empty() {
        return Err(if holes.is_empty() { "it has no reflected fields".into() } else { format!("no field is plain data ({})", holes.join("; ")) });
    }
    spans.sort();
    for w in spans.windows(2) {
        if w[1].0 < w[0].1 {
            return Err(format!("fields {} and {} overlap", w[0].2, w[1].2));
        }
    }
    let mut fields = Vec::new();
    for leaf in &leaves {
        let ty = match &leaf.ty {
            LeafType::Plain(t) => {
                // Vec3A is 16 bytes holding a 12-byte vec3; every other leaf must match exactly.
                if leaf.size != t.size() && !(matches!(t, FieldType::Vec3) && leaf.size == 16) {
                    return Err(format!("field {}: {} bytes for {}", leaf.name, leaf.size, t.ts_name()));
                }
                t.clone()
            }
            LeafType::Enum(variants) => FieldType::Enum { variants: variants.clone(), repr: None },
        };
        if leaf.offset % ty.align() != 0 {
            return Err(format!("field {} at {} is not aligned to {}", leaf.name, leaf.offset, ty.align()));
        }
        fields.push(FieldLayout { name: leaf.name.clone(), ty, offset: leaf.offset });
    }
    let defaults = bytes_of(value);
    for (leaf, field) in leaves.iter().zip(fields.iter_mut()) {
        if let LeafType::Enum(variants) = &leaf.ty {
            let repr = probe_enum(value, leaf, variants)?;
            field.ty = FieldType::Enum { variants: variants.clone(), repr: Some(repr) };
        }
    }
    Ok(Tiered { layout: ComponentLayout { size, align, fields }, holes, reflect_holes, defaults })
}

/// A value of a registered type to read offsets from: `ReflectDefault`, then the first live row,
/// then `ReflectFromWorld`.
fn sample(world: &mut World, reg: &TypeRegistration, rc: &ReflectComponent, id: ComponentId) -> Option<Box<dyn Reflect>> {
    if let Some(d) = reg.data::<ReflectDefault>() {
        return Some(d.default());
    }
    let mut q = QueryBuilder::<Entity>::new(world).with_id(id).build();
    if let Some(e) = q.iter(world).next() {
        let v = rc.reflect(world.entity(e))?;
        if let Ok(v) = v.reflect_clone() {
            return Some(v);
        }
    }
    let fw = reg.data::<ReflectFromWorld>()?.clone();
    // A FromWorld impl may expect resources this app lacks; such a type stays untiered.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fw.from_world(world))).ok()
}

/// Tiers one registered component type and records it. Returns false when no value could be
/// had, so the caller may retry once a row exists.
fn tier(world: &mut World, reg: &TypeRegistration, native: &mut NativeLayouts) -> bool {
    let Some(rc) = reg.data::<ReflectComponent>().cloned() else { return true };
    let path = reg.type_info().type_path().to_string();
    let short = reg.type_info().type_path_table().short_path().to_string();
    if matches!(reg.type_info(), bevy::reflect::TypeInfo::TupleStruct(t) if t.field_len() == 1) {
        native.mark_newtype(&path);
    }
    if native.get(&path).is_some() {
        return true;
    }
    let id = rc.register_component(world);
    let Some(mut value) = sample(world, reg, &rc, id) else {
        native.set_reflect_tier(&path, &short, id, "no value to read offsets from (no Default, FromWorld or live row)".into(), false);
        return false;
    };
    let info = world.components().get_info(id).expect("registered");
    let (mutable, std_layout, drops) = (info.mutable(), info.layout(), info.drop().is_some());
    match layout_of(value.as_mut()) {
        Ok(t) if t.layout.size == std_layout.size() && t.layout.align == std_layout.align() => {
            // Bytes may only be inserted as a whole value when the holes copy safely: a type
            // with drop glue may own heap data behind them.
            let insertable = t.holes.is_empty() || !drops;
            native.insert(NativeComponent {
                ts_name: path.clone(),
                short_name: short,
                type_path: path,
                id,
                layout: t.layout,
                defaults: t.defaults,
                writable: mutable,
                holes: t.holes,
                reflect_holes: t.reflect_holes,
                insertable,
                source: NativeSource::Reflect,
            });
        }
        Ok(_) => native.set_reflect_tier(&path, &short, id, "the sampled value's size differs from the component's".into(), true),
        Err(reason) => native.set_reflect_tier(&path, &short, id, reason, true),
    }
    true
}

/// Fills the registry from every registered type with `ReflectComponent`. Idempotent; types
/// already tiered (or registered by `NativeLayout`) are kept.
pub fn fill(world: &mut World) {
    let Some(registry) = world.get_resource::<AppTypeRegistry>().cloned() else { return };
    let started = std::time::Instant::now();
    let mut native = world.remove_resource::<NativeLayouts>().unwrap_or_default();
    {
        let registry = registry.read();
        let mut regs: Vec<&TypeRegistration> = registry.iter().filter(|r| r.data::<ReflectComponent>().is_some()).collect();
        regs.sort_by_key(|r| r.type_info().type_path());
        for reg in regs {
            tier(world, reg, &mut native);
        }
    }
    native.assign_names();
    native.filled = true;
    debug!(
        "bevy-ts: {} Rust components are columns, {} reflect tier ({:?})",
        native.iter().count(),
        native.reflect_tiered().count(),
        started.elapsed()
    );
    world.insert_resource(native);
}

/// Retries a type that had no value when the registry was filled, now from a live row. Called
/// when a system or element names it.
pub fn retry(world: &mut World, name: &str) {
    let pending = world.get_resource::<NativeLayouts>().and_then(|n| n.reflect_tier(name)).is_some_and(|r| !r.sampled);
    if !pending {
        return;
    }
    let Some(registry) = world.get_resource::<AppTypeRegistry>().cloned() else { return };
    let registry = registry.read();
    let path = world.resource::<NativeLayouts>().reflect_tier(name).map(|r| r.type_path.clone()).unwrap();
    let Some(reg) = registry.get_with_type_path(&path) else { return };
    let mut native = world.remove_resource::<NativeLayouts>().unwrap();
    native.forget_reflect_tier(&path);
    tier(world, reg, &mut native);
    native.assign_names();
    world.insert_resource(native);
}

/// `First`, once: fills the registry after every plugin has registered its types.
pub fn fill_once(world: &mut World) {
    if !world.get_resource::<NativeLayouts>().is_some_and(|n| n.filled) {
        fill(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::FieldType;

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct Mover {
        speed: f32,
        dir: Vec3,
        target: Option<u8>,
    }

    #[derive(Component, Reflect)]
    #[reflect(Component, Default)]
    struct Plain {
        hp: f32,
        alive: bool,
        mode: Mode,
        pub target: Entity,
        rot: Quat,
        lanes: [u16; 3],
        #[reflect(ignore)]
        _hidden: u64,
    }

    impl Default for Plain {
        fn default() -> Self {
            Plain { hp: 0.0, alive: false, mode: Mode::Idle, target: Entity::PLACEHOLDER, rot: Quat::IDENTITY, lanes: [0; 3], _hidden: 0 }
        }
    }

    #[derive(Reflect, Default, Clone, Copy, PartialEq, Debug)]
    enum Mode {
        #[default]
        Idle,
        Run,
        Fly,
    }

    #[derive(Component, Reflect, Default, Clone, Copy)]
    #[reflect(Component, Default)]
    enum Body {
        Dynamic,
        #[default]
        Fixed,
    }

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct Private(u8);

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct Pair(pub f32, pub f32);

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    #[component(immutable)]
    struct Frozen {
        v: f32,
    }

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct NoDefault {
        x: f32,
    }

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct Named {
        name: String,
    }

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct WithName {
        hp: f32,
        name: String,
    }

    fn world() -> World {
        let mut world = World::new();
        let registry = AppTypeRegistry::default();
        {
            let mut r = registry.write();
            r.register::<Mover>();
            r.register::<Plain>();
            r.register::<Body>();
            r.register::<Private>();
            r.register::<Pair>();
            r.register::<Frozen>();
            r.register::<NoDefault>();
            r.register::<Named>();
            r.register::<WithName>();
        }
        world.insert_resource(registry);
        world
    }

    #[test]
    fn plain_leaves_are_layout_tier_whatever_the_repr() {
        let mut w = world();
        fill(&mut w);
        let n = w.resource::<NativeLayouts>();
        let p = n.get("Plain").expect("Plain is layout tier");
        let off = |f: &str| p.layout.field(f).unwrap().offset;
        assert_eq!(off("hp"), core::mem::offset_of!(Plain, hp));
        assert_eq!(off("alive"), core::mem::offset_of!(Plain, alive));
        assert_eq!(off("mode"), core::mem::offset_of!(Plain, mode));
        assert_eq!(off("target"), core::mem::offset_of!(Plain, target));
        assert_eq!(off("rot"), core::mem::offset_of!(Plain, rot));
        assert_eq!(off("lanes"), core::mem::offset_of!(Plain, lanes));
        assert!(p.layout.field("_hidden").is_none(), "ignored fields are never touched");
        assert_eq!(p.layout.size, size_of::<Plain>());
        assert!(matches!(&p.layout.field("lanes").unwrap().ty, FieldType::Array { len: 3, .. }));
        assert!(p.writable);
    }

    #[test]
    fn enum_bytes_are_read_from_this_build() {
        let mut w = world();
        fill(&mut w);
        let n = w.resource::<NativeLayouts>();
        let p = n.get("Plain").unwrap();
        let FieldType::Enum { variants, repr: Some(r) } = &p.layout.field("mode").unwrap().ty else { panic!() };
        assert_eq!(variants, &["Idle", "Run", "Fly"]);
        for (i, m) in [Mode::Idle, Mode::Run, Mode::Fly].iter().enumerate() {
            let byte = unsafe { *(m as *const Mode as *const u8) };
            assert_eq!(r.values[i], byte as u32);
        }
        // A fieldless enum component is one field named `value`; defaults stay the default.
        let b = n.get("Body").unwrap();
        let f = b.layout.field("value").unwrap();
        let fixed = Body::Fixed;
        assert_eq!(b.defaults, vec![unsafe { *(&fixed as *const Body as *const u8) }]);
        assert_eq!(f.ty.enum_value(1), Some(b.defaults[0] as u32));
    }

    #[test]
    fn private_fields_and_immutability() {
        let mut w = world();
        fill(&mut w);
        let n = w.resource::<NativeLayouts>();
        // A newtype's field is `value`; a wider tuple struct keeps `0`, `1`.
        assert_eq!(n.get("Private").unwrap().layout.field("value").unwrap().offset, 0);
        assert!(n.get("Private").unwrap().layout.field("0").is_none());
        let pair = &n.get("Pair").unwrap().layout;
        assert_eq!((pair.field("0").unwrap().offset, pair.field("1").unwrap().offset), (0, 4));
        assert!(!n.get("Frozen").unwrap().writable);
    }

    #[test]
    fn heap_and_nested_data_is_reflect_tier_with_a_reason() {
        let mut w = world();
        fill(&mut w);
        let n = w.resource::<NativeLayouts>();
        assert!(n.get("Named").is_none());
        assert!(n.reflect_tier("Named").unwrap().reason.contains("name"), "{:?}", n.reflect_tier("Named"));
        // Known leaves around a hole are still columns; the hole is never exposed, and a value
        // owning heap data behind its holes is never inserted from bytes.
        let m = n.get("Mover").expect("Mover is layout tier with a hole");
        assert!(m.layout.field("speed").is_some() && m.layout.field("dir").is_some());
        assert!(m.layout.field("target").is_none());
        assert_eq!(m.holes.len(), 1);
        assert!(m.insertable, "Option<u8> has no drop glue");
        let w = n.get("WithName").expect("WithName is layout tier");
        assert!(!w.insertable);
    }

    #[test]
    fn time_reads_its_seconds_around_opaque_durations() {
        let mut w = world();
        w.resource::<AppTypeRegistry>().write().register::<Time>();
        fill(&mut w);
        let n = w.resource::<NativeLayouts>();
        let t = n.get("Time<()>").unwrap_or_else(|| panic!("Time is layout tier: {:?}", n.reflect_tier("Time<()>")));
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_millis(250));
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(&time as *const Time as *const u8, size_of::<Time>()) };
        let f = |name: &str| {
            let o = t.layout.field(name).unwrap().offset;
            f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap())
        };
        assert_eq!(f("delta_secs"), 0.25);
        assert_eq!(f("elapsed_secs"), 0.25);
        assert!(t.layout.field("delta").is_none(), "Duration is a hole");
        assert!(t.holes.iter().any(|h| h.starts_with("delta:")));
    }

    #[test]
    fn a_type_without_a_value_is_tiered_from_its_first_row() {
        let mut w = world();
        fill(&mut w);
        assert!(!w.resource::<NativeLayouts>().reflect_tier("NoDefault").unwrap().sampled);
        w.spawn(NoDefault { x: 3.0 });
        retry(&mut w, "NoDefault");
        let n = w.resource::<NativeLayouts>();
        assert_eq!(n.get("NoDefault").unwrap().layout.field("x").unwrap().offset, 0);
    }
}
