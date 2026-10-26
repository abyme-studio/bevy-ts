//! Screenshots: the one owner of what a shot captures, when it is taken and what it answers.
//! `bevy-ts/screenshot` and `BEVY_TS_SCREENSHOT` both take theirs with [`capture`], which spawns
//! Bevy's `Screenshot` observed by Bevy's `save_to_disk`. A shot waits for a ready run: the script
//! started, Bevy's pipeline cache with nothing waiting, and no asset load in flight ([`ReadyPlugin`],
//! [`loads`]). Under [`FixedStep`]
//! time advances 1/60 s a ready frame and not at all otherwise, and every camera draws without
//! Bevy's GPU culling, so a shot is the same at any load.

use crate::{frame, internal, invalid, protocol, required};
use bevy::asset::{RecursiveDependencyLoadState, ReflectAsset, ReflectHandle, UntypedAssetId, UntypedHandle};
use bevy::camera::RenderTarget;
use bevy::diagnostic::FrameCount;
use bevy::ecs::observer::ObservedBy;
use bevy::prelude::*;
use bevy::reflect::enums::VariantInfo;
use bevy::reflect::{ReflectRef, TypeInfo, TypeRegistry};
use bevy::remote::{BrpError, BrpResult};
use bevy::render::render_resource::PipelineCache;
use bevy::render::view::NoIndirectDrawing;
use bevy::render::view::screenshot::{Captured, Capturing, Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::time::{TimeSystems, TimeUpdateStrategy};
use bevy::world_serialization::{DynamicWorldRoot, WorldAssetRoot, WorldInstance, WorldInstanceSpawner};
use bevy_ts::runtime::RuntimeInfo;
use serde_json::Value;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// How long a shot waits for a frame to render it. Wall time: a viewport with no viewer renders none.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// How long a shot Bevy has started capturing waits for its image. A first frame under heavy load
/// takes longer than [`TIMEOUT`] to read back; a viewport shot with no viewer waits this long too,
/// since Bevy marks it captured on its first frame.
pub const READBACK_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a shot waits for the pipeline cache to empty and its assets to load before it gives up
/// (the method) or shoots anyway (`BEVY_TS_SCREENSHOT`). A missing shader keeps a pipeline waiting
/// for ever.
pub const READY_TIMEOUT: Duration = Duration::from_secs(60);
/// The env shot's ready frame: the 60th in a row.
pub const SHOT_FRAME: u64 = 60;
/// The method's ready update: the 3rd in a row. Loads are read as they are now and pipelines one
/// render behind, and an update renders after it ends, so the 3rd reads the render of the 1st:
/// the pipelines a load just finished needs (a texture's material) are waiting there, not missed.
pub const METHOD_FRAME: u32 = 3;
/// One frame of [`FixedStep`] time.
pub const STEP: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// How many pipelines Bevy's `PipelineCache` had waiting after each render, by the `FrameCount`
/// that render extracted, written by the render world. The last few renders.
#[derive(Resource, Clone, Default)]
pub struct PipelinesWaiting(Arc<Mutex<VecDeque<(u32, u64)>>>);

impl PipelinesWaiting {
    /// Pipelines waiting after the render of `frame`, if it rendered.
    pub fn at(&self, frame: u32) -> Option<u64> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).iter().rev().find(|(f, _)| *f == frame).map(|(_, n)| *n)
    }

    fn latest(&self) -> Option<u64> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).back().map(|(_, n)| *n)
    }
}

/// The `FrameCount` this update began with, noted in `First`. Bevy advances `FrameCount` in
/// `Last`, and that frame is extracted after `Last`, so a system after it (the method runs in
/// `RemoteLast`) would otherwise ask after the frame the render thread is still drawing.
#[derive(Resource, Default)]
struct UpdateFrame(Option<u32>);

fn note_frame(frame: Option<Res<FrameCount>>, mut update: ResMut<UpdateFrame>) {
    update.0 = frame.map(|f| f.0);
}

/// Pipelines waiting after the render of the frame before this update's, `None` if it did not
/// render. That render has always finished by now, pipelined or not, so every run reads the same
/// frame's answer at any load, from any schedule of the update. With no `FrameCount`, the last
/// render's.
pub fn pipelines_waiting(world: &World) -> Option<u64> {
    let waiting = world.get_resource::<PipelinesWaiting>()?;
    let noted = world.get_resource::<UpdateFrame>().and_then(|u| u.0);
    match noted.or_else(|| world.get_resource::<FrameCount>().map(|f| f.0)) {
        Some(frame) => waiting.at(frame.wrapping_sub(1)),
        None => waiting.latest(),
    }
}

/// Ready frames in a row: frames on which the script had started, no pipeline was waiting and no
/// asset load was in flight. Counted in `First` while a shot or [`FixedStep`] needs it.
#[derive(Resource, Default, Debug)]
pub struct Ready {
    pub frames: u64,
    /// [`STEP`]s of [`FixedStep`] time taken so far.
    pub steps: u64,
    /// This frame's [`loads`].
    pub loads: Loads,
}

/// What [`loads`] found: loads still in flight, and assets that failed, by path.
#[derive(Default, Debug, Clone)]
pub struct Loads {
    pub loading: usize,
    pub failed: Vec<String>,
}

/// Per reflected type, whether a value of it can hold a `Handle`: most components cannot, and are
/// never reflected.
#[derive(Resource, Default)]
struct HandleTypes(HashMap<TypeId, bool>);

/// Time advances [`STEP`] a ready frame and nothing otherwise, by Bevy's
/// `TimeUpdateStrategy::ManualDuration`. While a [`ShotWhenReady`] waits, it stops at the shot's
/// `at` steps: a ready frame before a frame that was not (a load not yet asked for) stepped time
/// too, and the shot's clock must not count it. Every camera spawned under it gets Bevy's
/// `NoIndirectDrawing`: GPU culling places each instance with an atomic add, so the order of a
/// batch's instances, and which of two at the same depth is drawn last, changes from frame to
/// frame. Under `BEVY_TS_SCREENSHOT` or `BEVY_TS_FIXED_STEP=1`.
#[derive(Resource)]
pub struct FixedStep;

/// `BEVY_TS_SCREENSHOT`'s shot: saved at the `at`th ready frame in a row ([`SHOT_FRAME`]), or
/// after [`READY_TIMEOUT`] with a warning.
#[derive(Resource)]
pub struct ShotWhenReady {
    pub path: PathBuf,
    pub at: u64,
    since: Option<Instant>,
}

impl ShotWhenReady {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), at: SHOT_FRAME, since: None }
    }
}

/// Counts [`Ready`] frames, steps [`FixedStep`] time and takes [`ShotWhenReady`]'s shot.
pub struct ReadyPlugin;

impl Plugin for ReadyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PipelinesWaiting>()
            .init_resource::<Ready>()
            .init_resource::<HandleTypes>()
            .init_resource::<UpdateFrame>()
            .add_observer(|add: On<Add<Camera>>, fixed: Option<Res<FixedStep>>, mut commands: Commands| {
                if fixed.is_some() {
                    commands.entity(add.entity).insert(NoIndirectDrawing);
                }
            })
            .add_systems(First, (note_frame, count_ready).chain().before(TimeSystems))
            .add_systems(Update, shoot_when_ready.run_if(resource_exists::<ShotWhenReady>));
    }

    fn finish(&self, app: &mut App) {
        let waiting = app.world().resource::<PipelinesWaiting>().clone();
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        render.add_systems(
            Render,
            (move |cache: Res<PipelineCache>, frame: Option<Res<FrameCount>>| {
                let mut renders = waiting.0.lock().unwrap_or_else(PoisonError::into_inner);
                renders.push_back((frame.map_or(0, |f| f.0), cache.waiting_pipelines().count() as u64));
                while renders.len() > 8 {
                    renders.pop_front();
                }
            })
            .in_set(RenderSystems::Cleanup),
        );
    }
}

/// Whether the pipeline cache had nothing waiting ([`pipelines_waiting`]). True with no
/// [`ReadyPlugin`].
pub fn pipelines_idle(world: &World) -> bool {
    !world.contains_resource::<PipelinesWaiting>() || pipelines_waiting(world) == Some(0)
}

/// Why a run is not ready, for a warning or an error: pipelines, loads, failed assets.
fn not_ready(world: &World, loads: &Loads) -> String {
    let mut why = Vec::new();
    match pipelines_waiting(world) {
        None if world.contains_resource::<PipelinesWaiting>() => why.push("no frame rendered".to_string()),
        Some(n) if n > 0 => why.push(format!("{n} pipelines still waiting")),
        _ => {}
    }
    if loads.loading > 0 {
        why.push(format!("{} assets still loading", loads.loading));
    }
    why.join(", ")
}

/// Warns once a shot is taken of each asset that failed to load.
fn warn_failed(path: &Path, loads: &Loads) {
    for asset in &loads.failed {
        warn!("{}: {asset} failed to load, so the shot shows it missing", path.display());
    }
}

fn count_ready(world: &mut World) {
    let fixed = world.contains_resource::<FixedStep>();
    if !fixed && !world.contains_resource::<ShotWhenReady>() {
        return;
    }
    let started = world.get_resource::<RuntimeInfo>().is_none_or(|i| i.generation > 0);
    let idle = pipelines_idle(world);
    let loads = loads(world);
    let now = started && idle && loads.loading == 0;
    let cap = world.get_resource::<ShotWhenReady>().map_or(u64::MAX, |s| s.at);
    let mut ready = world.resource_mut::<Ready>();
    ready.frames = if now { ready.frames + 1 } else { 0 };
    ready.loads = loads;
    let step = fixed && now && ready.steps < cap;
    ready.steps += u64::from(step);
    if fixed && let Some(mut strategy) = world.get_resource_mut::<TimeUpdateStrategy>() {
        *strategy = TimeUpdateStrategy::ManualDuration(if step { STEP } else { Duration::ZERO });
    }
}

/// Asset loads in flight, by Bevy's own state: every `Handle` an entity's reflected components
/// hold, and the handles inside an asset added without the server, whose
/// `RecursiveDependencyLoadState` is `Loading`. A world asset root whose `WorldInstance` is not
/// yet ready by `WorldInstanceSpawner::instance_is_ready` counts as one. A failed asset does not
/// keep a frame waiting; it is listed.
pub fn loads(world: &mut World) -> Loads {
    let mut types = world.remove_resource::<HandleTypes>().unwrap_or_default();
    let out = loads_with(world, &mut types.0);
    world.insert_resource(types);
    out
}

fn loads_with(world: &World, types: &mut HashMap<TypeId, bool>) -> Loads {
    let mut out = Loads::default();
    let (Some(server), Some(registry)) = (world.get_resource::<AssetServer>(), world.get_resource::<AppTypeRegistry>()) else { return out };
    let registry = registry.read();
    let mut handles = Vec::new();
    for archetype in world.archetypes().iter().filter(|a| !a.is_empty()) {
        for id in archetype.iter_components() {
            let Some(ty) = world.components().get_info(id).and_then(|i| i.type_id()) else { continue };
            let Some(reflect) = registry.get_type_data::<ReflectComponent>(ty) else { continue };
            if !holds_handles(&registry, ty, types) {
                continue;
            }
            for e in archetype.entities() {
                if let Ok(entity) = world.get_entity(e.id()) && let Some(value) = reflect.reflect(entity) {
                    collect(&registry, value.as_partial_reflect(), &mut handles);
                }
            }
        }
    }
    let mut seen: HashSet<UntypedAssetId> = HashSet::new();
    while let Some(handle) = handles.pop() {
        if !seen.insert(handle.id()) {
            continue;
        }
        match server.get_recursive_dependency_load_state(handle.id()) {
            Some(RecursiveDependencyLoadState::Loading) => out.loading += 1,
            Some(RecursiveDependencyLoadState::Failed(_)) => {
                out.failed.push(server.get_path(handle.id()).map_or_else(|| format!("{:?}", handle.id()), |p| p.to_string()));
            }
            Some(_) => {}
            None => {
                let ty = handle.type_id();
                if let Some(asset) = registry.get_type_data::<ReflectAsset>(ty)
                    && holds_handles(&registry, ty, types)
                    && let Some(value) = asset.get(world, handle.id())
                {
                    collect(&registry, value.as_partial_reflect(), &mut handles);
                }
            }
        }
    }
    if let Some(spawner) = world.get_resource::<WorldInstanceSpawner>() {
        let mut roots = world.try_query_filtered::<(Option<&WorldInstance>, Option<&WorldAssetRoot>, Option<&DynamicWorldRoot>), Or<(With<WorldAssetRoot>, With<DynamicWorldRoot>)>>();
        for (instance, root, dynamic) in roots.iter_mut().flat_map(|q| q.iter(world).collect::<Vec<_>>()) {
            let failed = root.map(|r| r.id().untyped()).or(dynamic.map(|d| d.id().untyped())).is_some_and(|id| server.recursive_dependency_load_state(id).is_failed());
            if !failed && !instance.is_some_and(|i| spawner.instance_is_ready(**i)) {
                out.loading += 1;
            }
        }
    }
    out
}

/// Pushes every `Handle` in `value`, recursing through its fields.
fn collect(registry: &TypeRegistry, value: &dyn PartialReflect, out: &mut Vec<UntypedHandle>) {
    if let Some(value) = value.try_as_reflect()
        && let Some(handle) = registry.get_type_data::<ReflectHandle>(value.as_any().type_id())
    {
        out.extend(handle.downcast_handle_untyped(value.as_any()));
        return;
    }
    match value.reflect_ref() {
        ReflectRef::Struct(s) => s.iter_fields().for_each(|(_, f)| collect(registry, f, out)),
        ReflectRef::TupleStruct(s) => s.iter_fields().for_each(|f| collect(registry, f, out)),
        ReflectRef::Tuple(t) => t.iter_fields().for_each(|f| collect(registry, f, out)),
        ReflectRef::List(l) => l.iter().for_each(|f| collect(registry, f, out)),
        ReflectRef::Array(a) => a.iter().for_each(|f| collect(registry, f, out)),
        ReflectRef::Map(m) => m.iter().for_each(|(k, v)| {
            collect(registry, k, out);
            collect(registry, v, out);
        }),
        ReflectRef::Set(s) => s.iter().for_each(|f| collect(registry, f, out)),
        ReflectRef::Enum(e) => e.iter_fields().for_each(|f| collect(registry, f.value(), out)),
        _ => {}
    }
}

/// Whether a value of type `ty` can hold a `Handle`, from its `TypeInfo`. A type with no info
/// is taken to hold one. Cached per type once known.
fn holds_handles(registry: &TypeRegistry, ty: TypeId, types: &mut HashMap<TypeId, bool>) -> bool {
    if let Some(&holds) = types.get(&ty) {
        return holds;
    }
    let holds = type_holds(registry, ty, types, &mut HashSet::new());
    types.insert(ty, holds);
    holds
}

/// As [`holds_handles`], under the types `walking` now. A type met again inside itself adds
/// nothing, so a `false` found that way is not cached: only `true`, which is final, is.
fn type_holds(registry: &TypeRegistry, ty: TypeId, types: &mut HashMap<TypeId, bool>, walking: &mut HashSet<TypeId>) -> bool {
    if let Some(&holds) = types.get(&ty) {
        return holds;
    }
    if !walking.insert(ty) {
        return false;
    }
    let holds = registry.get_type_data::<ReflectHandle>(ty).is_some() || registry.get(ty).is_none_or(|r| info_holds(registry, Some(r.type_info()), types, walking));
    walking.remove(&ty);
    if holds {
        types.insert(ty, true);
    }
    holds
}

fn info_holds(registry: &TypeRegistry, info: Option<&'static TypeInfo>, types: &mut HashMap<TypeId, bool>, walking: &mut HashSet<TypeId>) -> bool {
    let Some(info) = info else { return true };
    if registry.get_type_data::<ReflectHandle>(info.type_id()).is_some() {
        return true;
    }
    let fields: Vec<Option<&'static TypeInfo>> = match info {
        TypeInfo::Struct(s) => s.iter().map(|f| f.type_info()).collect(),
        TypeInfo::TupleStruct(s) => s.iter().map(|f| f.type_info()).collect(),
        TypeInfo::Tuple(t) => t.iter().map(|f| f.type_info()).collect(),
        TypeInfo::List(l) => vec![l.item_info()],
        TypeInfo::Array(a) => vec![a.item_info()],
        TypeInfo::Map(m) => vec![m.key_info(), m.value_info()],
        TypeInfo::Set(s) => return type_holds(registry, s.value_ty().id(), types, walking),
        TypeInfo::Enum(e) => e
            .iter()
            .flat_map(|v| match v {
                VariantInfo::Struct(s) => s.iter().map(|f| f.type_info()).collect::<Vec<_>>(),
                VariantInfo::Tuple(t) => t.iter().map(|f| f.type_info()).collect(),
                VariantInfo::Unit(_) => Vec::new(),
            })
            .collect(),
        TypeInfo::Opaque(_) => return false,
    };
    fields.into_iter().any(|f| match f {
        Some(f) => type_holds(registry, f.type_id(), types, walking),
        None => true,
    })
}

/// Takes [`ShotWhenReady`]'s shot once, at its ready frame.
pub fn shoot_when_ready(world: &mut World) {
    let frames = world.resource::<Ready>().frames;
    let mut shot = world.resource_mut::<ShotWhenReady>();
    let since = *shot.since.get_or_insert_with(Instant::now);
    let (at, late) = (shot.at, since.elapsed() > READY_TIMEOUT);
    if frames < at && !late {
        return;
    }
    let path = shot.path.clone();
    world.remove_resource::<ShotWhenReady>();
    let loads = world.resource::<Ready>().loads.clone();
    if frames < at {
        let started = world.get_resource::<RuntimeInfo>().is_none_or(|i| i.generation > 0);
        let why = if started { not_ready(world, &loads) } else { "the script has not started".to_string() };
        warn!("{}: not ready after {} s ({why}); shooting anyway", path.display(), READY_TIMEOUT.as_secs());
    }
    warn_failed(&path, &loads);
    capture(world, path);
}

/// What a shot captures. Absent: the primary window. Headless names its offscreen image.
#[derive(Resource, Clone)]
pub struct ScreenshotTarget(pub RenderTarget);

/// A shot asked for: where it goes, and what is known of it so far.
#[derive(Component)]
pub struct Shot {
    pub path: PathBuf,
    /// The file's modified time before the shot, if it existed.
    pub asked: Option<SystemTime>,
    pub since: Instant,
    /// The bevy-ts frame the shot shows. A target captured later than asked sets it.
    pub frame: u64,
    /// The captured image's size, once captured.
    pub size: Option<UVec2>,
}

/// Spawns a shot of the run's [`ScreenshotTarget`] (or the primary window), saved to `path` on
/// the next rendered frame.
pub fn capture(world: &mut World, path: impl Into<PathBuf>) -> Entity {
    let path = path.into();
    let target = world.get_resource::<ScreenshotTarget>().map_or(RenderTarget::Window(Default::default()), |t| t.0.clone());
    let asked = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let shot = Shot { path: path.clone(), asked, since: Instant::now(), frame: frame(world), size: None };
    world
        .spawn((Screenshot(target), shot))
        .observe(save_to_disk(path))
        .observe(|c: On<ScreenshotCaptured>, mut shots: Query<&mut Shot>| {
            if let Ok(mut s) = shots.get_mut(c.entity) {
                s.size = Some(c.image.size());
            }
        })
        .id()
}

/// Shots `bevy-ts/screenshot` is serving, keyed by params, and the updates counted so far.
#[derive(Resource, Default)]
pub(crate) struct Shots(HashMap<String, Served>, u64);

struct Served {
    state: Progress,
    /// Called this update; a request not called for an update has closed.
    touched: bool,
}

enum Progress {
    /// Asked, and waiting for the pipeline cache to empty and the loads to finish before it
    /// captures, at the [`METHOD_FRAME`]th ready update in a row; `ready` counts them, to update
    /// `at`, so identical requests in one update count it once.
    Waiting { path: PathBuf, since: Instant, ready: u32, at: u64 },
    Pending(Entity),
    /// Answered in update `at`, to `owed` requests. Bevy calls each once more in the next update
    /// (its send then fails, and Bevy drops it): those calls get the answer again, as does an
    /// identical request in update `at`. Any other call is a new request and takes a new shot.
    Done { answer: BrpResult, at: u64, owed: u32 },
}

/// Checks a path before anything is captured.
fn check(path: &str) -> Result<PathBuf, BrpError> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(invalid(format!("{path}: the path must be absolute")));
    }
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Err(invalid(format!("{path}: the path must end in .png")));
    }
    match p.parent() {
        Some(dir) if dir.is_dir() => Ok(p.to_path_buf()),
        Some(dir) => Err(invalid(format!("{path}: the folder {} does not exist", dir.display()))),
        None => Err(invalid(format!("{path}: the path names no folder"))),
    }
}

/// `bevy-ts/screenshot`, a deferred method: `None` until the shot is written or fails.
pub(crate) fn screenshot(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let update = world.resource::<Shots>().1;
    let state = world.resource_mut::<Shots>().0.remove(&key).map(|s| s.state);
    let state = match state {
        Some(Progress::Done { answer, at, owed }) if at == update => Progress::Done { answer, at, owed: owed + 1 },
        Some(Progress::Done { answer, at, owed }) if at + 1 == update && owed > 0 => Progress::Done { answer, at, owed: owed - 1 },
        Some(Progress::Pending(e)) => Progress::Pending(e),
        Some(w @ Progress::Waiting { .. }) => w,
        _ => {
            let params: protocol::ScreenshotParams = required(p)?;
            Progress::Waiting { path: check(&params.path)?, since: Instant::now(), ready: 0, at: update.wrapping_sub(1) }
        }
    };
    let state = match state {
        Progress::Waiting { path, since, ready, at } => {
            let loads = if pipelines_idle(world) { loads(world) } else { Loads { loading: 1, failed: Vec::new() } };
            let ready = match (pipelines_idle(world) && loads.loading == 0, at == update) {
                (false, _) => 0,
                (true, true) => ready,
                (true, false) => ready + 1,
            };
            if ready >= METHOD_FRAME {
                warn_failed(&path, &loads);
                Progress::Pending(capture(world, path))
            } else if world.contains_resource::<PipelinesWaiting>() && pipelines_waiting(world).is_none() && since.elapsed() > TIMEOUT {
                let answer = Err(internal(format!("{}: no frame rendered within {} s", path.display(), TIMEOUT.as_secs())));
                Progress::Done { answer, at: update, owed: 1 }
            } else if since.elapsed() > READY_TIMEOUT {
                let answer = Err(internal(format!("{}: {} after {} s", path.display(), not_ready(world, &loads), READY_TIMEOUT.as_secs())));
                Progress::Done { answer, at: update, owed: 1 }
            } else {
                Progress::Waiting { path, since, ready, at: update }
            }
        }
        state => state,
    };
    let state = match state {
        Progress::Pending(e) => match result(world, e) {
            Some(r) => {
                if r.is_err() {
                    abandon(world, e);
                }
                Progress::Done { answer: r, at: update, owed: 1 }
            }
            None => Progress::Pending(e),
        },
        done => done,
    };
    let answer = match &state {
        Progress::Done { answer, .. } => Some(answer.clone()),
        Progress::Pending(_) | Progress::Waiting { .. } => None,
    };
    world.resource_mut::<Shots>().0.insert(key, Served { state, touched: true });
    answer.transpose()
}

/// A shot's answer, or `None` while it waits.
fn result(world: &World, e: Entity) -> Option<BrpResult> {
    let Ok(entity) = world.get_entity(e) else {
        return Some(Err(internal("Bevy dropped the shot: another shot of the same view was taken that frame")));
    };
    let shot = entity.get::<Shot>().expect("a shot entity has its Shot");
    let path = shot.path.display();
    if !entity.contains::<Captured>() {
        let (limit, what) = if entity.contains::<Capturing>() { (READBACK_TIMEOUT, "no image read back") } else { (TIMEOUT, "no frame rendered") };
        return (shot.since.elapsed() > limit).then(|| Err(internal(format!("{path}: {what} within {} s", limit.as_secs()))));
    }
    let modified = std::fs::metadata(&shot.path).and_then(|m| m.modified()).ok();
    let written = modified.is_some_and(|m| shot.asked.is_none_or(|a| m > a));
    let size = shot.size.unwrap_or_default();
    Some(if written {
        serde_json::to_value(protocol::ScreenshotResult { path: shot.path.display().to_string(), width: size.x, height: size.y, frame: shot.frame })
            .map_err(|e| internal(e.to_string()))
    } else {
        Err(internal(format!("Bevy captured but did not write {path}; the engine log has its error")))
    })
}

/// Gives up on a shot so nothing writes it. One Bevy is reading back keeps its entity, without
/// its observers, for Bevy to finish and despawn: Bevy inserts `Captured` on it when it does. A
/// viewport shot with no viewer resolves when one attaches; a shot of a window Bevy never captures
/// (one with no wgpu surface and no ring) keeps its entity.
fn abandon(world: &mut World, e: Entity) {
    let Ok(mut entity) = world.get_entity_mut(e) else { return };
    if !entity.contains::<Capturing>() || entity.contains::<Captured>() {
        entity.despawn();
        return;
    }
    let observers = entity.take::<ObservedBy>().map(|o| o.get().to_vec()).unwrap_or_default();
    entity.remove::<Shot>();
    for o in observers {
        world.entity_mut(o).despawn();
    }
}

/// Forgets the shots of requests that closed, and the shot entities still waiting for them.
pub(crate) fn forget_closed(mut shots: ResMut<Shots>, mut commands: Commands) {
    shots.1 += 1;
    shots.0.retain(|_, s| {
        let keep = std::mem::take(&mut s.touched);
        if !keep && let Progress::Pending(e) = s.state {
            commands.queue(move |world: &mut World| abandon(world, e));
        }
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::diagnostic::{FrameCountPlugin, update_frame_count};

    #[derive(Resource, Default)]
    struct Seen(Vec<Option<u64>>);

    /// Each render is recorded during the next update, as a pipelined render thread finishes it. A
    /// system after `Last`, where the method runs, still reads a render that has finished.
    #[test]
    fn after_last_the_render_read_has_finished() {
        let mut app = App::new();
        app.add_plugins((FrameCountPlugin, ReadyPlugin)).init_resource::<Seen>().add_systems(
            Last,
            (|world: &mut World| {
                let waiting = pipelines_waiting(world);
                world.resource_mut::<Seen>().0.push(waiting);
            })
            .after(update_frame_count),
        );
        app.finish();
        app.cleanup();
        let renders = app.world().resource::<PipelinesWaiting>().clone();
        let mut drawing = None;
        for _ in 0..6 {
            app.update();
            if let Some(frame) = drawing {
                renders.0.lock().unwrap().push_back((frame, 0));
            }
            drawing = Some(app.world().resource::<FrameCount>().0);
        }
        let seen = &app.world().resource::<Seen>().0;
        assert_eq!(seen[2..], [Some(0); 4], "every update after the first render reads it: {seen:?}");
    }

    /// A ready frame, then one with a pipeline waiting, then ready frames: the shot's clock is its
    /// `at` steps, not one more for the early ready frame.
    #[test]
    fn the_shot_clock_skips_a_ready_frame_before_a_wait() {
        let mut app = App::new();
        app.add_plugins((bevy::time::TimePlugin, FrameCountPlugin, ReadyPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .insert_resource(FixedStep)
            .insert_resource({
                let mut shot = ShotWhenReady::new(std::env::temp_dir().join("bevy-ts-remote-never-written.png"));
                shot.at = 5;
                shot
            });
        app.finish();
        app.cleanup();
        let renders = app.world().resource::<PipelinesWaiting>().clone();
        // Pipelines waiting after each render: none, then one, then none.
        let mut waiting = [0, 1].into_iter().chain(std::iter::repeat(0));
        let mut elapsed = None;
        for _ in 0..20 {
            let drawing = app.world().resource::<FrameCount>().0;
            app.update();
            renders.0.lock().unwrap().push_back((drawing, waiting.next().unwrap()));
            if elapsed.is_none() && !app.world().contains_resource::<ShotWhenReady>() {
                elapsed = Some(app.world().resource::<Time>().elapsed());
            }
        }
        assert_eq!(elapsed, Some(STEP * 5), "the shot's clock");
    }
}
