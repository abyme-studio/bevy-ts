//! Reflect-tier Rust components, those with no native layout (`Mesh3d`, `SceneRoot`): inserted
//! from their `Default` and written field by field through reflection, so a changed prop writes
//! one field as a controlled field does. A string in a `Handle<T>` field is an asset path, loaded
//! as a `Handle<T>` (levels decision 12; 0.20 registers no `ReflectConvert` for handles).

use bevy::asset::ReflectHandle;
use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::*;
use bevy::reflect::serde::TypedReflectDeserializer;
use bevy::reflect::{ParsedPath, PartialReflect, ReflectPath, TypeRegistry};
use serde::de::DeserializeSeed;
use serde_json::{Map, Value};

/// A field's value for its type: a handle loaded from a path, or the JSON read by reflection.
fn field_value(registry: &TypeRegistry, server: &AssetServer, current: &dyn PartialReflect, v: &Value) -> Result<Box<dyn PartialReflect>, String> {
    let info = current.get_represented_type_info().ok_or("a field with no type info")?;
    let reg = registry.get(info.type_id()).ok_or_else(|| format!("{} is not registered", info.type_path()))?;
    if let Some(h) = reg.data::<ReflectHandle>() {
        let path = v.as_str().ok_or_else(|| format!("{}: expected an asset path, got {v}", info.type_path()))?;
        let untyped = server.load_builder().load_erased(h.asset_type_id(), path.to_string());
        return Ok(h.typed(untyped).into_partial_reflect());
    }
    TypedReflectDeserializer::new(reg, registry).deserialize(v.clone()).map_err(|e| format!("{}: {e}", info.type_path()))
}

fn set(registry: &TypeRegistry, server: &AssetServer, target: &mut dyn PartialReflect, field: &str, v: &Value) -> Result<(), String> {
    let path = ParsedPath::parse(&format!(".{field}")).map_err(|_| format!("no field {field}"))?;
    let slot = path.reflect_element_mut(target).map_err(|_| format!("no field {field}"))?;
    let value = field_value(registry, server, slot, v)?;
    slot.try_apply(&*value).map_err(|e| format!("{field}: {e}"))
}

fn component<'r>(registry: &'r TypeRegistry, type_path: &str) -> Result<(&'r ReflectComponent, &'r bevy::reflect::TypeRegistration), String> {
    let reg = registry.get_with_type_path(type_path).ok_or_else(|| format!("{type_path} is not registered"))?;
    let rc = reg.data::<ReflectComponent>().ok_or_else(|| format!("{type_path} does not reflect Component"))?;
    Ok((rc, reg))
}

/// Inserts the component from its default with `props` set.
pub fn insert(world: &mut World, e: Entity, type_path: &str, props: &Map<String, Value>) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let server = world.resource::<AssetServer>().clone();
    let registry = registry.read();
    let (rc, reg) = component(&registry, type_path)?;
    let default = reg.data::<ReflectDefault>().ok_or_else(|| format!("{type_path} has no reflected Default, so a level cannot insert it"))?;
    let mut value = default.default();
    for (f, v) in props {
        set(&registry, &server, value.as_partial_reflect_mut(), f, v)?;
    }
    rc.insert(&mut world.entity_mut(e), value.as_partial_reflect(), &registry);
    Ok(())
}

/// Writes one field of the entity's component; false when the entity no longer has it.
pub fn write(world: &mut World, e: Entity, type_path: &str, field: &str, v: &Value) -> Result<bool, String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let server = world.resource::<AssetServer>().clone();
    let registry = registry.read();
    let (rc, _) = component(&registry, type_path)?;
    let mut em = world.entity_mut(e);
    let Some(mut m) = rc.reflect_mut(&mut em) else { return Ok(false) };
    set(&registry, &server, m.as_partial_reflect_mut(), field, v)?;
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
