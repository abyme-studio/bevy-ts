use bevy::prelude::*;
use bevy_ts::layout::{FieldType, NativeLayouts, NativeSource};
use bevy_ts_native::prelude::*;
use core::mem::offset_of;

#[derive(Component, NativeLayout)]
pub struct Probe {
    pub fuel: f32,
    pub heading: Vec3,
    pub lit: bool,
    pub target: Entity,
    pub lanes: [u16; 2],
    pub spin: Quat,
}

#[derive(Component, NativeLayout)]
pub struct Meters(pub f32);

#[test]
fn offsets_come_from_offset_of_without_repr_c() {
    let l = Probe::native_layout();
    assert_eq!(l.size, size_of::<Probe>());
    let off = |n: &str| l.field(n).unwrap().offset;
    assert_eq!(off("fuel"), offset_of!(Probe, fuel));
    assert_eq!(off("heading"), offset_of!(Probe, heading));
    assert_eq!(off("lit"), offset_of!(Probe, lit));
    assert_eq!(off("target"), offset_of!(Probe, target));
    assert_eq!(off("spin"), offset_of!(Probe, spin));
    assert_eq!(l.field("lanes").unwrap().ty, FieldType::Array { of: Box::new(FieldType::U16), len: 2 });
    assert_eq!(Meters::native_layout().field("0").unwrap().offset, 0);
}

#[test]
fn ts_layout_registers_a_column_reflection_does_not_replace() {
    let mut app = App::new();
    app.ts_layout::<Probe>();
    bevy_ts::layout_reflect::fill(app.world_mut());
    let n = app.world().resource::<NativeLayouts>();
    let p = n.get("Probe").expect("registered");
    assert_eq!(p.source, NativeSource::OffsetOf);
    assert!(p.writable && p.insertable);
}
