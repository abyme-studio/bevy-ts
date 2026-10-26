//! Reflect-tier Rust components, those with no native layout (`Mesh3d`, `Projection`), and the
//! writable holes of layout-tier ones (a light's `color`): inserted from their `Default` and
//! written field by field through `bevy_ts::reflect_value`, so a changed prop writes one field as
//! a controlled field does. A field's value is its JSON over the default's value of that field. A
//! component that is itself an enum takes its whole value as `value`. An immutable component
//! (`TilemapChunk`) cannot be written in place: a changed prop inserts it again, which runs its
//! insert hook.

use bevy::ecs::reflect::{ReflectComponent, ReflectFromWorld};
use bevy::prelude::*;
use bevy::reflect::{ParsedPath, PartialReflect, ReflectPath, ReflectRef, TypeRegistry};
use bevy_ts::reflect_asset::{ScriptAssets, Slots};
use bevy_ts::reflect_value;
use serde_json::{Map, Value};

/// The reflection path a TS prop names: `value` is the component's single value, so the whole
/// value of an enum component (reflect-enums-color) and `.0` of a newtype (native-components);
/// levels write with reflection paths, so `0` stays valid. `None` is the whole value.
fn reflect_path(target: &dyn PartialReflect, field: &str) -> Option<String> {
    if field == "value" {
        match target.reflect_ref() {
            ReflectRef::Enum(_) => return None,
            ReflectRef::TupleStruct(s) if s.field_len() == 1 => return Some(".0".into()),
            _ => {}
        }
    }
    Some(format!(".{field}"))
}

/// The slot `field` names in `target`.
fn slot<'t>(target: &'t mut dyn PartialReflect, field: &str) -> Result<&'t mut dyn PartialReflect, String> {
    let Some(path) = reflect_path(target, field) else { return Ok(target) };
    let path = ParsedPath::parse(&path).map_err(|_| format!("no field {field}"))?;
    path.reflect_element_mut(target).map_err(|_| format!("no field {field}"))
}

fn slot_ref<'t>(target: &'t dyn PartialReflect, field: &str) -> Option<&'t dyn PartialReflect> {
    let Some(path) = reflect_path(target, field) else { return Some(target) };
    ParsedPath::parse(&path).ok()?.reflect_element(target).ok()
}

/// Writes `v` into `field` of `target`, over `base`'s value of that field (the slot's own when
/// there is no default).
fn set(slots: &Slots, target: &mut dyn PartialReflect, base: Option<&dyn PartialReflect>, type_path: &str, field: &str, v: &Value) -> Result<(), String> {
    // A `texture_atlas` whose layout is a `fromGrid` value: its index is checked against the grid.
    if field == "texture_atlas" {
        bevy_ts::atlas_layout::check_index(v, field).map_err(|e| format!("{type_path}.{e}"))?;
    }
    if field == "tileset" && type_path == TILEMAP_CHUNK {
        check_tileset(v)?;
    }
    let s = slot(target, field).map_err(|e| format!("{type_path}: {e}"))?;
    let b = base.and_then(|b| slot_ref(b, field));
    reflect_value::write_with(slots.cx(), s, b, v).map_err(|e| format!("{type_path}.{field}: {e}"))
}

const TILEMAP_CHUNK: &str = "bevy_sprite_render::tilemap_chunk::TilemapChunk";

/// A `TilemapChunk`'s tileset is an array texture, one layer per tile: a path loads as one 2D
/// image, which its material cannot sample, so a path needs `Image.load`'s `array_layout`. A ref
/// is the script's own image, taken as it is.
fn check_tileset(v: &Value) -> Result<(), String> {
    let layered = v.get("settings").and_then(|s| s.get("array_layout")).is_some();
    let path = v.as_str().or_else(|| v.get("$load").and_then(Value::as_str));
    match path {
        Some(p) if !layered => Err(format!(
            "TilemapChunk.tileset is an array texture: give Image.load({p:?}, {{ array_layout: {{ RowCount: {{ rows }} }} }}) from bevy-ts/bevy, one layer per tile, not {v}"
        )),
        _ => Ok(()),
    }
}

/// The asset hook for writes into the world: refs and asset values in `Handle<T>` slots.
fn slots_of(world: &World) -> ScriptAssets {
    world.get_resource::<ScriptAssets>().cloned().unwrap_or_default()
}

fn component<'r>(registry: &'r TypeRegistry, type_path: &str) -> Result<(&'r ReflectComponent, &'r bevy::reflect::TypeRegistration), String> {
    let reg = registry.get_with_type_path(type_path).ok_or_else(|| format!("{type_path} is not registered"))?;
    let rc = reg.data::<ReflectComponent>().ok_or_else(|| format!("{type_path} does not reflect Component"))?;
    Ok((rc, reg))
}

/// A fresh default of the component, the base every write is over: its `ReflectDefault`, else
/// its `ReflectFromWorld` run on an empty world (`reflect_value::default_of`).
fn default(registry: &TypeRegistry, reg: &bevy::reflect::TypeRegistration) -> Option<Box<dyn PartialReflect>> {
    reflect_value::default_of(registry, reg.type_id())
}

/// [`default`], else a `FromWorld` that needs the world's resources run on the live world, as a
/// script's `C.of(...)` is. A `FromWorld` that panics there fails the insert, naming the type.
fn live_default(world: &mut World, registry: &TypeRegistry, reg: &bevy::reflect::TypeRegistration, type_path: &str) -> Result<Box<dyn PartialReflect>, String> {
    if let Some(v) = default(registry, reg) {
        return Ok(v);
    }
    let fw = reg
        .data::<ReflectFromWorld>()
        .ok_or_else(|| format!("{type_path} has neither a reflected Default nor a reflected FromWorld, so a level cannot insert it"))?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fw.from_world(world)))
        .map(<dyn Reflect>::into_partial_reflect)
        .map_err(|_| format!("{type_path}'s FromWorld panicked"))
}

/// Checks `props` against a default of the component without touching the world: the errors a
/// load fails with (unknown fields, variants and keys, bad colours). A component with no default
/// is checked when it is written.
pub fn check(registry: &TypeRegistry, type_path: &str, props: &Map<String, Value>) -> Result<(), String> {
    reflect_value::refuse_computed(type_path)?;
    let (_, reg) = component(registry, type_path)?;
    let Some(mut value) = default(registry, reg) else { return Ok(()) };
    let assets = ScriptAssets::default();
    let slots = Slots { registry, server: None, assets: &assets, world: None, check: true };
    for (f, v) in props {
        set(&slots, value.as_mut(), None, type_path, f, v)?;
    }
    Ok(())
}

/// Inserts the component from its default with `props` set.
pub fn insert(world: &mut World, e: Entity, type_path: &str, props: &Map<String, Value>) -> Result<(), String> {
    reflect_value::refuse_computed(type_path)?;
    let registry = world.resource::<AppTypeRegistry>().clone();
    let server = world.get_resource::<AssetServer>().cloned();
    let registry = registry.read();
    let (rc, reg) = component(&registry, type_path)?;
    let mut value = live_default(world, &registry, reg, type_path)?;
    let assets = slots_of(world);
    let slots = Slots { registry: &registry, server: server.as_ref(), assets: &assets, world: Some(world), check: false };
    for (f, v) in props {
        set(&slots, value.as_mut(), None, type_path, f, v)?;
    }
    rc.insert(&mut world.entity_mut(e), value.as_ref(), &registry);
    // Asset values the props made go in now: no run ends after a TSX write.
    bevy_ts::reflect_asset::flush(world);
    Ok(())
}

/// Writes one field of the entity's component over the default's value of it; false when the
/// entity no longer has the component.
pub fn write(world: &mut World, e: Entity, type_path: &str, field: &str, v: &Value) -> Result<bool, String> {
    reflect_value::refuse_computed(type_path)?;
    let registry = world.resource::<AppTypeRegistry>().clone();
    let server = world.get_resource::<AssetServer>().cloned();
    let registry = registry.read();
    let (rc, reg) = component(&registry, type_path)?;
    let base = default(&registry, reg);
    let assets = slots_of(world);
    let slots = Slots { registry: &registry, server: server.as_ref(), assets: &assets, world: None, check: false };
    let immutable = world.components().get_id(reg.type_id()).and_then(|id| world.components().get_info(id)).is_some_and(|i| !i.mutable());
    if immutable {
        let Some(current) = rc.reflect(world.entity(e)) else { return Ok(false) };
        let mut value = current.reflect_clone().map_err(|err| format!("{type_path} is immutable and cannot be cloned to write {field}: {err}"))?.into_partial_reflect();
        set(&slots, value.as_mut(), base.as_deref(), type_path, field, v)?;
        rc.insert(&mut world.entity_mut(e), value.as_ref(), &registry);
        bevy_ts::reflect_asset::flush(world);
        return Ok(true);
    }
    {
        let mut em = world.entity_mut(e);
        let Some(mut m) = rc.reflect_mut(&mut em) else { return Ok(false) };
        set(&slots, m.as_partial_reflect_mut(), base.as_deref(), type_path, field, v)?;
    }
    bevy_ts::reflect_asset::flush(world);
    Ok(true)
}

/// Removes the component.
pub fn remove(world: &mut World, e: Entity, type_path: &str) {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    if let Ok((rc, _)) = component(&registry, type_path) {
        rc.remove(&mut world.entity_mut(e));
    }
}
