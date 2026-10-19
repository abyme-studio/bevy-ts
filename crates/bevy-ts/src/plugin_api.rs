//! The public Rust API feature crates extend bevy-ts through: manifest sections, host-invoked
//! entries, command kinds, component lookup and encoding, reflected component access and
//! restart notification. All of it is engine-neutral.

use crate::backend::{ScriptError, SourceLocation};
use crate::commands::{CommandDecoder, CommandKinds, NativeRecipe, NativeRecipes};
use crate::components::{PendingComponents, TsComponents};
use crate::layout::ComponentLayout;
use crate::runtime::ScriptRuntime;
use crate::value::ValueError;
use bevy::ecs::change_detection::ComponentTicks;
use bevy::ecs::component::ComponentId;
use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::*;
use bevy::reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer};
use serde::de::DeserializeSeed;
use std::collections::HashMap;
use std::sync::Arc;

/// What a section handler receives when a generation is applied.
pub struct SectionInput<'a> {
    pub generation: u64,
    /// The bytes JS wrote under the section's name.
    pub bytes: &'a [u8],
    /// Where in TS the section was (last) written.
    pub location: Option<SourceLocation>,
    /// The generation's components, already registered.
    pub components: &'a PendingComponents,
}

/// A manifest section handler. Returning errors fails the whole generation.
pub type SectionHandler = Arc<dyn Fn(&mut World, &SectionInput) -> Result<(), Vec<ScriptError>> + Send + Sync>;

/// A section's apply step: runs once the generation is committed (components live and migrated,
/// systems replaced), before startups and before any system of the generation. Cannot fail.
pub type SectionApply = Arc<dyn Fn(&mut World, &AppliedSection) + Send + Sync>;

/// What a section's apply step receives.
pub struct AppliedSection<'a> {
    pub generation: u64,
    pub bytes: &'a [u8],
    pub location: Option<SourceLocation>,
}

#[derive(Resource, Default, Clone)]
pub struct SectionHandlers(pub HashMap<String, SectionHandler>);

#[derive(Resource, Default, Clone)]
pub struct SectionAppliers(pub HashMap<String, Vec<SectionApply>>);

/// Called on a restart, between the despawn and the first-time apply.
pub type RestartHandler = Arc<dyn Fn(&mut World) + Send + Sync>;

#[derive(Resource, Default, Clone)]
pub struct RestartHandlers(pub Vec<RestartHandler>);

/// Registration of extension points on the `App`.
pub trait BevyTsAppExt {
    /// Registers a manifest section JS fills with `section(name, value)` from `bevy-ts`.
    fn add_ts_section(
        &mut self,
        name: impl Into<String>,
        handler: impl Fn(&mut World, &SectionInput) -> Result<(), Vec<ScriptError>> + Send + Sync + 'static,
    ) -> &mut Self;

    /// Registers an apply step for a section, run after the generation is committed and before
    /// its startups and systems run. Pair it with [`BevyTsAppExt::add_ts_section`], which
    /// validates (and may reject) the same bytes first.
    fn on_ts_section_applied(
        &mut self,
        name: impl Into<String>,
        apply: impl Fn(&mut World, &AppliedSection) + Send + Sync + 'static,
    ) -> &mut Self;

    /// Registers a command kind JS appends with `ctx.commands.custom(name, bytes)`. Its ops are
    /// applied in order with the built-in ones.
    fn add_ts_command_kind(
        &mut self,
        name: impl Into<String>,
        decoder: impl Fn(&mut World, &[u8]) -> Result<(), String> + Send + Sync + 'static,
    ) -> &mut Self;

    /// Registers a native spawn recipe for `commands.spawnNative(name, params)`.
    fn add_ts_native_recipe(
        &mut self,
        name: impl Into<String>,
        recipe: impl Fn(&mut World, Entity, &serde_json::Value) -> Result<(), String> + Send + Sync + 'static,
    ) -> &mut Self;

    /// Called on `bevy-ts/reload` with `restart: true`, after TS-spawned entities are despawned
    /// and before the generation is applied as if first.
    fn on_ts_restart(&mut self, f: impl Fn(&mut World) + Send + Sync + 'static) -> &mut Self;
}

impl BevyTsAppExt for App {
    fn add_ts_section(
        &mut self,
        name: impl Into<String>,
        handler: impl Fn(&mut World, &SectionInput) -> Result<(), Vec<ScriptError>> + Send + Sync + 'static,
    ) -> &mut Self {
        self.world_mut().get_resource_or_init::<SectionHandlers>().0.insert(name.into(), Arc::new(handler));
        self
    }

    fn on_ts_section_applied(
        &mut self,
        name: impl Into<String>,
        apply: impl Fn(&mut World, &AppliedSection) + Send + Sync + 'static,
    ) -> &mut Self {
        self.world_mut().get_resource_or_init::<SectionAppliers>().0.entry(name.into()).or_default().push(Arc::new(apply));
        self
    }

    fn add_ts_command_kind(
        &mut self,
        name: impl Into<String>,
        decoder: impl Fn(&mut World, &[u8]) -> Result<(), String> + Send + Sync + 'static,
    ) -> &mut Self {
        let decoder: CommandDecoder = Arc::new(decoder);
        self.world_mut().get_resource_or_init::<CommandKinds>().register(name, decoder);
        self
    }

    fn add_ts_native_recipe(
        &mut self,
        name: impl Into<String>,
        recipe: impl Fn(&mut World, Entity, &serde_json::Value) -> Result<(), String> + Send + Sync + 'static,
    ) -> &mut Self {
        let recipe: NativeRecipe = Arc::new(recipe);
        self.world_mut().get_resource_or_init::<NativeRecipes>().register(name, recipe);
        self
    }

    fn on_ts_restart(&mut self, f: impl Fn(&mut World) + Send + Sync + 'static) -> &mut Self {
        self.world_mut().get_resource_or_init::<RestartHandlers>().0.push(Arc::new(f));
        self
    }
}

/// Calls a named JS entry of the live generation with a payload, from an exclusive system on
/// the main thread. Returns the entry's bytes; its commands are applied like a system's.
pub fn call_entry(world: &mut World, name: &str, payload: &[u8]) -> Result<Vec<u8>, ScriptError> {
    ScriptRuntime::call_entry(world, name, payload)
}

/// A live TS component's id and layout by declared name.
pub fn component_by_name(world: &World, name: &str) -> Option<(ComponentId, ComponentLayout)> {
    world.get_resource::<TsComponents>()?.live(name).map(|c| (c.id, c.layout.clone()))
}

/// Encodes a map of field names to values into a live TS component's native bytes, defaults
/// filled in, type errors naming the field.
pub fn encode_component(world: &World, name: &str, values: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<u8>, ValueError> {
    world.resource::<TsComponents>().encode(name, values)
}

/// A component's change ticks on an entity, by id. Cheap: no reflection.
pub fn change_ticks(world: &World, entity: Entity, id: ComponentId) -> Option<ComponentTicks> {
    world.get_entity(entity).ok()?.get_change_ticks_by_id(id)
}

fn reflect_component<'a>(
    registry: &'a bevy::reflect::TypeRegistry,
    type_path: &str,
) -> Result<(&'a bevy::reflect::TypeRegistration, &'a ReflectComponent), String> {
    let reg = registry.get_with_type_path(type_path).ok_or_else(|| format!("{type_path} is not a registered type"))?;
    let rc = reg.data::<ReflectComponent>().ok_or_else(|| format!("{type_path} does not reflect Component"))?;
    Ok((reg, rc))
}

/// Reads a reflected Rust component on an entity as JSON, by type path.
pub fn read_reflected(world: &World, entity: Entity, type_path: &str) -> Result<serde_json::Value, String> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let (_, rc) = reflect_component(&registry, type_path)?;
    let e = world.get_entity(entity).map_err(|e| e.to_string())?;
    let value = rc.reflect(e).ok_or_else(|| format!("entity {entity} has no {type_path}"))?;
    let ser = TypedReflectSerializer::new(value.as_partial_reflect(), &registry);
    serde_json::to_value(ser).map_err(|e| e.to_string())
}

/// Writes a reflected Rust component on an entity from JSON, by type path. Marks it changed.
pub fn write_reflected(world: &mut World, entity: Entity, type_path: &str, value: &serde_json::Value) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let (reg, rc) = reflect_component(&registry, type_path)?;
    let de = TypedReflectDeserializer::new(reg, &registry);
    let reflected = de.deserialize(value).map_err(|e| e.to_string())?;
    let mut e = world.get_entity_mut(entity).map_err(|e| e.to_string())?;
    rc.insert(&mut e, reflected.as_partial_reflect(), &registry);
    Ok(())
}
