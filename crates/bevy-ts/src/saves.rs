//! World saves: `commands.saveWorld(name)` and `commands.loadWorld(name)` (docs/saves.md).
//!
//! One builder: Bevy's `DynamicWorldBuilder` saves every reflected Rust component and resource.
//! TS components have no `TypeId`, so Bevy skips them; each saved entity gets one reflected
//! [`TsSaved`] instead, a map from the TS component's name to its JSON (`value::decode_component`),
//! and TS resources ride in one [`TsSavedResources`]. The file is then exactly
//! `DynamicWorld::serialize`, `<name>.scn.ron`, which Bevy's `WorldAssetLoader` reads.
//!
//! The walk and serialisation run in the op; the write runs on the `IoTaskPool`, to a temporary
//! file renamed over the old one. A load reads and deserialises on the pool, and [`finish_io`]
//! writes the world in a later frame. Outcomes arrive as [`WorldSaved`] and [`WorldLoaded`].

use crate::commands::{TsSpawned, insert_owned};
use crate::components::{Migration, TsComponents};
use crate::layout::{ComponentLayout, FieldType};
use crate::log::{LogLevel, ScriptLog};
use crate::manifest::ComponentDecl;
use crate::value;
use bevy::ecs::component::ComponentId;
use bevy::ecs::entity::{EntityHashMap, EntityHashSet, EntityMapper, SceneEntityMapper};
use bevy::ecs::resource::IsResource;
use bevy::prelude::*;
use bevy::reflect::{ReflectMut, TypeRegistry};
use bevy::tasks::IoTaskPool;
use bevy::world_serialization::serde::WorldDeserializer;
use bevy::world_serialization::{DynamicWorld, DynamicWorldBuilder, WorldFilter};
use serde::Deserialize;
use serde::de::DeserializeSeed;
use serde_json::{Map, Value};
use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Marks an entity a save holds by default. Saved itself, so a loaded entity has it again.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct Save;

/// A saved entity's TS components: name to JSON text. Written into a save; on load, entries whose
/// class is declared become components and leave, the rest stay until their class comes back.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct TsSaved(pub BTreeMap<String, String>);

/// A save's TS resources, as [`TsSaved`] is its entities' TS components.
#[derive(Resource, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Resource, Default)]
pub struct TsSavedResources(pub BTreeMap<String, String>);

/// A save finished: the file is in place, or `error` says why not.
#[derive(Message, Reflect, Clone, Debug, PartialEq)]
pub struct WorldSaved {
    pub name: String,
    pub error: Option<String>,
}

/// A load finished: `entities` are the new entities, or `error` says why there are none.
#[derive(Message, Reflect, Clone, Debug, PartialEq)]
pub struct WorldLoaded {
    pub name: String,
    pub entities: Vec<Entity>,
    pub error: Option<String>,
}

/// The folder saves live in: the player's data folder in a played game, `<project>/.bevy-ts/saves`
/// in dev. Created on the first save.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct SaveDir(pub PathBuf);

/// Components never saved, beside bevy-ts's own bookkeeping. Measured over every entity of
/// `examples/arena` (openspec world-saves task 3.2): ones Bevy computes from others, which come
/// back by themselves, and ones with no serde form in Bevy 0.20, which fail any save.
pub const ALWAYS_DENIED: &[&str] = &[
    // Computed.
    "bevy_transform::components::global_transform::GlobalTransform",
    "bevy_transform::components::transform::TransformTreeChanged",
    "bevy_camera::visibility::InheritedVisibility",
    "bevy_camera::visibility::ViewVisibility",
    "bevy_camera::visibility::VisibleEntities",
    "bevy_camera::visibility::CascadesVisibleEntities",
    "bevy_camera::primitives::Frustum",
    "bevy_camera::primitives::CascadesFrusta",
    "bevy_light::cascade::Cascades",
    "bevy_ui::ui_node::ComputedNode",
    "bevy_ui::ui_node::ComputedUiTargetCamera",
    "bevy_ui::ui_node::ComputedUiRenderTargetInfo",
    "bevy_ui::stack::ComputedStackIndex",
    "bevy_ui::ui_transform::UiGlobalTransform",
    "bevy_ui::measurement::ContentSize",
    "bevy_ui::widget::text::TextNodeFlags",
    "bevy_text::pipeline::TextLayoutInfo",
    "bevy_text::text::ComputedTextBlock",
    // Names observer entities, which a save never holds.
    "bevy_ecs::observer::distributed_storage::ObservedBy",
    // No serde form in Bevy 0.20.
    "bevy_camera::visibility::VisibilityClass",
    "bevy_camera::camera::CameraMainTextureUsages",
    "bevy_render::camera::CameraRenderGraph",
    "bevy_camera::camera::Exposure",
    "bevy_render::view::ColorGrading",
];

/// `saveWorld`'s options, as JS sends them: classes by TS name or Rust type path.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SaveOptions {
    /// `Entity::to_bits` of each entity to save; none saves the `Save`-marked ones.
    pub entities: Option<Vec<u64>>,
    pub allow: Option<Vec<String>>,
    pub deny: Vec<String>,
    pub resources: Vec<String>,
}

/// Whether `name` may name a save: 1 to 64 of `A-Z a-z 0-9 _ -`, so it never leaves the folder.
pub fn check_name(name: &str) -> Result<(), String> {
    let ok = (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok { Ok(()) } else { Err(format!("save name {name:?}: use 1 to 64 of A-Z, a-z, 0-9, _ and -")) }
}

/// The per-user data folder, in the style of `bevy_platform::dirs`: macOS
/// `~/Library/Application Support`, Windows `%APPDATA%`, else `$XDG_DATA_HOME` or `~/.local/share`.
/// `env` looks up a variable, so a test passes one without setting it.
pub fn data_dir(os: &str, env: impl Fn(&str) -> Option<String>, home: Option<&Path>) -> Option<PathBuf> {
    let set = |k: &str| env(k).filter(|v| Path::new(v).is_absolute()).map(PathBuf::from);
    match os {
        "macos" => home.map(|h| h.join("Library/Application Support")),
        "windows" => set("APPDATA").or_else(|| home.map(|h| h.join("AppData").join("Roaming"))),
        _ => set("XDG_DATA_HOME").or_else(|| home.map(|h| h.join(".local/share"))),
    }
}

/// A played game's save folder: `<data>/<game>/saves` on this machine.
pub fn game_save_dir(game: &str) -> Option<PathBuf> {
    let env = |k: &str| std::env::var(k).ok();
    let home = env(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    data_dir(std::env::consts::OS, env, home.as_deref()).map(|d| d.join(game).join("saves"))
}

/// What the `IoTaskPool` hands back to the main world.
enum Done {
    Saved { name: String, error: Option<String> },
    Loaded { name: String, parent: Option<Entity>, world: Result<DynamicWorld, String> },
}

#[derive(Resource)]
struct SaveIo {
    tx: Sender<Done>,
    rx: Mutex<Receiver<Done>>,
}

impl Default for SaveIo {
    fn default() -> Self {
        let (tx, rx) = channel();
        SaveIo { tx, rx: Mutex::new(rx) }
    }
}

/// Registers the types and messages and the system that finishes IO.
pub(crate) fn register(app: &mut App) {
    app.register_type::<Save>().register_type::<TsSaved>().register_type::<TsSavedResources>();
    // A unit marker has no reflected fields, so reflection gives it no layout; an empty one lets TS
    // spawn and insert it (`spawn(Save, ...)`).
    let world = app.world_mut();
    let mut native = world.remove_resource::<crate::layout::NativeLayouts>().unwrap_or_default();
    native.register::<Save>(world, ComponentLayout { size: 0, align: 1, fields: vec![] }, None);
    world.insert_resource(native);
    crate::rust_events::register_message::<WorldSaved>(app);
    crate::rust_events::register_message::<WorldLoaded>(app);
    app.init_resource::<SaveIo>();
    app.add_systems(First, finish_io.after(crate::generation::apply_pending));
}

fn saved(world: &mut World, name: &str, error: Option<String>) {
    world.write_message(WorldSaved { name: name.to_string(), error });
}

fn loaded(world: &mut World, name: &str, entities: Vec<Entity>, error: Option<String>) {
    world.write_message(WorldLoaded { name: name.to_string(), entities, error });
}

fn warn(world: &mut World, message: String) {
    let generation = world.get_resource::<crate::generation::Generations>().map_or(0, |g| g.live);
    if let Some(mut log) = world.get_resource_mut::<ScriptLog>() {
        log.push(LogLevel::Warn, message, None, generation);
    }
}

/// A class named by TS: a TS component or resource by name, or a Rust one by type path.
enum Class {
    Ts(String),
    Rust(TypeId),
}

fn resolve(world: &World, registry: &TypeRegistry, name: &str, resource: bool) -> Result<Class, String> {
    if world.resource::<TsComponents>().get(name).is_some() {
        return Ok(Class::Ts(name.to_string()));
    }
    let r = registry.get_with_type_path(name).or_else(|| registry.get_with_short_type_path(name));
    let reflected = r.filter(|r| if resource { r.data::<ReflectResource>().is_some() } else { r.data::<ReflectComponent>().is_some() });
    match reflected {
        Some(r) => Ok(Class::Rust(r.type_id())),
        None => {
            let kind = if resource { "resource" } else { "component" };
            Err(format!("{name} is neither a reflected Rust {kind} nor a declared TS {kind}"))
        }
    }
}

/// Splits classes into TS names and Rust type ids.
fn resolve_all(world: &World, registry: &TypeRegistry, names: &[String], resource: bool) -> Result<(BTreeSet<String>, Vec<TypeId>), String> {
    let (mut ts, mut rust) = (BTreeSet::new(), Vec::new());
    for n in names {
        match resolve(world, registry, n, resource)? {
            Class::Ts(n) => {
                ts.insert(n);
            }
            Class::Rust(t) => rust.push(t),
        }
    }
    Ok((ts, rust))
}

/// A TS component's bytes on an entity, as JSON text.
fn read_ts(world: &World, e: Entity, id: ComponentId, layout: &ComponentLayout) -> Option<String> {
    let ptr = world.get_by_id(e, id)?;
    // SAFETY: a TS component's column holds `layout.size` bytes per entity.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), layout.size) };
    // SAFETY: a live column value of `layout`.
    Some(unsafe { value::decode_component(layout, bytes) }.to_string())
}

/// `commands.saveWorld(name, options)`: builds the save and starts its write.
pub fn save(world: &mut World, name: &str, options: SaveOptions) {
    match build(world, name, &options) {
        Ok((text, dir)) => {
            let tx = world.resource::<SaveIo>().tx.clone();
            let name = name.to_string();
            IoTaskPool::get()
                .spawn(async move {
                    let error = write(&dir, &name, &text).err();
                    let _ = tx.send(Done::Saved { name, error });
                })
                .detach();
        }
        Err(e) => saved(world, name, Some(e)),
    }
}

/// The save's RON and the folder it goes in.
fn build(world: &mut World, name: &str, options: &SaveOptions) -> Result<(String, PathBuf), String> {
    check_name(name)?;
    let dir = world.get_resource::<SaveDir>().map(|d| d.0.clone()).ok_or("no save folder: SaveDir is not set")?;
    let save_id = world.register_component::<Save>();
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();

    let entities: Vec<Entity> = match &options.entities {
        Some(bits) => bits
            .iter()
            .map(|b| Entity::try_from_bits(*b).filter(|e| world.get_entity(*e).is_ok()).ok_or_else(|| format!("entity {b} does not exist")))
            .filter(|e| e.as_ref().map_or(true, |e| !world.entity(*e).contains::<IsResource>()))
            .collect::<Result<_, _>>()?,
        // Every archetype with `Save`, so disabled entities are saved too, as Bevy's own walk does.
        None => world.archetypes().iter().filter(|a| a.contains(save_id)).flat_map(|a| a.entities().iter().map(|e| e.id())).collect(),
    };

    let (ts_allow, rust_allow) = match &options.allow {
        Some(names) => {
            let (ts, rust) = resolve_all(world, &registry, names, false)?;
            (Some(ts), Some(rust))
        }
        None => (None, None),
    };
    let (ts_deny, rust_deny) = resolve_all(world, &registry, &options.deny, false)?;
    let (ts_resources, rust_resources) = resolve_all(world, &registry, &options.resources, true)?;

    let mut components = match &rust_allow {
        Some(list) => list.iter().fold(WorldFilter::deny_all(), |f, t| f.allow_by_id(*t)).allow::<Save>(),
        None => WorldFilter::allow_all(),
    };
    let always = [TypeId::of::<TsSpawned>(), TypeId::of::<crate::ScriptHidden>(), TypeId::of::<TsSaved>()]
        .into_iter()
        .chain(ALWAYS_DENIED.iter().filter_map(|p| registry.get_with_type_path(p).map(|r| r.type_id())));
    for t in rust_deny.into_iter().chain(always) {
        components = components.deny_by_id(t);
    }
    let resources = rust_resources.iter().fold(WorldFilter::deny_all(), |f, t| f.allow_by_id(*t));

    let mut builder = DynamicWorldBuilder::from_world(world, &registry)
        .with_component_filter(components)
        .with_resource_filter(resources)
        .extract_entities(entities.into_iter());
    if !rust_resources.is_empty() {
        builder = builder.extract_resources();
    }
    let mut dynamic = builder.build();
    cut_hierarchy(&mut dynamic);

    // TS components: live and undeclared, merged over what an earlier load kept. A relationship
    // target is derived from its sources, so it is never saved.
    let ts = world.resource::<TsComponents>();
    let by_id: HashMap<ComponentId, (&str, &ComponentLayout)> = ts.current().filter(|c| !c.decl.is_target()).map(|c| (c.id, (c.decl.name.as_str(), &c.layout))).collect();
    let keep = |n: &str| ts_allow.as_ref().is_none_or(|a| a.contains(n)) && !ts_deny.contains(n);
    for d in &mut dynamic.entities {
        let mut map = world.get::<TsSaved>(d.entity).map(|s| s.0.clone()).unwrap_or_default();
        for id in world.entity(d.entity).archetype().components().iter() {
            if let Some((n, layout)) = by_id.get(id)
                && let Some(json) = read_ts(world, d.entity, *id, layout)
            {
                map.insert(n.to_string(), json);
            }
        }
        map.retain(|n, _| keep(n));
        if !map.is_empty() {
            d.components.push(Box::new(TsSaved(map)));
        }
    }
    // Kept resources of a class that is gone, saved again as kept entities are.
    let mut res = world.get_resource::<TsSavedResources>().map(|r| r.0.clone()).unwrap_or_default();
    for n in &ts_resources {
        let Some(c) = ts.get(n) else { continue };
        if let Some(json) = world.resource_entities().get(c.id).and_then(|e| read_ts(world, e, c.id, &c.layout)) {
            res.insert(n.clone(), json);
        }
    }
    if !res.is_empty() {
        dynamic.resources.push(Box::new(TsSavedResources(res)));
    }
    let text = dynamic.serialize(&registry).map_err(|e| format!("cannot serialise the save: {e}"))?;
    Ok((text, dir))
}

/// Drops a `ChildOf` whose parent is not in the save, and `Children` entries not in it, so an
/// entity saved without its parent loads as a root instead of under an entity that is never alive.
fn cut_hierarchy(dynamic: &mut DynamicWorld) {
    let saved: EntityHashSet = dynamic.entities.iter().map(|d| d.entity).collect();
    let inside = |v: &dyn PartialReflect| Entity::from_reflect(v).is_some_and(|e| saved.contains(&e));
    for d in &mut dynamic.entities {
        d.components.retain(|c| ChildOf::from_reflect(c.as_partial_reflect()).is_none_or(|p| saved.contains(&p.parent())));
        d.components.retain_mut(|c| {
            if !c.represents::<Children>() {
                return true;
            }
            let ReflectMut::TupleStruct(t) = c.reflect_mut() else { return true };
            let Some(ReflectMut::List(list)) = t.field_mut(0).map(|f| f.reflect_mut()) else { return true };
            for i in (0..list.len()).rev() {
                if !list.get(i).is_some_and(inside) {
                    list.remove(i);
                }
            }
            !list.is_empty()
        });
    }
}

/// Each write's own temporary name, so two saves of one name never share a file.
static WRITES: AtomicU64 = AtomicU64::new(0);

/// Writes `<name>.scn.ron` by a temporary file in the same folder renamed over it, so a failed or
/// cut-off write leaves the old save whole. Two writes of one name each rename a whole file: the
/// last rename wins.
fn write(dir: &Path, name: &str, text: &str) -> Result<(), String> {
    use std::io::Write;
    let path = dir.join(format!("{name}.scn.ron"));
    let tmp = dir.join(format!("{name}.scn.ron.{}.tmp", WRITES.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let written = std::fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(text.as_bytes()).and_then(|_| f.sync_all()))
        .map_err(|e| format!("{}: {e}", tmp.display()));
    let renamed = written.and_then(|_| std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display())));
    if renamed.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    renamed
}

/// `commands.loadWorld(name, { parent })`: starts the read; [`finish_io`] writes the world.
pub fn load(world: &mut World, name: &str, parent: Option<Entity>) {
    let start = || -> Result<(PathBuf, AssetServer), String> {
        check_name(name)?;
        let dir = world.get_resource::<SaveDir>().ok_or("no save folder: SaveDir is not set")?;
        let server = world.get_resource::<AssetServer>().ok_or("loading a save needs the AssetServer (AssetPlugin)")?;
        Ok((dir.0.join(format!("{name}.scn.ron")), server.clone()))
    };
    let (path, server) = match start() {
        Ok(x) => x,
        Err(e) => return loaded(world, name, vec![], Some(e)),
    };
    let registry = world.resource::<AppTypeRegistry>().clone();
    let tx = world.resource::<SaveIo>().tx.clone();
    let name = name.to_string();
    IoTaskPool::get()
        .spawn(async move {
            let world = read(&path, &registry.read(), server);
            let _ = tx.send(Done::Loaded { name, parent, world });
        })
        .detach();
}

/// Reads a save with Bevy's `WorldDeserializer`; handles load through the `AssetServer`.
fn read(path: &Path, registry: &TypeRegistry, mut server: AssetServer) -> Result<DynamicWorld, String> {
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("no save at {}", path.display()),
        _ => format!("{}: {e}", path.display()),
    })?;
    let mut de = ron::de::Deserializer::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    WorldDeserializer { type_registry: registry, load_from_path: &mut server }
        .deserialize(&mut de)
        .map_err(|e| format!("{}: {}", path.display(), de.span_error(e)))
}

/// `First`: hands finished saves and loads to the world.
pub fn finish_io(world: &mut World) {
    let done: Vec<Done> = match world.get_resource::<SaveIo>() {
        Some(io) => io.rx.lock().unwrap().try_iter().collect(),
        None => return,
    };
    for d in done {
        match d {
            Done::Saved { name, error } => saved(world, &name, error),
            Done::Loaded { name, parent, world: Ok(dynamic) } => {
                let (entities, error) = apply(world, &dynamic, parent);
                loaded(world, &name, entities, error);
            }
            Done::Loaded { name, world: Err(e), .. } => loaded(world, &name, vec![], Some(e)),
        }
    }
}

/// Writes a read save into the world: fresh entities, then their TS components and resources.
fn apply(world: &mut World, dynamic: &DynamicWorld, parent: Option<Entity>) -> (Vec<Entity>, Option<String>) {
    let kept = world.remove_resource::<TsSavedResources>().map(|r| r.0).unwrap_or_default();
    let mut map = EntityHashMap::default();
    let written = dynamic.write_to_world(world, &mut map);
    let entities: Vec<Entity> = dynamic.entities.iter().filter_map(|d| map.get(&d.entity).copied()).collect();
    if let Err(e) = written {
        if !kept.is_empty() {
            world.insert_resource(TsSavedResources(kept));
        }
        return (entities, Some(e.to_string()));
    }
    let parent = parent.filter(|p| world.get_entity(*p).is_ok());
    for e in &entities {
        let mut em = world.entity_mut(*e);
        em.insert(TsSpawned);
        if let Some(p) = parent
            && !em.contains::<ChildOf>()
        {
            em.insert(ChildOf(p));
        }
    }

    let mut migrations: BTreeMap<String, Migration> = BTreeMap::new();
    let mut unknown = BTreeSet::new();
    let mut failed = Vec::new();
    for e in &entities {
        let Some(TsSaved(entries)) = world.entity_mut(*e).take::<TsSaved>() else { continue };
        let mut left = BTreeMap::new();
        for (name, json) in entries {
            match restore(world, &mut map, &name, &json) {
                Ok((bytes, id, m)) => {
                    let mut em = world.entity_mut(*e);
                    // SAFETY: `restore` encoded the bytes with this live component's layout.
                    unsafe { insert_owned(&mut em, id, bytes) };
                    count(&mut migrations, m);
                }
                Err(e) => {
                    match e {
                        Some(e) => failed.push(format!("load: {name}: {e}; its saved data stays in TsSaved")),
                        None => {
                            unknown.insert(name.clone());
                        }
                    }
                    left.insert(name, json);
                }
            }
        }
        if !left.is_empty() {
            world.entity_mut(*e).insert(TsSaved(left));
        }
    }
    let mut left_resources = kept;
    if let Some(TsSavedResources(entries)) = world.remove_resource::<TsSavedResources>() {
        let left = &mut left_resources;
        for (name, json) in entries {
            match restore(world, &mut map, &name, &json) {
                Ok((bytes, id, m)) => {
                    let mut em = match world.resource_entities().get(id) {
                        Some(e) => world.entity_mut(e),
                        None => world.spawn(IsResource::new(id)),
                    };
                    // SAFETY: as above.
                    unsafe { insert_owned(&mut em, id, bytes) };
                    count(&mut migrations, m);
                }
                Err(e) => {
                    match e {
                        Some(e) => failed.push(format!("load: {name}: {e}; its saved data stays in TsSaved")),
                        None => {
                            unknown.insert(name.clone());
                        }
                    }
                    left.insert(name, json);
                }
            }
        }
    }
    if !left_resources.is_empty() {
        world.insert_resource(TsSavedResources(left_resources));
    }
    for m in migrations.into_values() {
        warn(world, format!("load: {m}"));
    }
    for f in failed {
        warn(world, f);
    }
    for n in unknown {
        warn(world, format!("load: no TS class is named {n}; its saved data stays in TsSaved"));
    }
    (entities, None)
}

fn count(migrations: &mut BTreeMap<String, Migration>, m: Option<Migration>) {
    if let Some(m) = m {
        migrations.entry(m.component.clone()).and_modify(|o| o.entities += 1).or_insert(m);
    }
}

/// One saved TS component or resource, migrated to its live layout and its `Entity` fields
/// remapped: its bytes, its id, and the migration if its fields changed. `Err(None)` when no live
/// class has the name, `Err(Some(why))` when its value does not encode.
fn restore(world: &mut World, map: &mut EntityHashMap<Entity>, name: &str, json: &str) -> Result<(crate::heap::HeapBytes, ComponentId, Option<Migration>), Option<String>> {
    let c = world.resource::<TsComponents>().live(name).ok_or(None)?.clone();
    let saved: Map<String, Value> = match serde_json::from_str(json) {
        Ok(Value::Object(o)) => o,
        _ => Map::new(),
    };
    let (mut values, m) = migrate(&c.decl, &c.layout, &saved);
    SceneEntityMapper::world_scope(map, world, |_, mapper| {
        for f in &c.layout.fields {
            if let Some(v) = values.get_mut(&f.name) {
                map_entities(&f.ty, v, mapper);
            }
        }
    });
    let bytes = world.resource::<TsComponents>().encode(name, &values).map_err(|e| Some(e.to_string()))?;
    Ok((bytes, c.id, m))
}

/// A JSON value's shape: what a field type's values look like, to tell a field whose type
/// changed since the save.
#[derive(PartialEq)]
enum Shape {
    Bool,
    Int,
    Float,
    Text,
    Null,
    List(Vec<Shape>),
    Object(Vec<String>),
}

fn shape(v: &Value) -> Shape {
    match v {
        Value::Bool(_) => Shape::Bool,
        Value::Number(n) if n.is_f64() => Shape::Float,
        Value::Number(_) => Shape::Int,
        Value::String(_) => Shape::Text,
        Value::Null => Shape::Null,
        Value::Array(a) => Shape::List(a.iter().map(shape).collect()),
        Value::Object(o) => Shape::Object(o.keys().cloned().collect()),
    }
}

fn same_shape(saved: &Shape, live: &Shape) -> bool {
    match (saved, live) {
        // A non-finite float decodes as null.
        (Shape::Null, Shape::Float) => true,
        (Shape::List(a), Shape::List(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_shape(x, y)),
        _ => saved == live,
    }
}

/// The hot-reload rules between a saved component's fields and its live layout: a field of the
/// same name and shape is kept, a new one takes its default, one of another type is reset to its
/// default, and one no longer declared is dropped.
fn migrate(decl: &ComponentDecl, layout: &ComponentLayout, saved: &Map<String, Value>) -> (Map<String, Value>, Option<Migration>) {
    let mut m = Migration { component: decl.name.clone(), entities: 1, kept: vec![], added: vec![], reset: vec![], removed: vec![] };
    let mut values = Map::new();
    for f in &layout.fields {
        match saved.get(&f.name) {
            Some(v) => {
                let mut scratch = vec![0u8; f.ty.size()];
                // A heap field is kept when the saved value is one of its type, whatever a list's
                // length; a plain field when its shape matches too.
                let fits = match f.ty.is_heap() {
                    true => value::heap_value("", &f.ty, v).is_ok(),
                    false => same_shape(&shape(v), &shape(&value::zero(&f.ty))) && value::encode_field("", &f.ty, v, &mut scratch).is_ok(),
                };
                if fits {
                    values.insert(f.name.clone(), v.clone());
                    m.kept.push(f.name.clone());
                } else {
                    m.reset.push(f.name.clone());
                }
            }
            None => m.added.push(f.name.clone()),
        }
    }
    m.removed = saved.keys().filter(|k| layout.field(k).is_none()).cloned().collect();
    let changed = !(m.added.is_empty() && m.reset.is_empty() && m.removed.is_empty());
    (values, changed.then_some(m))
}

/// Maps the saved entities in a field's value to the loaded ones.
fn map_entities(ty: &FieldType, v: &mut Value, mapper: &mut SceneEntityMapper) {
    match ty {
        FieldType::Entity => {
            if let Some(e) = v.as_u64().and_then(Entity::try_from_bits)
                && e != Entity::PLACEHOLDER
            {
                *v = Value::from(mapper.get_mapped(e).to_bits());
            }
        }
        FieldType::Array { of, .. } | FieldType::List { of } => {
            if let Value::Array(a) = v {
                for x in a {
                    map_entities(of, x, mapper);
                }
            }
        }
        FieldType::Struct { fields } => {
            if let Value::Object(o) = v {
                for f in fields {
                    if let Some(x) = o.get_mut(&f.name) {
                        map_entities(&f.ty, x, mapper);
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn the_data_folder_per_os() {
        let home = Path::new("/home/p");
        assert_eq!(data_dir("macos", env(&[]), Some(home)), Some(PathBuf::from("/home/p/Library/Application Support")));
        assert_eq!(data_dir("windows", env(&[("APPDATA", "/c/Users/p/AppData/Roaming")]), Some(home)), Some(PathBuf::from("/c/Users/p/AppData/Roaming")));
        assert_eq!(data_dir("linux", env(&[("XDG_DATA_HOME", "/d")]), Some(home)).map(|d| d.join("arena/saves")), Some(PathBuf::from("/d/arena/saves")));
        assert_eq!(data_dir("linux", env(&[]), Some(home)), Some(PathBuf::from("/home/p/.local/share")));
        // A relative XDG_DATA_HOME is ignored, as the spec says.
        assert_eq!(data_dir("linux", env(&[("XDG_DATA_HOME", "d")]), Some(home)), Some(PathBuf::from("/home/p/.local/share")));
        assert_eq!(data_dir("linux", env(&[]), None), None);
    }

    #[test]
    fn a_name_cannot_leave_the_folder() {
        assert!(check_name("slot_1-a").is_ok());
        assert!(check_name(&"a".repeat(64)).is_ok());
        for bad in ["", "..", "../x", "a/b", "a\\b", "/abs", "C:x", "a.b", &"a".repeat(65)] {
            let e = check_name(bad).unwrap_err();
            assert!(e.contains("A-Z, a-z, 0-9, _ and -"), "{bad}: {e}");
        }
    }

    /// An entity whose bits pass 2^53 survives the JSON in `TsSaved` and maps exactly.
    #[test]
    fn a_high_generation_entity_maps_exactly() {
        use bevy::ecs::entity::EntityGeneration;
        let mut world = World::new();
        let old = Entity::from_index_and_generation(Entity::from_raw_u32(3).unwrap().index(), EntityGeneration::FIRST.after_versions(1 << 30));
        assert!(old.to_bits() > 1 << 53);
        let new = world.spawn_empty().id();
        let mut map = EntityHashMap::default();
        map.insert(old, new);
        let text = serde_json::json!({ "target": old.to_bits() }).to_string();
        let mut v: Value = serde_json::from_str::<Value>(&text).unwrap()["target"].take();
        assert_eq!(v.as_u64(), Some(old.to_bits()));
        SceneEntityMapper::world_scope(&mut map, &mut world, |_, mapper| map_entities(&FieldType::Entity, &mut v, mapper));
        assert_eq!(v.as_u64(), Some(new.to_bits()));
    }

    #[test]
    fn entities_in_a_list_and_its_objects_are_remapped() {
        use crate::layout::StructField;
        let mut world = World::new();
        let (old, new) = (Entity::from_bits(40), world.spawn_empty().id());
        let mut map = EntityHashMap::default();
        map.insert(old, new);
        let hit = FieldType::Struct { fields: vec![StructField { name: "by".into(), ty: FieldType::Entity }, StructField { name: "hp".into(), ty: FieldType::F32 }] };
        let (who, hits) = (FieldType::List { of: Box::new(FieldType::Entity) }, FieldType::List { of: Box::new(hit) });
        let mut a = serde_json::json!([old.to_bits()]);
        let mut b = serde_json::json!([{ "by": old.to_bits(), "hp": 1.0 }]);
        SceneEntityMapper::world_scope(&mut map, &mut world, |_, mapper| {
            map_entities(&who, &mut a, mapper);
            map_entities(&hits, &mut b, mapper);
        });
        assert_eq!((a, b), (serde_json::json!([new.to_bits()]), serde_json::json!([{ "by": new.to_bits(), "hp": 1.0 }])));
    }

    #[test]
    fn a_list_of_objects_is_kept_only_while_its_members_match() {
        use crate::layout::StructField;
        use crate::manifest::FieldDecl;
        use serde_json::json;
        let m = |n: &str, ty: FieldType| StructField { name: n.into(), ty };
        let decl = |members: Vec<StructField>| ComponentDecl {
            name: "game::Trail".into(),
            fields: vec![FieldDecl { name: "hits".into(), ty: FieldType::List { of: Box::new(FieldType::Struct { fields: members }) }, default: None, hints: Default::default() }],
            ..Default::default()
        };
        let saved = json!({ "hits": [{ "at": [1, 2, 3], "hp": 2 }] });
        let same = decl(vec![m("at", FieldType::Vec3), m("hp", FieldType::F32)]);
        assert!(migrate(&same, &same.layout(), saved.as_object().unwrap()).1.is_none(), "its members match: kept");
        let added = decl(vec![m("at", FieldType::Vec3), m("hp", FieldType::F32), m("kind", FieldType::U8)]);
        let (values, change) = migrate(&added, &added.layout(), saved.as_object().unwrap());
        assert_eq!((change.unwrap().reset, values.len()), (vec!["hits".to_string()], 0), "a member added resets it");
    }

    #[test]
    fn migration_by_shape() {
        use crate::manifest::FieldDecl;
        use serde_json::json;
        let f = |n: &str, ty: FieldType, d: Option<Value>| FieldDecl { name: n.into(), ty, default: d, hints: Default::default() };
        let decl = ComponentDecl {
            name: "game::Health".into(),
            fields: vec![f("hp", FieldType::F32, None), f("max", FieldType::F32, Some(json!(10))), f("lives", FieldType::I32, Some(json!(3)))],
            ..Default::default()
        };
        let layout = decl.layout();
        let saved = json!({ "hp": 3.0, "lives": 2.5, "shield": true });
        let (values, m) = migrate(&decl, &layout, saved.as_object().unwrap());
        let m = m.unwrap();
        assert_eq!((m.kept, m.added, m.reset, m.removed), (vec!["hp".into()], vec!["max".into()], vec!["lives".into()], vec!["shield".to_string()]));
        assert_eq!(Value::Object(values), json!({ "hp": 3.0 }));
        let (_, none) = migrate(&decl, &layout, json!({ "hp": 1.0, "max": 2.0, "lives": 1 }).as_object().unwrap());
        assert!(none.is_none());
    }
}
