//! Levels in the engine: the `BsnLevel` asset and its loader, level roots reconciled through
//! `tsx` as external roots named `level:<path>`, the world's types as a [`Schema`], and the
//! `LevelInstance` component on every level-placed entity.

use crate::compose::{self, ComponentSchema, Composed, ExpandRequest, FieldSchema, Files, Frame, ScenePlacement, SceneSchema, Schema, ValueType};
use crate::syntax::{self, EntityList, Entry, Extension, ParsedFile};
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoadFailedEvent, AssetLoader, LoadContext, ReflectHandle, UntypedAssetLoadFailedEvent};
use bevy::reflect::serde::TypedReflectSerializer;
use bevy::reflect::enums::VariantInfo;
use bevy::reflect::{ParsedPath, ReflectPath, TypeInfo, TypeRegistry};
use serde_json::Value;
use std::any::TypeId;
use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use bevy_ts::components::TsComponents;
use bevy_ts::layout::NativeLayouts;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts::plugin_api::BevyTsAppExt;
use bevy_ts::value;
use bevy_ts_tsx::desc::{ComponentKind, NodeDesc, node_id};
use bevy_ts_tsx::{TsxNode, TsxTree};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The manifest section `app.level` writes.
pub const SECTION: &str = "levels";

/// A parsed `.bsn` file.
#[derive(Asset, TypePath, Debug)]
pub struct BsnLevel {
    #[dependency]
    pub includes: Vec<Handle<BsnLevel>>,
    pub parsed: Arc<ParsedFile>,
    /// FNV-1a of the file's bytes: the writer's base hash.
    pub hash: u64,
}

impl BsnLevel {
    pub fn extensions(&self) -> BTreeSet<Extension> {
        self.parsed.extensions.iter().map(|u| u.extension).collect()
    }
}

/// FNV-1a, 64 bit, of a file's bytes.
pub fn content_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[derive(Debug)]
pub struct BsnLoadError(pub String);

impl std::fmt::Display for BsnLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BsnLoadError {}

#[derive(Default, TypePath)]
pub struct BsnLoader;

/// The inclusion paths a file names, as written.
pub fn includes_of(list: &EntityList, out: &mut Vec<String>) {
    for e in &list.items {
        for entry in &e.entries {
            match entry {
                Entry::Include(i) => out.push(i.path.clone()),
                Entry::Related(r) => includes_of(&r.list, out),
                _ => {}
            }
        }
    }
}

impl AssetLoader for BsnLoader {
    type Asset = BsnLevel;
    type Settings = ();
    type Error = BsnLoadError;

    async fn load(&self, reader: &mut dyn Reader, _: &(), cx: &mut LoadContext<'_>) -> Result<BsnLevel, BsnLoadError> {
        let path = cx.path().path().to_string_lossy().replace('\\', "/");
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(|e| BsnLoadError(format!("{path}: {e}")))?;
        let src = String::from_utf8(bytes).map_err(|_| BsnLoadError(format!("{path}: not UTF-8")))?;
        let parsed = syntax::parse(&src).map_err(|d| BsnLoadError(d.render(&path, &src)))?;
        let mut paths = Vec::new();
        includes_of(&parsed.entities, &mut paths);
        let includes = paths.iter().map(|p| cx.load::<BsnLevel>(compose::resolve_path(&path, p))).collect();
        Ok(BsnLevel { includes, hash: content_hash(src.as_bytes()), parsed: Arc::new(parsed) })
    }

    fn extensions(&self) -> &[&str] {
        &["bsn"]
    }
}

/// On every entity a level placed or overrode: where it comes from (viewport's D5).
#[derive(Component, Clone, Debug, PartialEq, Serialize)]
pub struct LevelInstance {
    /// The level root it belongs to.
    pub level: String,
    /// Its name path from the level's top.
    #[serde(rename = "namePath")]
    pub name_path: Vec<String>,
    /// Every level frame that could hold an edit, outermost first.
    pub owners: Vec<OwnerFrame>,
    /// A scene component instance's props as placed.
    #[serde(skip)]
    pub scene: Option<ScenePlacement>,
    /// Written in the level's own file, rather than coming from a prefab, an included level or
    /// (for a sidecar) the scene's TSX.
    #[serde(skip)]
    pub written: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OwnerFrame {
    pub file: String,
    #[serde(rename = "namePath")]
    pub name_path: Vec<String>,
    pub line: u32,
}

impl From<&Frame> for OwnerFrame {
    fn from(f: &Frame) -> Self {
        OwnerFrame { file: f.file.clone(), name_path: f.path.clone(), line: f.at.line }
    }
}

/// Who opened a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Opener {
    /// `app.level`: a generation without it closes it.
    Ts,
    /// `add_level`.
    Rust,
    /// `bevy-ts/level_open`.
    Editor,
    /// A scene's sidecar.
    Sidecar,
}

/// One open level.
#[derive(Debug)]
pub struct LevelRoot {
    pub handle: Handle<BsnLevel>,
    pub opener: Opener,
    /// The level and every file it includes, as of its last successful compose.
    pub files: BTreeSet<String>,
    /// Hashes of those files when last applied.
    pub applied: BTreeMap<String, u64>,
    pub owners: BTreeMap<Vec<String>, Vec<Frame>>,
    pub error: Option<ScriptError>,
    pub warnings: Vec<ScriptError>,
    /// Waiting to be composed and reconciled.
    pub dirty: bool,
    /// Asset paths the level's handle fields name (label dropped), with where.
    pub assets: BTreeMap<String, bevy_ts::backend::SourceLocation>,
    /// A sidecar level (decision 11): layered over this `app.scene` root instead of mounted.
    pub overlay: Option<String>,
}

/// Open levels, by asset path.
#[derive(Resource, Debug)]
pub struct Levels {
    pub roots: BTreeMap<String, LevelRoot>,
    /// Where asset paths are on disk, for the writer: Bevy's default `assets` folder unless set.
    pub asset_root: std::path::PathBuf,
    /// The project's `levels.prefabEdits`.
    pub prefab_edits: crate::route::PrefabEdits,
    /// `level+watch` messages by sequence number, the latest 256.
    pub events: std::collections::VecDeque<(u64, Value)>,
    pub seq: u64,
    /// Recently seen versions of level files by content hash, so a write made against an older
    /// version can tell which fields the disk changed since (the latest 32).
    pub history: std::collections::VecDeque<(u64, Arc<ParsedFile>)>,
    /// The levels the live generation's `app.level` declares.
    pub declared: Vec<String>,
    /// Levels `level_close` closed: a generation does not reopen them until `level_open` does.
    pub closed: BTreeSet<String>,
}

impl Default for Levels {
    fn default() -> Self {
        Levels { roots: BTreeMap::new(), asset_root: bevy::asset::io::file::FileAssetReader::get_base_path().join("assets"), prefab_edits: Default::default(), events: Default::default(), seq: 0, history: Default::default(), declared: Vec::new(), closed: BTreeSet::new() }
    }
}

impl Levels {
    /// Records a `level+watch` message.
    pub fn emit(&mut self, message: Value) {
        self.seq += 1;
        self.events.push_back((self.seq, message));
        while self.events.len() > 256 {
            self.events.pop_front();
        }
    }

    /// Remembers a version of a file.
    pub fn remember(&mut self, hash: u64, parsed: Arc<ParsedFile>) {
        if self.history.iter().any(|(h, _)| *h == hash) {
            return;
        }
        self.history.push_back((hash, parsed));
        while self.history.len() > 32 {
            self.history.pop_front();
        }
    }

    /// A remembered version by hash.
    pub fn version(&self, hash: u64) -> Option<Arc<ParsedFile>> {
        self.history.iter().find(|(h, _)| *h == hash).map(|(_, p)| p.clone())
    }

    /// Every level that is, or includes, `file`.
    pub fn affected(&self, file: &str) -> Vec<String> {
        self.roots.iter().filter(|(p, r)| p.as_str() == file || r.files.contains(file)).map(|(p, _)| p.clone()).collect()
    }
}

/// The tsx root of a level.
pub fn root_name(path: &str) -> String {
    format!("level:{path}")
}

/// Adds levels to an app with [`bevy_ts::BevyTsPlugin`] and `TsxPlugin`, after both.
pub struct LevelsPlugin;

/// `app.add_level(path)` from Rust; `app.level(path)` from TS.
pub trait LevelsAppExt {
    fn add_level(&mut self, path: impl Into<String>) -> &mut Self;
}

impl LevelsAppExt for App {
    fn add_level(&mut self, path: impl Into<String>) -> &mut Self {
        let path = path.into();
        self.world_mut().get_resource_or_init::<PendingLevels>().0.push(path);
        self
    }
}

#[derive(Resource, Default)]
struct PendingLevels(Vec<String>);

#[derive(Debug, Default, Serialize, Deserialize)]
struct LevelSection {
    #[serde(default)]
    levels: Vec<String>,
}

fn open(world: &mut World, path: &str, opener: Opener) {
    open_as(world, path, opener, None);
}

fn open_as(world: &mut World, path: &str, opener: Opener, overlay: Option<String>) {
    if world.resource::<Levels>().roots.contains_key(path) {
        return;
    }
    let handle = world.resource::<AssetServer>().load::<BsnLevel>(path.to_string());
    let mut levels = world.resource_mut::<Levels>();
    levels.emit(serde_json::json!({ "event": "opened", "level": path, "opener": opener }));
    levels.roots.insert(
        path.to_string(),
        LevelRoot {
            handle,
            opener,
            files: BTreeSet::new(),
            applied: BTreeMap::new(),
            owners: BTreeMap::new(),
            error: None,
            warnings: Vec::new(),
            dirty: true,
            assets: BTreeMap::new(),
            overlay,
        },
    );
}

/// Opens `scene`'s sidecar level at `path`, layered over the scene (decision 11).
pub fn open_sidecar(world: &mut World, scene: &str, path: &str) {
    open_as(world, path, Opener::Sidecar, Some(scene.to_string()));
}

/// Closes a level: its entities despawn (a sidecar's overrides leave its scene).
pub fn close(world: &mut World, path: &str) {
    let removed = world.resource_mut::<Levels>().roots.remove(path);
    if let Some(r) = removed {
        world.resource_mut::<Levels>().emit(serde_json::json!({ "event": "closed", "level": path }));
        match r.overlay {
            Some(scene) => {
                bevy_ts_tsx::clear_overlay(world, &scene);
            }
            None => {
                bevy_ts_tsx::unmount_external(world, &root_name(path));
            }
        }
    }
}

/// Opens the sidecar level of every scene that has one on disk, but one the editor closed.
fn open_sidecars(world: &mut World) {
    let scenes: Vec<String> = world.resource::<TsxTree>().scenes.keys().cloned().collect();
    for scene in scenes {
        let Some(path) = bevy_ts_tsx::scene_level(world, &scene) else { continue };
        let levels = world.resource::<Levels>();
        if !levels.closed.contains(&path) && levels.asset_root.join(&path).exists() {
            open_sidecar(world, &scene, &path);
        }
    }
}

fn check_section(world: &mut World, input: &bevy_ts::plugin_api::SectionInput) -> Result<(), Vec<ScriptError>> {
    let s: LevelSection = serde_json::from_slice(input.bytes).map_err(|e| vec![ScriptError::at(format!("levels section: {e}"), input.location.clone())])?;
    for p in &s.levels {
        if !p.ends_with(".bsn") {
            return Err(vec![ScriptError::at(format!("app.level({p:?}): expected a path to a .bsn file"), input.location.clone())]);
        }
    }
    // Rule 9: a generation under which a loaded level no longer reads fails whole. Expansions
    // need the new generation's JS, so they are checked once it is live.
    let mut errors = Vec::new();
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let schema = WorldSchema::pending(world, input.components, &registry);
    let mut files = LoadedFiles { server: world.resource::<AssetServer>(), assets: world.resource::<Assets<BsnLevel>>(), hashes: BTreeMap::new() };
    for p in &s.levels {
        if files.parsed(p).is_err() {
            continue;
        }
        if let Err(e) = compose::prepare(p, &mut files, &schema) {
            errors.extend(e.into_iter().filter(|e| !e.message.ends_with(": not loaded")));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn apply_section(world: &mut World, applied: &bevy_ts::plugin_api::AppliedSection) {
    let wanted: Vec<String> = if applied.present { serde_json::from_slice::<LevelSection>(applied.bytes).map(|s| s.levels).unwrap_or_default() } else { Vec::new() };
    let gone: Vec<String> = world.resource::<Levels>().roots.iter().filter(|(p, r)| r.opener == Opener::Ts && !wanted.contains(p)).map(|(p, _)| p.clone()).collect();
    for p in gone {
        close(world, &p);
    }
    // A level the editor closed stays closed until it opens it again.
    let closed = world.resource::<Levels>().closed.clone();
    for p in wanted.iter().filter(|p| !closed.contains(*p)) {
        open(world, p, Opener::Ts);
    }
    world.resource_mut::<Levels>().declared = wanted;
}

/// A generation re-expands every instance (rule 9). The runtime is busy while a generation
/// applies, so the expansion runs at the next frame boundary, in `watch`.
fn after_generation(world: &mut World, _: &bevy_ts::plugin_api::AppliedSection) {
    for r in world.resource_mut::<Levels>().roots.values_mut() {
        r.dirty = true;
    }
    open_sidecars(world);
}

fn after_restart(world: &mut World) {
    for r in world.resource_mut::<Levels>().roots.values_mut() {
        r.dirty = true;
        r.applied.clear();
    }
}

impl Plugin for LevelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<BsnLevel>().register_asset_loader(BsnLoader).init_resource::<Levels>().init_resource::<crate::edit::WriteResults>();
        app.add_ts_section(SECTION, check_section);
        app.on_ts_section_applied(SECTION, apply_section);
        app.on_ts_section_applied(bevy_ts_tsx::SECTION, after_generation);
        app.on_ts_restart(after_restart);
        app.insert_resource(bevy_ts_tsx::protocol::TreeProvenance(tree_level));
        app.add_systems(Startup, open_pending);
        app.add_systems(Update, watch.before(bevy_ts::BevyTsSet::Update));
    }
}

fn open_pending(world: &mut World) {
    let pending = std::mem::take(&mut world.get_resource_or_init::<PendingLevels>().0);
    let closed = world.resource::<Levels>().closed.clone();
    for p in pending.iter().filter(|p| !closed.contains(*p)) {
        open(world, p, Opener::Rust);
    }
}

/// Marks levels whose files changed, and reconciles every dirty level whose files are loaded.
fn watch(
    world: &mut World,
    mut cursor: Local<MessageCursor<AssetEvent<BsnLevel>>>,
    mut failed: Local<MessageCursor<AssetLoadFailedEvent<BsnLevel>>>,
    mut missing: Local<MessageCursor<UntypedAssetLoadFailedEvent>>,
) {
    let mut changed = Vec::new();
    let mut external = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    {
        let messages = world.resource::<Messages<UntypedAssetLoadFailedEvent>>();
        let levels = world.resource::<Levels>();
        for ev in missing.read(messages) {
            let path = ev.path.path().to_string_lossy().replace('\\', "/");
            for r in levels.roots.values() {
                if let Some(at) = r.assets.get(&path) {
                    warnings.push(ScriptError::at(format!("missing asset {path}: {}", ev.error), Some(at.clone())));
                }
            }
        }
    }
    for w in &warnings {
        log(world, LogLevel::Warn, w);
    }
    {
        let messages = world.resource::<Messages<AssetLoadFailedEvent<BsnLevel>>>();
        for ev in failed.read(messages) {
            let path = ev.path.path().to_string_lossy().replace('\\', "/");
            errors.push(located(&path, &ev.error.to_string()));
        }
    }
    for e in &errors {
        log(world, LogLevel::Error, e);
        let affected = world.resource::<Levels>().affected(e.location.as_ref().map_or("", |l| l.file.as_str()));
        let mut levels = world.resource_mut::<Levels>();
        for p in affected {
            if let Some(r) = levels.roots.get_mut(&p) {
                r.error = Some(e.clone());
            }
        }
    }
    {
        let messages = world.resource::<Messages<AssetEvent<BsnLevel>>>();
        let server = world.resource::<AssetServer>();
        let assets = world.resource::<Assets<BsnLevel>>();
        let levels = world.resource::<Levels>();
        for ev in cursor.read(messages) {
            if let AssetEvent::Modified { id } | AssetEvent::LoadedWithDependencies { id } = ev
                && let Some(p) = server.get_path(*id)
            {
                let file = p.path().to_string_lossy().replace('\\', "/");
                // The writer's own write, already reconciled: the reload brings nothing new.
                let hash = assets.get(*id).map(|a| a.hash);
                let applied = levels.roots.values().any(|r| r.applied.get(&file).is_some_and(|h| Some(*h) == hash)) && levels.affected(&file).iter().all(|l| levels.roots[l].applied.contains_key(&file));
                if !applied {
                    if matches!(ev, AssetEvent::Modified { .. }) {
                        external.push((file.clone(), hash));
                    }
                    changed.push(file);
                }
            }
        }
    }
    for (file, hash) in external {
        world.resource_mut::<Levels>().emit(serde_json::json!({ "event": "external", "level": file, "hash": hash.map(crate::edit::hash_text) }));
    }
    for file in changed {
        let affected = world.resource::<Levels>().affected(&file);
        let mut levels = world.resource_mut::<Levels>();
        for p in affected {
            if let Some(r) = levels.roots.get_mut(&p) {
                r.dirty = true;
            }
        }
    }
    reconcile_dirty(world);
}

/// A load failure as a located error: the loader renders parse errors `file:line:col: message`.
fn located(path: &str, text: &str) -> ScriptError {
    let needle = format!("{path}:");
    for (i, _) in text.match_indices(&needle) {
        let rest = &text[i + needle.len()..];
        let mut parts = rest.splitn(3, ':');
        if let (Some(l), Some(c), Some(m)) = (parts.next(), parts.next(), parts.next())
            && let (Ok(line), Ok(column)) = (l.parse::<u32>(), c.parse::<u32>())
        {
            return ScriptError::at(m.trim_start(), Some(bevy_ts::backend::SourceLocation { file: path.to_string(), line, column }));
        }
    }
    ScriptError::at(text, Some(bevy_ts::backend::SourceLocation { file: path.to_string(), line: 1, column: 1 }))
}

/// Files read from the loaded `BsnLevel` assets.
struct LoadedFiles<'w> {
    server: &'w AssetServer,
    assets: &'w Assets<BsnLevel>,
    hashes: BTreeMap<String, u64>,
}

impl Files for LoadedFiles<'_> {
    fn parsed(&mut self, path: &str) -> Result<Arc<ParsedFile>, String> {
        let handle = self.server.get_handle::<BsnLevel>(path.to_string()).ok_or("not loaded")?;
        let level = self.assets.get(&handle).ok_or("not loaded")?;
        self.hashes.insert(path.to_string(), level.hash);
        Ok(level.parsed.clone())
    }
}

/// The world's components as levels see them: layout-tier Rust components, `Name`, and the
/// live generation's TS components, by full path or unambiguous short name.
pub struct WorldSchema<'w> {
    /// The generation's TS components: the live ones, or a pending generation's while checking.
    pub ts: Vec<bevy_ts::manifest::ComponentDecl>,
    pub native: &'w NativeLayouts,
    pub registry: &'w TypeRegistry,
}

impl<'w> WorldSchema<'w> {
    /// The live generation's types.
    pub fn live(world: &'w World, registry: &'w TypeRegistry) -> Self {
        let ts = world.resource::<TsComponents>();
        let decls = ts.current().filter(|c| ts.live(&c.decl.name).is_some()).map(|c| c.decl.clone()).collect();
        WorldSchema { ts: decls, native: world.resource::<NativeLayouts>(), registry }
    }

    /// A pending generation's types, while it is checked.
    pub fn pending(world: &'w World, pending: &bevy_ts::components::PendingComponents, registry: &'w TypeRegistry) -> Self {
        let decls = pending.iter().filter_map(|(name, ..)| pending.decl(name).cloned()).collect();
        WorldSchema { ts: decls, native: world.resource::<NativeLayouts>(), registry }
    }
}

fn short(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

impl WorldSchema<'_> {
    fn ts_schema(&self, name: &str) -> Option<ComponentSchema> {
        let decl = self.ts.iter().find(|d| d.name == name)?;
        let fields = decl
            .fields
            .iter()
            .map(|f| FieldSchema { name: f.name.clone(), ty: ValueType::Field(f.ty.clone()), default: f.default.clone().unwrap_or_else(|| value::zero(&f.ty)) })
            .collect();
        Some(ComponentSchema { name: decl.name.clone(), kind: ComponentKind::Ts, fields, is_name: false })
    }

    /// What a reflected field holds.
    fn value_type(&self, type_id: TypeId, type_path: &str) -> ValueType {
        use bevy_ts::layout::FieldType as F;
        if self.registry.get(type_id).is_some_and(|r| r.data::<ReflectHandle>().is_some()) {
            return ValueType::Handle;
        }
        let field = |t| ValueType::Field(t);
        let plain = [
            (TypeId::of::<f32>(), F::F32),
            (TypeId::of::<f64>(), F::F64),
            (TypeId::of::<i8>(), F::I8),
            (TypeId::of::<u8>(), F::U8),
            (TypeId::of::<i16>(), F::I16),
            (TypeId::of::<u16>(), F::U16),
            (TypeId::of::<i32>(), F::I32),
            (TypeId::of::<u32>(), F::U32),
            (TypeId::of::<bool>(), F::Bool),
            (TypeId::of::<Vec2>(), F::Vec2),
            (TypeId::of::<Vec3>(), F::Vec3),
            (TypeId::of::<Vec4>(), F::Vec4),
            (TypeId::of::<Quat>(), F::Quat),
        ];
        if let Some((_, t)) = plain.into_iter().find(|(id, _)| *id == type_id) {
            return field(t);
        }
        if type_id == TypeId::of::<String>() {
            return ValueType::String;
        }
        if let Some(TypeInfo::Enum(e)) = self.registry.get(type_id).map(|r| r.type_info())
            && e.iter().all(|v| matches!(v, VariantInfo::Unit(_)))
        {
            return field(F::Enum { variants: e.variant_names().iter().map(|s| s.to_string()).collect(), repr: None });
        }
        ValueType::Unsupported(type_path.to_string())
    }

    /// A component written through reflection: its fields from reflection, defaults from its
    /// `Default`.
    fn reflect_schema(&self, type_path: &str) -> Result<ComponentSchema, String> {
        let Some(reg) = self.registry.get_with_type_path(type_path) else { return Err(format!("{type_path} is not registered")) };
        let default = reg.data::<ReflectDefault>().map(|d| d.default());
        let field_default = |name: &str| -> Value {
            let Some(d) = &default else { return Value::Null };
            let Ok(p) = ParsedPath::parse(&format!(".{name}")) else { return Value::Null };
            let Ok(f) = p.reflect_element(d.as_partial_reflect()) else { return Value::Null };
            serde_json::to_value(TypedReflectSerializer::new(f, self.registry)).unwrap_or(Value::Null)
        };
        let fields: Vec<FieldSchema> = match reg.type_info() {
            TypeInfo::Struct(s) => s.iter().map(|f| FieldSchema { name: f.name().to_string(), ty: self.value_type(f.type_id(), f.type_path()), default: field_default(f.name()) }).collect(),
            TypeInfo::TupleStruct(s) => s
                .iter()
                .map(|f| {
                    let name = f.index().to_string();
                    FieldSchema { default: field_default(&name), name, ty: self.value_type(f.type_id(), f.type_path()) }
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok(ComponentSchema { name: type_path.to_string(), kind: ComponentKind::Rust, fields, is_name: false })
    }
}

impl Schema for WorldSchema<'_> {
    fn component(&self, path: &str) -> Result<ComponentSchema, String> {
        if path == "Name" || path == "bevy_ecs::name::Name" {
            return Ok(ComponentSchema { name: "Name".into(), kind: ComponentKind::Rust, fields: Vec::new(), is_name: true });
        }
        if let Some(s) = self.ts_schema(path) {
            return Ok(s);
        }
        let mut candidates: Vec<String> = self.ts.iter().filter(|d| short(&d.name) == path).map(|d| d.name.clone()).collect();
        let native_full = self.native.iter().any(|n| n.type_path == path);
        match self.native.ambiguous(path) {
            Some(paths) if !native_full => candidates.extend(paths.iter().cloned()),
            _ => {
                if let Some(n) = self.native.get(path) {
                    candidates.push(n.type_path.clone());
                }
            }
        }
        candidates.sort();
        candidates.dedup();
        match candidates.as_slice() {
            [] => match self.native.reflect_tier(path) {
                Some(t) => self.reflect_schema(&t.type_path.clone()),
                None => Err(format!("unknown component {path}")),
            },
            [one] => {
                if let Some(s) = self.ts_schema(one) {
                    return Ok(s);
                }
                let n = self.native.get(one).expect("a native candidate");
                if !n.insertable {
                    return self.reflect_schema(&n.type_path);
                }
                let fields = n
                    .layout
                    .fields
                    .iter()
                    .map(|f| FieldSchema { name: f.name.clone(), ty: ValueType::Field(f.ty.clone()), default: value::decode_field(&f.ty, &n.defaults[f.offset..f.offset + f.ty.size()]) })
                    .collect();
                Ok(ComponentSchema { name: n.ts_name.clone(), kind: ComponentKind::Rust, fields, is_name: false })
            }
            many => Err(format!("{path} is ambiguous: {}; write the full path", many.join(", "))),
        }
    }

    fn scene(&self, path: &str) -> Result<SceneSchema, String> {
        // By name, by id (`src/enemies.tsx#Grunt`), or by a short name only one has.
        let by = |f: &dyn Fn(&bevy_ts::manifest::ComponentDecl) -> bool| self.ts.iter().filter(|d| f(d)).cloned().collect::<Vec<_>>();
        let mut found = by(&|d| d.name == path || d.id.as_deref() == Some(path));
        if found.is_empty() {
            found = by(&|d| short(&d.name) == path && d.scene.is_some());
        }
        let decl = match found.as_slice() {
            [one] => one.clone(),
            [] => return Err(format!("unknown scene component {path}")),
            many => return Err(format!("{path} is ambiguous: {}; write the full path", many.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", "))),
        };
        let Some(scene) = &decl.scene else {
            return Err(format!("{} is not a scene component: a level places classes that extend SceneComponent with a static scene()", decl.name));
        };
        let component = self.ts_schema(&decl.name).ok_or_else(|| format!("{} is not live", decl.name))?;
        let props = scene
            .props
            .iter()
            .map(|p| {
                let (ty, zero) = match &p.ty {
                    bevy_ts::manifest::PropType::Field(t) => (ValueType::Field(t.clone()), value::zero(t)),
                    bevy_ts::manifest::PropType::String { .. } => (ValueType::String, Value::String(String::new())),
                };
                FieldSchema { name: p.name.clone(), ty, default: p.default.clone().unwrap_or(zero) }
            })
            .collect();
        let required = scene.props.iter().filter(|p| p.default.is_none() && !p.has_default && !p.optional).map(|p| p.name.clone()).collect();
        Ok(SceneSchema { component, props, required })
    }
}

/// Expands scene component instances through tsx's host entry: one call for all of them.
fn expand(world: &mut World, requests: &[ExpandRequest]) -> Result<Vec<NodeDesc>, Vec<ScriptError>> {
    let items: Vec<bevy_ts_tsx::ExpandItem> = requests.iter().map(|r| bevy_ts_tsx::ExpandItem { component: r.component.clone(), props: r.props.clone(), at: r.at.clone() }).collect();
    bevy_ts_tsx::expand(world, &items)
}

/// Composes one level from the loaded files, expanding its instances.
pub fn compose_level(world: &mut World, path: &str) -> (Result<Composed, Vec<ScriptError>>, BTreeMap<String, u64>) {
    let mut files = LoadedFiles { server: world.resource::<AssetServer>(), assets: world.resource::<Assets<BsnLevel>>(), hashes: BTreeMap::new() };
    let prepared = prepare_with(world, path, &mut files);
    let hashes = files.hashes;
    (finish_compose(world, prepared), hashes)
}

fn prepare_with(world: &World, path: &str, files: &mut dyn Files) -> Result<compose::Prepared, Vec<ScriptError>> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let schema = WorldSchema::live(world, &registry);
    compose::prepare(path, files, &schema)
}

/// Expands a prepared level's instances and finishes it.
fn finish_compose(world: &mut World, prepared: Result<compose::Prepared, Vec<ScriptError>>) -> Result<Composed, Vec<ScriptError>> {
    let prepared = prepared?;
    let expansions = if prepared.requests.is_empty() { Vec::new() } else { expand(world, &prepared.requests)? };
    prepared.finish(expansions)
}

fn log(world: &mut World, level: LogLevel, e: &ScriptError) {
    let generation = bevy_ts::runtime::current_generation(world);
    world.resource_mut::<ScriptLog>().push(level, format!("levels: {}", e.message), e.location.clone(), generation);
}

/// Composes and reconciles every dirty level whose files are loaded. A level that fails stays
/// exactly as it was (rule 8).
pub fn reconcile_dirty(world: &mut World) {
    let ready: Vec<String> = {
        let levels = world.resource::<Levels>();
        let server = world.resource::<AssetServer>();
        levels.roots.iter().filter(|(_, r)| r.dirty && server.is_loaded_with_dependencies(&r.handle)).map(|(p, _)| p.clone()).collect()
    };
    for path in ready {
        reconcile_level(world, &path);
    }
}

/// Composes and reconciles one level now.
pub fn reconcile_level(world: &mut World, path: &str) {
    if let Some(r) = world.resource_mut::<Levels>().roots.get_mut(path) {
        r.dirty = false;
    }
    let (result, hashes) = compose_level(world, path);
    apply_composed(world, path, result, hashes);
}

/// Reconciles a level from its compose: a failure leaves it as it was.
fn apply_composed(world: &mut World, path: &str, result: Result<Composed, Vec<ScriptError>>, hashes: BTreeMap<String, u64>) {
    let versions: Vec<(u64, Arc<ParsedFile>)> = {
        let server = world.resource::<AssetServer>();
        let assets = world.resource::<Assets<BsnLevel>>();
        hashes.keys().filter_map(|f| server.get_handle::<BsnLevel>(f.clone()).and_then(|h| assets.get(&h)).map(|a| (a.hash, a.parsed.clone()))).collect()
    };
    for (h, p) in versions {
        world.resource_mut::<Levels>().remember(h, p);
    }
    let composed = match result {
        Ok(c) => c,
        // An inclusion the last edit added is still loading: try again when it has.
        Err(errors) if errors.iter().any(|e| e.message.ends_with(": not loaded")) => {
            if let Some(r) = world.resource_mut::<Levels>().roots.get_mut(path) {
                r.dirty = true;
            }
            return;
        }
        Err(errors) => {
            for e in &errors {
                log(world, LogLevel::Error, e);
            }
            let diagnostics: Vec<Value> = errors.iter().map(|e| serde_json::json!({ "message": e.message, "location": e.location })).collect();
            let mut levels = world.resource_mut::<Levels>();
            levels.emit(serde_json::json!({ "event": "failed", "level": path, "diagnostics": diagnostics }));
            if let Some(r) = levels.roots.get_mut(path) {
                r.error = errors.into_iter().next();
            }
            return;
        }
    };
    let root = root_name(path);
    let overlay = world.resource::<Levels>().roots.get(path).and_then(|r| r.overlay.clone());
    let applied = match &overlay {
        // A sidecar's names are the scene's keys: they name nothing.
        Some(scene) => {
            let mut nodes = composed.nodes.clone();
            strip_names(&mut nodes);
            bevy_ts_tsx::set_overlay(world, scene, nodes).map(|_| ()).map_err(|mut e| e.remove(0))
        }
        None => bevy_ts_tsx::reconcile_external(world, &root, None, &composed.nodes).map(|_| ()),
    };
    if let Err(e) = applied {
        log(world, LogLevel::Error, &e);
        if let Some(r) = world.resource_mut::<Levels>().roots.get_mut(path) {
            r.error = Some(e);
        }
        return;
    }
    let new_warnings: Vec<ScriptError> = {
        let levels = world.resource::<Levels>();
        let old = levels.roots.get(path).map(|r| r.warnings.clone()).unwrap_or_default();
        composed.warnings.iter().filter(|w| !old.contains(w)).cloned().collect()
    };
    for w in &new_warnings {
        log(world, LogLevel::Warn, w);
    }
    let stamp_root = overlay.clone().unwrap_or(root.clone());
    stamp_instances(world, path, &stamp_root, &composed.owners, &composed.scenes);
    let mut assets = BTreeMap::new();
    asset_paths(&composed.nodes, &mut assets);
    if let Some(r) = world.resource_mut::<Levels>().roots.get_mut(path) {
        r.error = None;
        r.files = composed.files;
        r.applied = hashes;
        r.owners = composed.owners;
        r.warnings = composed.warnings;
        r.assets = assets;
    }
    let hash = world.resource::<Levels>().roots.get(path).and_then(|r| r.applied.get(path)).copied();
    world.resource_mut::<Levels>().emit(serde_json::json!({ "event": "loaded", "level": path, "hash": hash.map(crate::edit::hash_text) }));
}

/// Strings in overridden component fields are asset paths: only handle fields take strings.
fn asset_paths(nodes: &[NodeDesc], out: &mut BTreeMap<String, bevy_ts::backend::SourceLocation>) {
    for n in nodes {
        for c in &n.components {
            for (f, at) in &c.overrides {
                if let Some(Value::String(p)) = c.props.get(f) {
                    out.insert(p.split('#').next().unwrap_or(p).to_string(), at.clone());
                }
            }
        }
        asset_paths(&n.children, out);
    }
}

fn strip_names(nodes: &mut [NodeDesc]) {
    for n in nodes {
        n.name = None;
        strip_names(&mut n.children);
    }
}

/// `LevelInstance` on every entity the level placed or overrode.
fn stamp_instances(world: &mut World, level: &str, root: &str, owners: &BTreeMap<Vec<String>, Vec<Frame>>, scenes: &BTreeMap<Vec<String>, ScenePlacement>) {
    let mut writes = Vec::new();
    {
        let tree = world.resource::<TsxTree>();
        let parsed = world.resource::<Levels>().roots.get(level).and_then(|r| world.resource::<Assets<BsnLevel>>().get(&r.handle)).map(|a| a.parsed.clone());
        let sidecar = root != root_name(level);
        for (key_path, frames) in owners {
            let Some(e) = tree.entity(node_id(root, key_path)) else { continue };
            let mut inst = LevelInstance { level: level.to_string(), name_path: key_path.clone(), owners: frames.iter().map(OwnerFrame::from).collect(), scene: scenes.get(key_path).cloned(), written: false };
            // Looked up in the file only when the instance changed (design, Risks).
            inst.written = match world.get::<LevelInstance>(e) {
                Some(old) if LevelInstance { written: old.written, ..inst.clone() } == *old => old.written,
                // A sidecar's override of a TSX entity is not written there: the scene made it.
                _ => parsed.as_ref().is_some_and(|p| crate::writer::written(p, key_path)) && (!sidecar || world.get::<TsxNode>(e).and_then(|n| n.source.last()).is_some_and(|s| s.file.ends_with(".bsn"))),
            };
            writes.push((e, inst));
        }
    }
    let keep: std::collections::HashSet<Entity> = writes.iter().map(|(e, _)| *e).collect();
    for (e, inst) in writes {
        let Ok(mut em) = world.get_entity_mut(e) else { continue };
        if em.get::<LevelInstance>() != Some(&inst) {
            em.insert(inst);
        }
    }
    // An entity whose override went is the prefab's again.
    let stale: Vec<Entity> = world.query_filtered::<(Entity, &TsxNode), With<LevelInstance>>().iter(world).filter(|(e, n)| n.root == root && !keep.contains(e)).map(|(e, _)| e).collect();
    for e in stale {
        world.entity_mut(e).remove::<LevelInstance>();
    }
}

/// A `tsx_tree` row's `level`: an entity the level placed or overrode, from its `LevelInstance`;
/// a prefab's inner entity, from the scene component instance above it, never written.
fn tree_level(world: &World, e: Entity, n: &TsxNode) -> Option<Value> {
    if let Some(inst) = world.get::<LevelInstance>(e) {
        return Some(serde_json::json!({ "file": inst.level, "namePath": inst.name_path, "written": inst.written }));
    }
    let tree = world.resource::<TsxTree>();
    (1..n.key_path.len()).rev().find_map(|i| {
        let inst = world.get::<LevelInstance>(tree.entity(node_id(&n.root, &n.key_path[..i]))?)?;
        inst.scene.as_ref().map(|_| serde_json::json!({ "file": inst.level, "namePath": n.key_path, "written": false }))
    })
}

/// Why `bevy-ts/level_open` opened nothing.
#[derive(Debug)]
pub struct OpenFailure {
    /// `invalid`, `not-found` or `load`.
    pub reason: &'static str,
    pub diagnostics: Vec<ScriptError>,
}

/// `file` as an asset path inside the asset root, normalized: `None` when it does not end in
/// `.bsn` or leaves the root.
pub fn asset_path(file: &str) -> Option<String> {
    if !file.ends_with(".bsn") || file.starts_with('/') || file.contains('\\') || file.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for seg in file.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Files read from disk for an open; parse failures are kept with their place.
struct DiskFiles {
    root: std::path::PathBuf,
    read: BTreeMap<String, (String, Arc<ParsedFile>)>,
    failed: Vec<ScriptError>,
}

impl Files for DiskFiles {
    fn parsed(&mut self, path: &str) -> Result<Arc<ParsedFile>, String> {
        if let Some((_, p)) = self.read.get(path) {
            return Ok(p.clone());
        }
        let src = std::fs::read_to_string(self.root.join(path)).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "not found".to_string() } else { e.to_string() })?;
        match syntax::parse(&src) {
            Ok(p) => {
                let p = Arc::new(p);
                self.read.insert(path.to_string(), (src, p.clone()));
                Ok(p)
            }
            Err(d) => {
                self.failed.push(located(path, &d.render(path, &src)));
                Err("does not parse".into())
            }
        }
    }
}

/// Opens `file` now (`bevy-ts/level_open`): reads it and every file it includes from disk,
/// composes, and creates and reconciles its root only when that succeeds. A level already open
/// answers as it is. Runs no system: the reconcile's spawns are its only writes.
pub fn open_now(world: &mut World, file: &str) -> Result<String, OpenFailure> {
    let fail = |reason, message: String| OpenFailure { reason, diagnostics: vec![ScriptError::new(message)] };
    let Some(path) = asset_path(file) else { return Err(fail("invalid", format!("{file}: expected a path to a .bsn file inside the asset root"))) };
    if world.resource::<Levels>().roots.contains_key(&path) {
        world.resource_mut::<Levels>().closed.remove(&path);
        return Ok(path);
    }
    let root = world.resource::<Levels>().asset_root.clone();
    if !root.join(&path).is_file() {
        return Err(fail("not-found", format!("{path}: no such file")));
    }
    let mut files = DiskFiles { root, read: BTreeMap::new(), failed: Vec::new() };
    let composed = match prepare_with(world, &path, &mut files) {
        // A parse failure says where; compose only says the inclusion failed.
        Err(_) if !files.failed.is_empty() => Err(std::mem::take(&mut files.failed)),
        p => finish_compose(world, p),
    };
    let composed = composed.map_err(|diagnostics| OpenFailure { reason: "load", diagnostics })?;
    let mut hashes = BTreeMap::new();
    for (f, (text, _)) in &files.read {
        let hash = content_hash(text.as_bytes());
        hashes.insert(f.clone(), hash);
        let same = {
            let server = world.resource::<AssetServer>();
            server.get_handle::<BsnLevel>(f.clone()).and_then(|h| world.resource::<Assets<BsnLevel>>().get(&h)).is_some_and(|a| a.hash == hash)
        };
        if !same {
            crate::edit::load_text(world, f, text).map_err(|e| fail("load", e))?;
        }
    }
    // A scene's sidecar the editor closed layers over its scene again.
    match crate::edit::sidecar_of(world, &path) {
        Some(scene) => open_as(world, &path, Opener::Sidecar, Some(scene)),
        None => {
            let opener = if world.resource::<Levels>().declared.contains(&path) { Opener::Ts } else { Opener::Editor };
            open(world, &path, opener);
        }
    }
    let mut levels = world.resource_mut::<Levels>();
    levels.closed.remove(&path);
    if let Some(r) = levels.roots.get_mut(&path) {
        r.dirty = false;
    }
    apply_composed(world, &path, Ok(composed), hashes);
    Ok(path)
}

/// Closes a level for the editor (`bevy-ts/level_close`), whoever opened it: neither a generation
/// nor a sidecar check reopens it until [`open_now`] names it. False when it was not open.
pub fn close_now(world: &mut World, file: &str) -> bool {
    let path = asset_path(file).unwrap_or_else(|| file.to_string());
    if !world.resource::<Levels>().roots.contains_key(&path) {
        return false;
    }
    close(world, &path);
    world.resource_mut::<Levels>().closed.insert(path);
    true
}
