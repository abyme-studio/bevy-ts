//! Levels in the engine: the `BsnLevel` asset and its loader, level roots reconciled through
//! `tsx` as external roots named `level:<path>`, the world's types as a [`Schema`], and the
//! `LevelInstance` component on every level-placed entity.

use crate::compose::{self, ComponentSchema, Composed, ExpandRequest, Expander, FieldSchema, Files, Frame, SceneSchema, Schema, ValueType};
use crate::syntax::{self, EntityList, Entry, Extension, ParsedFile};
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoadFailedEvent, AssetLoader, LoadContext};
use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use bevy_ts::components::TsComponents;
use bevy_ts::layout::NativeLayouts;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts::plugin_api::BevyTsAppExt;
use bevy_ts::value;
use bevy_ts_tsx::desc::{ComponentKind, NodeDesc, node_id};
use bevy_ts_tsx::TsxTree;
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

fn includes(list: &EntityList, out: &mut Vec<String>) {
    for e in &list.items {
        for entry in &e.entries {
            match entry {
                Entry::Include(i) => out.push(i.path.clone()),
                Entry::Related(r) => includes(&r.list, out),
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
        includes(&parsed.entities, &mut paths);
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

/// One open level.
#[derive(Debug)]
pub struct LevelRoot {
    pub handle: Handle<BsnLevel>,
    /// Declared by `app.level` (so a generation without it closes it), not by Rust.
    pub from_ts: bool,
    /// The level and every file it includes, as of its last successful compose.
    pub files: BTreeSet<String>,
    /// Hashes of those files when last applied.
    pub applied: BTreeMap<String, u64>,
    pub owners: BTreeMap<Vec<String>, Vec<Frame>>,
    pub error: Option<ScriptError>,
    pub warnings: Vec<ScriptError>,
    /// Waiting to be composed and reconciled.
    pub dirty: bool,
}

/// Open levels, by asset path.
#[derive(Resource, Default, Debug)]
pub struct Levels {
    pub roots: BTreeMap<String, LevelRoot>,
}

impl Levels {
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

fn open(world: &mut World, path: &str, from_ts: bool) {
    if world.resource::<Levels>().roots.contains_key(path) {
        return;
    }
    let handle = world.resource::<AssetServer>().load::<BsnLevel>(path.to_string());
    world.resource_mut::<Levels>().roots.insert(
        path.to_string(),
        LevelRoot { handle, from_ts, files: BTreeSet::new(), applied: BTreeMap::new(), owners: BTreeMap::new(), error: None, warnings: Vec::new(), dirty: true },
    );
}

/// Closes a level: its entities despawn.
pub fn close(world: &mut World, path: &str) {
    if world.resource_mut::<Levels>().roots.remove(path).is_some() {
        bevy_ts_tsx::unmount_external(world, &root_name(path));
    }
}

fn check_section(_: &mut World, input: &bevy_ts::plugin_api::SectionInput) -> Result<(), Vec<ScriptError>> {
    let s: LevelSection = serde_json::from_slice(input.bytes).map_err(|e| vec![ScriptError::at(format!("levels section: {e}"), input.location.clone())])?;
    for p in &s.levels {
        if !p.ends_with(".bsn") {
            return Err(vec![ScriptError::at(format!("app.level({p:?}): expected a path to a .bsn file"), input.location.clone())]);
        }
    }
    Ok(())
}

fn apply_section(world: &mut World, applied: &bevy_ts::plugin_api::AppliedSection) {
    let wanted: Vec<String> = if applied.present { serde_json::from_slice::<LevelSection>(applied.bytes).map(|s| s.levels).unwrap_or_default() } else { Vec::new() };
    let gone: Vec<String> = world.resource::<Levels>().roots.iter().filter(|(p, r)| r.from_ts && !wanted.contains(p)).map(|(p, _)| p.clone()).collect();
    for p in gone {
        close(world, &p);
    }
    for p in &wanted {
        open(world, p, true);
    }
}

/// A generation re-expands every instance (rule 9): run after tsx applied the generation.
fn after_generation(world: &mut World, _: &bevy_ts::plugin_api::AppliedSection) {
    for r in world.resource_mut::<Levels>().roots.values_mut() {
        r.dirty = true;
    }
    reconcile_dirty(world);
}

fn after_restart(world: &mut World) {
    for r in world.resource_mut::<Levels>().roots.values_mut() {
        r.dirty = true;
        r.applied.clear();
    }
}

impl Plugin for LevelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<BsnLevel>().register_asset_loader(BsnLoader).init_resource::<Levels>();
        app.add_ts_section(SECTION, check_section);
        app.on_ts_section_applied(SECTION, apply_section);
        app.on_ts_section_applied(bevy_ts_tsx::SECTION, after_generation);
        app.on_ts_restart(after_restart);
        app.add_systems(Startup, open_pending);
        app.add_systems(Update, watch.before(bevy_ts::BevyTsSet::Update));
    }
}

fn open_pending(world: &mut World) {
    let pending = std::mem::take(&mut world.get_resource_or_init::<PendingLevels>().0);
    for p in pending {
        open(world, &p, false);
    }
}

/// Marks levels whose files changed, and reconciles every dirty level whose files are loaded.
fn watch(world: &mut World, mut cursor: Local<MessageCursor<AssetEvent<BsnLevel>>>, mut failed: Local<MessageCursor<AssetLoadFailedEvent<BsnLevel>>>) {
    let mut changed = Vec::new();
    let mut errors = Vec::new();
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
        for ev in cursor.read(messages) {
            if let AssetEvent::Modified { id } | AssetEvent::LoadedWithDependencies { id } = ev
                && let Some(p) = server.get_path(*id)
            {
                changed.push(p.path().to_string_lossy().replace('\\', "/"));
            }
        }
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
    pub ts: &'w TsComponents,
    pub native: &'w NativeLayouts,
}

fn short(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

impl WorldSchema<'_> {
    fn ts_schema(&self, name: &str) -> Option<ComponentSchema> {
        let c = self.ts.live(name)?;
        let fields = c
            .decl
            .fields
            .iter()
            .map(|f| FieldSchema { name: f.name.clone(), ty: ValueType::Field(f.ty.clone()), default: f.default.clone().unwrap_or_else(|| value::zero(&f.ty)) })
            .collect();
        Some(ComponentSchema { name: c.decl.name.clone(), kind: ComponentKind::Ts, fields, is_name: false })
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
        let mut candidates: Vec<String> = self.ts.current().filter(|c| self.ts.live(&c.decl.name).is_some() && short(&c.decl.name) == path).map(|c| c.decl.name.clone()).collect();
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
            [] => Err(match self.native.reflect_tier(path) {
                Some(t) => format!("{path} has no native layout ({}); levels set layout-tier components", t.reason),
                None => format!("unknown component {path}"),
            }),
            [one] => {
                if let Some(s) = self.ts_schema(one) {
                    return Ok(s);
                }
                let n = self.native.get(one).expect("a native candidate");
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
        Err(format!("@{path}: scene components cannot be placed yet; their props are read by systems-api's compile pass (levels task 2.5)"))
    }
}

/// No scene component is placeable until task 2.5, so nothing reaches this.
struct NoExpander;

impl Expander for NoExpander {
    fn expand(&mut self, _: &[ExpandRequest]) -> Result<Vec<NodeDesc>, Vec<ScriptError>> {
        Err(vec![ScriptError::new("scene component expansion is not available yet")])
    }
}

/// Composes one level from the loaded files.
pub fn compose_level(world: &World, path: &str) -> (Result<Composed, Vec<ScriptError>>, BTreeMap<String, u64>) {
    let mut files = LoadedFiles { server: world.resource::<AssetServer>(), assets: world.resource::<Assets<BsnLevel>>(), hashes: BTreeMap::new() };
    let schema = WorldSchema { ts: world.resource::<TsComponents>(), native: world.resource::<NativeLayouts>() };
    let r = compose::compose(path, &mut files, &schema, &mut NoExpander);
    (r, files.hashes)
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
    let composed = match result {
        Ok(c) => c,
        Err(errors) => {
            for e in &errors {
                log(world, LogLevel::Error, e);
            }
            if let Some(r) = world.resource_mut::<Levels>().roots.get_mut(path) {
                r.error = errors.into_iter().next();
            }
            return;
        }
    };
    let root = root_name(path);
    if let Err(e) = bevy_ts_tsx::reconcile_external(world, &root, None, &composed.nodes) {
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
    stamp_instances(world, path, &root, &composed.owners);
    if let Some(r) = world.resource_mut::<Levels>().roots.get_mut(path) {
        r.error = None;
        r.files = composed.files;
        r.applied = hashes;
        r.owners = composed.owners;
        r.warnings = composed.warnings;
    }
}

/// `LevelInstance` on every entity the level placed or overrode.
fn stamp_instances(world: &mut World, level: &str, root: &str, owners: &BTreeMap<Vec<String>, Vec<Frame>>) {
    let mut writes = Vec::new();
    {
        let tree = world.resource::<TsxTree>();
        for (key_path, frames) in owners {
            let Some(e) = tree.entity(node_id(root, key_path)) else { continue };
            let inst = LevelInstance { level: level.to_string(), name_path: key_path.clone(), owners: frames.iter().map(OwnerFrame::from).collect() };
            writes.push((e, inst));
        }
    }
    for (e, inst) in writes {
        let Ok(mut em) = world.get_entity_mut(e) else { continue };
        if em.get::<LevelInstance>() != Some(&inst) {
            em.insert(inst);
        }
    }
}
