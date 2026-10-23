//! Reflect-tier Rust components, those with no native layout (`Mesh3d`, `Projection`), and the
//! writable holes of layout-tier ones (a light's `color`): inserted from their `Default` and
//! written field by field through `bevy_ts::reflect_value`, so a changed prop writes one field as
//! a controlled field does. A field's value is its JSON over the default's value of that field. A
//! component that is itself an enum takes its whole value as `value`.

use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::*;
use bevy::reflect::{ParsedPath, PartialReflect, ReflectPath, ReflectRef, TypeRegistry};
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
fn set(registry: &TypeRegistry, server: Option<&AssetServer>, target: &mut dyn PartialReflect, base: Option<&dyn PartialReflect>, type_path: &str, field: &str, v: &Value) -> Result<(), String> {
    let s = slot(target, field).map_err(|e| format!("{type_path}: {e}"))?;
    let b = base.and_then(|b| slot_ref(b, field));
    reflect_value::write(registry, server, s, b, v).map_err(|e| format!("{type_path}.{field}: {e}"))
}

fn component<'r>(registry: &'r TypeRegistry, type_path: &str) -> Result<(&'r ReflectComponent, &'r bevy::reflect::TypeRegistration), String> {
    let reg = registry.get_with_type_path(type_path).ok_or_else(|| format!("{type_path} is not registered"))?;
    let rc = reg.data::<ReflectComponent>().ok_or_else(|| format!("{type_path} does not reflect Component"))?;
    Ok((rc, reg))
}

/// A fresh default of the component, the base every write is over.
fn default(reg: &bevy::reflect::TypeRegistration) -> Option<Box<dyn Reflect>> {
    reg.data::<ReflectDefault>().map(|d| d.default())
}

/// Checks `props` against a default of the component without touching the world: the errors a
/// load fails with (unknown fields, variants and keys, bad colours). A component with no default
/// is checked when it is written.
pub fn check(registry: &TypeRegistry, type_path: &str, props: &Map<String, Value>) -> Result<(), String> {
    reflect_value::refuse_computed(type_path)?;
    let (_, reg) = component(registry, type_path)?;
    let Some(mut value) = default(reg) else { return Ok(()) };
    for (f, v) in props {
        set(registry, None, value.as_partial_reflect_mut(), None, type_path, f, v)?;
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
    let mut value = default(reg).ok_or_else(|| format!("{type_path} has no reflected Default, so a level cannot insert it"))?;
    for (f, v) in props {
        set(&registry, server.as_ref(), value.as_partial_reflect_mut(), None, type_path, f, v)?;
    }
    rc.insert(&mut world.entity_mut(e), value.as_partial_reflect(), &registry);
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
    let base = default(reg);
    let mut em = world.entity_mut(e);
    let Some(mut m) = rc.reflect_mut(&mut em) else { return Ok(false) };
    set(&registry, server.as_ref(), m.as_partial_reflect_mut(), base.as_deref().map(|b| b.as_partial_reflect()), type_path, field, v)?;
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
