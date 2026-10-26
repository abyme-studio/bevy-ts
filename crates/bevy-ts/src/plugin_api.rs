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
    /// Every section the generation wrote, so a check can read another's without depending on
    /// the order sections are checked in.
    pub sections: &'a [crate::manifest::Section],
}

/// A manifest section handler. Returning errors fails the whole generation.
pub type SectionHandler = Arc<dyn Fn(&mut World, &SectionInput) -> Result<(), Vec<ScriptError>> + Send + Sync>;

/// A section's apply step: runs once the generation is committed (components live and migrated,
/// systems replaced), before startups and before any system of the generation. Cannot fail.
pub type SectionApply = Arc<dyn Fn(&mut World, &AppliedSection) + Send + Sync>;

/// What a section's apply step receives.
pub struct AppliedSection<'a> {
    pub generation: u64,
    /// False when the generation wrote no such section: the apply step runs for every applied
    /// generation, so a plugin can drop what an earlier one built.
    pub present: bool,
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

/// Called when the game starts, before any `Startup` system runs.
pub type StartHandler = Arc<dyn Fn(&mut World) + Send + Sync>;

#[derive(Resource, Default, Clone)]
pub struct StartHandlers(pub Vec<StartHandler>);

/// Called with the non-module files of the graph (`.wgsl`) that changed when nothing else did.
pub type FileHandler = Arc<dyn Fn(&mut World, &[std::path::PathBuf]) + Send + Sync>;

#[derive(Resource, Default, Clone)]
pub struct FileHandlers(pub Vec<FileHandler>);

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

    /// Called once when the game starts (the first Play after an `--edit` start or a Stop, a step
    /// before it, or the first `Startup` of a game booted in play), before any `Startup` system
    /// and, for a Play, before its `OnEnter(EngineMode::Play)`. Once per game start: a `restart`
    /// reload during play does not call it again.
    fn on_ts_start(&mut self, f: impl Fn(&mut World) + Send + Sync + 'static) -> &mut Self;

    /// Called when files the module graph imports but does not run (`.wgsl`) are saved and no
    /// module changed. No generation starts.
    fn on_ts_files_changed(&mut self, f: impl Fn(&mut World, &[std::path::PathBuf]) + Send + Sync + 'static) -> &mut Self;

    /// Hand-maps asset type `T` for scripts (`assets.add`, `get`, `set`, `T.of` and refs): `read`
    /// gives it as JSON, `write` builds it from JSON, and the built `T` goes whole into
    /// `Assets<T>` at its ref's id. `T` needs no `Reflect`. See [`crate::reflect_asset::register`].
    #[cfg(feature = "reflect")]
    #[track_caller]
    fn ts_hand_map<T: Asset>(&mut self, read: crate::reflect_asset::HandRead<T>, write: crate::reflect_asset::HandWrite<T>) -> &mut Self;
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

    fn on_ts_start(&mut self, f: impl Fn(&mut World) + Send + Sync + 'static) -> &mut Self {
        self.world_mut().get_resource_or_init::<StartHandlers>().0.push(Arc::new(f));
        self
    }

    fn on_ts_files_changed(&mut self, f: impl Fn(&mut World, &[std::path::PathBuf]) + Send + Sync + 'static) -> &mut Self {
        self.world_mut().get_resource_or_init::<FileHandlers>().0.push(Arc::new(f));
        self
    }

    #[cfg(feature = "reflect")]
    #[track_caller]
    fn ts_hand_map<T: Asset>(&mut self, read: crate::reflect_asset::HandRead<T>, write: crate::reflect_asset::HandWrite<T>) -> &mut Self {
        crate::reflect_asset::register(read, write);
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

/// A live TS component's declaration by declared name (`enemies::Grunt`) or `module#export`,
/// with its fields and, for a scene component, its scene props.
pub fn component_decl(world: &World, name_or_id: &str) -> Option<crate::manifest::ComponentDecl> {
    let registry = world.get_resource::<TsComponents>()?;
    registry
        .live(name_or_id)
        .or_else(|| registry.current().find(|c| c.state == crate::components::ComponentState::Live && c.decl.id.as_deref() == Some(name_or_id)))
        .map(|c| c.decl.clone())
}

/// Every live scene component's declaration, sorted by name: what a level may place.
pub fn scene_components(world: &World) -> Vec<crate::manifest::ComponentDecl> {
    let Some(registry) = world.get_resource::<TsComponents>() else { return Vec::new() };
    let mut out: Vec<_> = registry.current().filter(|c| c.state == crate::components::ComponentState::Live && c.decl.scene.is_some()).map(|c| c.decl.clone()).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Encodes a map of field names to values into a live TS component's native bytes, defaults
/// filled in, type errors naming the field. Insert it with `commands::insert_owned`: a component
/// with a heap field owns its values.
pub fn encode_component(world: &World, name: &str, values: &serde_json::Map<String, serde_json::Value>) -> Result<crate::heap::HeapBytes, ValueError> {
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

/// Pushes a message into a TS message type's queue from Rust, by the name the TS class pins
/// (`static path`), field values as JSON. Returns `false` when the live generation declares no
/// such message, so a port can forward its Rust messages only when TS reads them.
pub fn write_message(world: &mut World, name: &str, values: &serde_json::Map<String, serde_json::Value>) -> Result<bool, String> {
    let Some(mut payloads) = world.get_resource_mut::<crate::messages::TsPayloads>() else { return Ok(false) };
    let Some(i) = payloads.message(name) else { return Ok(false) };
    let queue = &mut payloads.messages[i];
    let mut bytes = vec![0u8; queue.layout.size];
    for f in &queue.layout.fields {
        let Some(v) = values.get(&f.name) else { continue };
        crate::value::encode_field(&format!("{name}.{}", f.name), &f.ty, v, &mut bytes[f.offset..f.offset + f.ty.size()])
            .map_err(|e| e.to_string())?;
    }
    if let Some(k) = values.keys().find(|k| queue.layout.field(k).is_none()) {
        return Err(format!("{name} has no field {k}"));
    }
    queue.push(&bytes);
    Ok(true)
}
