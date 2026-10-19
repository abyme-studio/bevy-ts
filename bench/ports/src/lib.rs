//! What generated port code can do with a third-party component that is not `#[repr(C)]`.
//!
//! Subject: `bevy_rapier3d` 0.37 (Bevy 0.20). `Velocity { pub linear: Vec3, pub angular: Vec3 }`
//! derives `Component, Reflect` and has no `repr`. The passing cases are `#[test]`s below; the
//! failing ones are `compile_fail` doctests here, run by `cargo test --doc`.
//!
//! A private field is refused by `offset_of!` (E0616):
//! ```compile_fail
//! // LockedAxes(u8): a bitflags newtype whose field is private.
//! let _ = core::mem::offset_of!(bevy_rapier3d::prelude::LockedAxes, 0);
//! ```
//!
//! An enum variant's field needs the unstable `offset_of_enum` feature (E0658):
//! ```compile_fail
//! let _ = core::mem::offset_of!(bevy_rapier3d::prelude::CollisionEvent, Started.0);
//! ```
//!
//! glam's SIMD types have no named fields on aarch64 (neon) or x86_64 (sse2): `Quat` and `Vec3A`
//! wrap a vector register and reach `x`/`y`/`z` through `Deref`, which `offset_of!` cannot follow.
//! ```compile_fail
//! let _ = core::mem::offset_of!(bevy_math::Quat, x);
//! ```
//! ```compile_fail
//! let _ = core::mem::offset_of!(bevy_math::Vec3A, y);
//! ```
//!
//! The same nested path through a plain `#[repr(C)]` glam type compiles (see `nested_paths`).

#[cfg(test)]
mod tests {
    use bevy_ecs::prelude::*;
    use bevy_math::{Quat, Vec3};
    use bevy_rapier3d::prelude::{GravityScale, LockedAxes, RigidBody, Velocity};
    use bevy_reflect::{PartialReflect, Reflect, ReflectRef};
    use bevy_transform::components::Transform;
    use core::any::TypeId;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn third_party_non_repr_c_struct() {
        // Public fields of a foreign, non-repr(C) struct: offset_of! is stable since 1.77 and only
        // checks visibility, not repr.
        let linear = offset_of!(Velocity, linear);
        let angular = offset_of!(Velocity, angular);
        println!(
            "Velocity: size {} align {} linear @{linear} angular @{angular}",
            size_of::<Velocity>(),
            align_of::<Velocity>()
        );
        assert_eq!(size_of::<Velocity>(), 24);
        assert_eq!(align_of::<Velocity>(), 4);
        assert!(linear % 4 == 0 && angular % 4 == 0);
        // Tuple structs: the field is named `0`.
        assert_eq!(offset_of!(GravityScale, 0), 0);
    }

    #[test]
    fn nested_paths() {
        // Nested field paths are stable since 1.82.
        let ly = offset_of!(Velocity, linear.y);
        let az = offset_of!(Velocity, angular.z);
        assert_eq!(ly, offset_of!(Velocity, linear) + 4);
        assert_eq!(az, offset_of!(Velocity, angular) + 8);
        let tz = offset_of!(Transform, translation.z);
        let rot = offset_of!(Transform, rotation);
        let sx = offset_of!(Transform, scale.x);
        println!(
            "Transform: size {} align {} translation.z @{tz} rotation @{rot} scale.x @{sx}",
            size_of::<Transform>(),
            align_of::<Transform>()
        );
        // Rust reorders Transform's fields: rotation (align 16 on neon/sse2) comes first.
        assert_eq!(align_of::<Quat>(), 16);
    }

    /// Offsets from reflection alone: the address of each reflected field minus the address of the
    /// value. Works for private fields and for glam's SIMD types, because a derived `Reflect`
    /// hands out references into the value itself.
    fn reflect_leaves(base: *const u8, value: &dyn PartialReflect, path: String, out: &mut Vec<(String, usize, TypeId)>) {
        let here = value as *const dyn PartialReflect as *const u8 as usize - base as usize;
        let leaf = value.try_as_reflect().map(|r| r.type_id());
        match value.reflect_ref() {
            ReflectRef::Struct(s) => {
                for i in 0..s.field_len() {
                    let name = s.name_at(i).unwrap();
                    let p = if path.is_empty() { name.to_string() } else { format!("{path}.{name}") };
                    reflect_leaves(base, s.field_at(i).unwrap(), p, out);
                }
            }
            ReflectRef::TupleStruct(s) => {
                for i in 0..s.field_len() {
                    let p = if path.is_empty() { i.to_string() } else { format!("{path}.{i}") };
                    reflect_leaves(base, s.field(i).unwrap(), p, out);
                }
            }
            _ => out.push((path, here, leaf.unwrap())),
        }
    }

    fn offsets_of<T: Reflect>(value: &T) -> Vec<(String, usize, TypeId)> {
        let mut out = Vec::new();
        reflect_leaves(value as *const T as *const u8, value.as_partial_reflect(), String::new(), &mut out);
        out
    }

    #[test]
    fn reflection_gives_the_same_offsets() {
        let v = Velocity::default();
        let got = offsets_of(&v);
        println!("Velocity via reflection: {got:?}");
        let expect = [
            ("linear.x", offset_of!(Velocity, linear.x)),
            ("linear.y", offset_of!(Velocity, linear.y)),
            ("linear.z", offset_of!(Velocity, linear.z)),
            ("angular.x", offset_of!(Velocity, angular.x)),
            ("angular.y", offset_of!(Velocity, angular.y)),
            ("angular.z", offset_of!(Velocity, angular.z)),
        ];
        for (path, off) in expect {
            let (_, o, ty) = got.iter().find(|(p, _, _)| p == path).unwrap();
            assert_eq!(*o, off, "{path}");
            assert_eq!(*ty, TypeId::of::<f32>());
        }

        // Transform: rotation is a glam Quat, which offset_of! cannot look inside. Reflection can.
        let t = Transform::default();
        let got = offsets_of(&t);
        println!("Transform via reflection: {got:?}");
        let rot = offset_of!(Transform, rotation);
        for (i, axis) in ["x", "y", "z", "w"].iter().enumerate() {
            let (_, o, _) = got.iter().find(|(p, _, _)| *p == format!("rotation.{axis}")).unwrap();
            assert_eq!(*o, rot + 4 * i);
        }
        let (_, o, _) = got.iter().find(|(p, _, _)| p == "scale.y").unwrap();
        assert_eq!(*o, offset_of!(Transform, scale.y));

        // A private field: offset_of! refuses it (doctest above); reflection reaches it.
        let got = offsets_of(&LockedAxes::default());
        println!("LockedAxes via reflection: {got:?}");
        assert_eq!(got, vec![("0".to_string(), 0, TypeId::of::<u8>())]);
    }

    #[test]
    fn fieldless_enum_bytes_are_read_not_assumed() {
        // A fieldless enum without repr has an unspecified representation. Port code reads each
        // variant's byte at startup, so a TS setter only ever writes a byte this build produced.
        assert_eq!(size_of::<RigidBody>(), 1);
        let variants = [
            RigidBody::Dynamic,
            RigidBody::Fixed,
            RigidBody::KinematicPositionBased,
            RigidBody::KinematicVelocityBased,
        ];
        let bytes: Vec<u8> = variants.iter().map(|v| unsafe { *(v as *const RigidBody as *const u8) }).collect();
        println!("RigidBody bytes: {bytes:?}");
        assert_eq!(bytes, vec![0, 1, 2, 3]);
        // Option<Entity> uses Entity's niche: 8 bytes, None is a bit pattern rustc chooses.
        println!("Option<Entity>: size {}", size_of::<Option<Entity>>());
    }

    #[test]
    fn zero_copy_write_through_a_float_view() {
        // What bridge does for Transform, applied to Velocity with offsets from offset_of!:
        // wrap the table column as f32s, write a field at row * stride + offset, read it back
        // through Bevy's own Query. No mirror, no copy, no repr(C).
        let mut world = World::new();
        let ids: Vec<Entity> = (0..1000).map(|_| world.spawn(Velocity::default()).id()).collect();
        let cid = world.component_id::<Velocity>().unwrap();
        let table_id = world.entity(ids[0]).location().table_id;
        let tick = world.change_tick();
        {
            let table = world.storages().tables.get(table_id).unwrap();
            let len = table.entity_count() as usize;
            assert_eq!(len, 1000);
            let col = unsafe { table.get_data_slice_for::<Velocity>(cid).unwrap() };
            let base = col.as_ptr() as *mut u8;
            let stride = size_of::<Velocity>();
            assert_eq!(stride % 4, 0);
            // The "Float32Array" bridge would hand JS: len * stride bytes from the column pointer.
            let f32s = unsafe { core::slice::from_raw_parts_mut(base as *mut f32, len * stride / 4) };
            let ly = offset_of!(Velocity, linear.y) / 4;
            let az = offset_of!(Velocity, angular.z) / 4;
            for row in 0..len {
                f32s[row * stride / 4 + ly] = row as f32;
                f32s[row * stride / 4 + az] = -1.0;
            }
            // And the per-row change tick, as bridge decision 6 does.
            let ticks = table.get_changed_ticks_slice_for(cid).unwrap();
            unsafe { *ticks[7].get() = tick };
        }
        let mut q = world.query::<(Entity, &Velocity)>();
        for (e, v) in q.iter(&world) {
            let row = world.entity(e).location().table_row.index();
            assert_eq!(v.linear.y, row as f32);
            assert_eq!(v.angular.z, -1.0);
        }
        println!("zero-copy write: 1000 rows of rapier Velocity written through an f32 view");
    }
}
