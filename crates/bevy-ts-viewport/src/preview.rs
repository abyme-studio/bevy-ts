//! Asset previews (openspec change `asset-preview`): a mesh or glTF scene loaded into a scene of
//! its own on a render layer from 20 to 29, under a root far from the game, lit by a rig only that
//! layer sees and seen by a camera that renders into a preview viewport's ring. The viewport's
//! input orbits, zooms and reframes the camera here in Rust, so no JS runs for it.

use crate::assets::{AssetRoot, kind};
use crate::edit::EditorOnly;
use crate::methods::{internal, invalid, required, to_value, CRATE};
use crate::ring::ViewportWindow;
use crate::shared::{Shared, SharedRes};
use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::asset::LoadState;
use bevy_ts_animation::players;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Exposure, RenderTarget};
use bevy::light::EnvironmentMapLight;
use bevy::prelude::*;
use bevy::remote::BrpResult;
use bevy::render::view::Tonemapping;
use bevy::window::{WindowRef, WindowResolution};
use bevy_ts_remote::protocol::{self as p, ViewportInput};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The render layers previews take, one each: ten previews at once.
pub const PREVIEW_LAYERS: std::ops::Range<usize> = 20..30;
/// How long a preview outlives its viewport, so a pane that closes and reopens does not reload.
pub const KEEP_ALIVE: Duration = Duration::from_secs(30);

/// How long a preview outlives its viewport: [`KEEP_ALIVE`], unless an app replaces it (a test
/// that opens many previews on a loaded machine, where opening them outlasts it).
#[derive(Resource, Clone, Copy)]
pub struct PreviewKeepAlive(pub Duration);

impl Default for PreviewKeepAlive {
    fn default() -> Self {
        PreviewKeepAlive(KEEP_ALIVE)
    }
}
/// Frames to let a spawned scene's bounds settle before framing it.
const SETTLE_FRAMES: u32 = 3;
/// A preview's size before a viewer attaches, in points at 2x.
const DEFAULT_SIZE: (f32, f32, f32) = (512.0, 512.0, 2.0);

/// On every entity of a preview scene: never part of the game, its lists or its source. Like
/// [`EditorOnly`] it requires bevy-ts's `ScriptHidden`, so TS queries and `bevy-ts/entities`
/// skip it.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(bevy_ts::ScriptHidden)]
pub struct PreviewOnly;

/// The layer a preview's entities render on; children spawned under it inherit it.
#[derive(Component, Debug, Clone, Copy)]
struct PreviewMember(usize);

/// A preview's camera.
#[derive(Component, Debug, Clone, Copy)]
pub struct PreviewCamera(pub u32);

/// Where a preview's camera is: around `focus`, `distance` away, turned by `yaw` and `pitch`.
#[derive(Clone, Copy, Debug)]
struct Orbit {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { focus: Vec3::ZERO, yaw: std::f32::consts::FRAC_PI_4, pitch: -0.45, distance: 4.0 }
    }
}

impl Orbit {
    fn transform(&self) -> Transform {
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
        Transform { translation: self.focus + rotation * Vec3::Z * self.distance, rotation, ..default() }
    }
}

/// A glTF's animations and the graph that plays them.
struct Animations {
    names: Vec<String>,
    clips: Vec<Handle<AnimationClip>>,
    graph: Option<(Handle<AnimationGraph>, Vec<AnimationNodeIndex>)>,
    current: Option<usize>,
    playing: bool,
    speed: f32,
}

pub(crate) struct Preview {
    path: String,
    kind: &'static str,
    layer: usize,
    root: Entity,
    subject: Entity,
    camera: Entity,
    window: Entity,
    pub(crate) shared: Arc<Shared>,
    /// The viewport attached to it now, if any.
    pub(crate) viewport: Option<u32>,
    last_attached: Instant,
    /// Since when no viewport has been attached; it closes `KEEP_ALIVE` later.
    detached_at: Option<Instant>,
    orbit: Orbit,
    frames: u32,
    framed: bool,
    drag: Option<Vec2>,
    animations: Animations,
}

enum Loading {
    Mesh(Handle<Mesh>),
    Gltf(Handle<bevy::gltf::Gltf>),
}

/// Every preview, those loading, and which viewport shows which.
#[derive(Resource, Default)]
pub(crate) struct Previews {
    next: u32,
    pub(crate) open: HashMap<u32, Preview>,
    loading: HashMap<String, Loading>,
    next_viewport: u32,
    /// Viewport ids to the preview each shows.
    pub(crate) viewports: HashMap<u32, u32>,
}

impl Preview {
    pub(crate) fn window(&self) -> Entity {
        self.window
    }
}

impl Previews {
    /// The preview a viewport id shows.
    pub(crate) fn by_viewport(&self, viewport: u32) -> Option<&Preview> {
        self.viewports.get(&viewport).and_then(|id| self.open.get(id))
    }
}

pub(crate) fn build(app: &mut App) {
    app.init_resource::<Previews>()
        .init_resource::<PreviewKeepAlive>()
        .add_observer(inherit)
        .add_systems(First, drain)
        .add_systems(Update, settle)
        .add_systems(PostUpdate, (keep_alive, place_cameras).chain().before(TransformSystems::Propagate));
}

pub(crate) fn add_methods(app: &mut App) {
    use bevy_ts_remote::add_method;
    add_method(app, CRATE, "bevy-ts/asset_preview", preview);
    add_method(app, CRATE, "bevy-ts/asset_preview_control", control);
    add_method(app, CRATE, "bevy-ts/asset_preview_close", close_method);
}

/// A preview's root: far from the game's origin and from other previews, so the game's physics
/// and spatial queries never meet it.
fn far(layer: usize) -> Vec3 {
    Vec3::new(10_000.0 + 1_000.0 * (layer - PREVIEW_LAYERS.start) as f32, -10_000.0, 0.0)
}

/// An entity parented under a preview joins it: its marker and layer, down its subtree. Scene
/// spawning parents in any order, so the subtree is walked whole. A camera that came with the
/// asset stays off.
fn inherit(add: On<Insert<ChildOf>>, mut commands: Commands, members: Query<&PreviewMember>, parents: Query<&ChildOf>, children: Query<&Children>, cameras: Query<(), (With<Camera>, Without<PreviewCamera>)>) {
    let Ok(parent) = parents.get(add.entity) else { return };
    let Ok(&PreviewMember(layer)) = members.get(parent.parent()) else { return };
    let mut stack = vec![add.entity];
    while let Some(e) = stack.pop() {
        if members.get(e).is_err() || e == add.entity {
            commands.entity(e).insert((PreviewOnly, PreviewMember(layer), RenderLayers::layer(layer)));
            if cameras.contains(e) {
                commands.entity(e).insert(Camera { is_active: false, ..default() });
            }
        }
        if let Ok(c) = children.get(e) {
            stack.extend(c.iter());
        }
    }
}

fn answer(path: &str, id: Option<u32>, kind: &str, animations: &[String], pending: bool) -> BrpResult {
    to_value(p::AssetPreviewResult { path: path.into(), preview: id, kind: kind.into(), animations: animations.to_vec(), pending })
}

fn preview(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let params: p::AssetPreviewParams = required(params)?;
    let path = params.path;
    let k = kind(&path);
    if k != "mesh" {
        return Err(invalid(format!("{path} is a {k}, not a mesh or glTF scene")));
    }
    let root = world.resource::<AssetRoot>().0.clone();
    if !root.join(&path).is_file() {
        return Err(invalid(format!("no asset {path}")));
    }
    if let Some((id, p)) = world.resource::<Previews>().open.iter().find(|(_, p)| p.path == path) {
        return answer(&path, Some(*id), p.kind, &p.animations.names, false);
    }
    let loading = match world.resource_mut::<Previews>().loading.remove(&path) {
        Some(l) => l,
        None if path.to_ascii_lowercase().ends_with(".obj") => Loading::Mesh(world.resource::<AssetServer>().load(path.clone())),
        None => Loading::Gltf(world.resource::<AssetServer>().load(path.clone())),
    };
    let id = match &loading {
        Loading::Mesh(h) => h.id().untyped(),
        Loading::Gltf(h) => h.id().untyped(),
    };
    let server = world.resource::<AssetServer>();
    if let Some(LoadState::Failed(e)) = server.get_load_state(id) {
        return Err(internal(format!("{path} failed to load: {e}")));
    }
    if !server.is_loaded_with_dependencies(id) {
        world.resource_mut::<Previews>().loading.insert(path.clone(), loading);
        return answer(&path, None, k, &[], true);
    }
    let id = open(world, &path, loading)?;
    let p = &world.resource::<Previews>().open[&id];
    answer(&path, Some(id), p.kind, &p.animations.names, false)
}

/// The game's main camera's look: tonemapping, exposure and environment lighting.
fn game_look(world: &mut World) -> (Option<Tonemapping>, Option<Exposure>, Option<EnvironmentMapLight>) {
    let mut q = world.query_filtered::<(&Camera, Option<&Tonemapping>, Option<&Exposure>, Option<&EnvironmentMapLight>), (With<Camera3d>, Without<EditorOnly>, Without<PreviewOnly>)>();
    let mut best: Option<(bool, isize, _)> = None;
    for (c, t, e, env) in q.iter(world) {
        // Edit mode turns game cameras off: any game camera counts, the active one first.
        let rank = (c.is_active, c.order);
        if best.as_ref().is_none_or(|b| (b.0, b.1) < rank) {
            best = Some((rank.0, rank.1, (t.copied(), e.copied(), env.cloned())));
        }
    }
    best.map_or((None, None, None), |b| b.2)
}

/// Builds the scene of a loaded asset and answers its preview id.
fn open(world: &mut World, path: &str, loading: Loading) -> Result<u32, bevy::remote::BrpError> {
    let layer = take_layer(world);
    let layers = RenderLayers::layer(layer);
    let mut names = vec![];
    let mut clips = vec![];
    let root = world
        .spawn((Name::new(format!("bevy-ts preview {path}")), Transform::from_translation(far(layer)), Visibility::default(), PreviewOnly, PreviewMember(layer), layers.clone()))
        .id();
    let subject = match loading {
        Loading::Mesh(mesh) => {
            let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::default());
            world.spawn((Mesh3d(mesh), MeshMaterial3d(material), ChildOf(root))).id()
        }
        Loading::Gltf(handle) => {
            let gltfs = world.resource::<Assets<bevy::gltf::Gltf>>();
            let g = gltfs.get(&handle).ok_or_else(|| internal(format!("{path} is not loaded")))?;
            let scene = g.default_scene.clone().or_else(|| g.scenes.first().cloned()).ok_or_else(|| invalid(format!("{path} has no scene")))?;
            for (i, clip) in g.animations.iter().enumerate() {
                let name = g.named_animations.iter().find(|(_, h)| *h == clip).map_or_else(|| format!("Animation{i}"), |(n, _)| n.to_string());
                names.push(name);
                clips.push(clip.clone());
            }
            world.spawn((bevy::world_serialization::WorldAssetRoot(scene), Transform::default(), Visibility::default(), ChildOf(root))).id()
        }
    };
    // A three-point rig only this layer sees: key, fill and rim.
    for (dir, lux) in [(Vec3::new(-1.0, -1.4, -1.0), 9000.0), (Vec3::new(1.2, -0.5, -0.6), 3000.0), (Vec3::new(0.2, -0.6, 1.4), 5000.0)] {
        world.spawn((DirectionalLight { illuminance: lux, ..default() }, Transform::default().looking_to(dir, Vec3::Y), ChildOf(root)));
    }
    let id = {
        let mut previews = world.resource_mut::<Previews>();
        previews.next += 1;
        previews.next
    };
    let (w, h, scale) = DEFAULT_SIZE;
    let mut resolution = WindowResolution::new((w * scale) as u32, (h * scale) as u32);
    resolution.set_scale_factor(scale);
    let shared = world.resource::<SharedRes>().0.sibling();
    let window = world
        .spawn((Window { title: format!("bevy-ts preview {id}"), resolution, focused: false, ..default() }, ViewportWindow(shared.clone()), PreviewOnly))
        .id();
    let (tonemapping, exposure, env) = game_look(world);
    let orbit = Orbit::default();
    let mut camera = world.spawn((
        Name::new(format!("bevy-ts preview camera {id}")),
        Camera3d::default(),
        Camera { is_active: false, ..default() },
        RenderTarget::Window(WindowRef::Entity(window)),
        Transform::from_translation(far(layer)) * orbit.transform(),
        layers,
        PreviewOnly,
        PreviewCamera(id),
    ));
    if let Some(t) = tonemapping {
        camera.insert(t);
    }
    if let Some(e) = exposure {
        camera.insert(e);
    }
    if let Some(env) = env {
        camera.insert(env);
    }
    let camera = camera.id();
    let now = Instant::now();
    let kind = kind(path);
    world.resource_mut::<Previews>().open.insert(
        id,
        Preview {
            path: path.to_string(),
            kind,
            layer,
            root,
            subject,
            camera,
            window,
            shared,
            viewport: None,
            last_attached: now,
            detached_at: Some(now),
            orbit,
            frames: 0,
            framed: false,
            drag: None,
            animations: Animations { names, clips, graph: None, current: None, playing: false, speed: 1.0 },
        },
    );
    Ok(id)
}

/// A free layer of the pool, closing the least recently attached preview when all ten are taken.
fn take_layer(world: &mut World) -> usize {
    let previews = world.resource::<Previews>();
    if let Some(l) = PREVIEW_LAYERS.clone().find(|l| previews.open.values().all(|p| p.layer != *l)) {
        return l;
    }
    let (&oldest, p) = previews.open.iter().min_by_key(|(_, p)| p.last_attached).expect("ten previews are open");
    let layer = p.layer;
    close(world, oldest);
    layer
}

/// Despawns a preview's scene, frees its layer and ends its viewport.
pub(crate) fn close(world: &mut World, id: u32) -> bool {
    let Some(p) = world.resource_mut::<Previews>().open.remove(&id) else { return false };
    p.shared.close();
    world.resource_mut::<Previews>().viewports.retain(|_, v| *v != id);
    for e in [p.root, p.camera, p.window] {
        if let Ok(e) = world.get_entity_mut(e) {
            e.despawn();
        }
    }
    if let Some((graph, _)) = p.animations.graph {
        world.resource_mut::<Assets<AnimationGraph>>().remove(&graph);
    }
    true
}

fn close_method(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let params: p::AssetPreviewCloseParams = required(params)?;
    to_value(p::AssetPreviewCloseResult { closed: close(world, params.preview) })
}

/// Attaches a new viewport to preview `id`, replacing the one it had; answers the viewport id.
pub(crate) fn attach(world: &mut World, id: u32, name: &str, w: f64, h: f64, scale: f64, hz: Option<f64>) -> Result<u32, bevy::remote::BrpError> {
    let mut previews = world.resource_mut::<Previews>();
    let previews = &mut *previews;
    let p = previews.open.get_mut(&id).ok_or_else(|| invalid(format!("no preview {id}")))?;
    if let Some(hz) = hz {
        p.shared.set_hz(hz);
    }
    p.shared.open(name, INPUTS).map_err(internal)?;
    previews.next_viewport += 1;
    let viewport = previews.next_viewport;
    if let Some(old) = p.viewport.replace(viewport) {
        previews.viewports.remove(&old);
    }
    previews.viewports.insert(viewport, id);
    p.last_attached = Instant::now();
    p.detached_at = None;
    let window = p.window;
    resize(world, window, w, h, scale);
    Ok(viewport)
}

/// Ends a preview viewport's channel; the scene stays `KEEP_ALIVE`.
pub(crate) fn detach(world: &mut World, viewport: u32) -> Option<bool> {
    let mut previews = world.resource_mut::<Previews>();
    let id = *previews.viewports.get(&viewport)?;
    let p = previews.open.get_mut(&id)?;
    let was = p.shared.service_name().is_some();
    p.shared.close();
    p.detached_at.get_or_insert_with(Instant::now);
    Some(was)
}

/// Sets a preview window's size, as `viewport_resize` does the game's.
pub(crate) fn resize(world: &mut World, window: Entity, w: f64, h: f64, scale: f64) -> Value {
    if let Some(mut win) = world.get_mut::<Window>(window) {
        let (pw, ph) = ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32);
        win.resolution.set_scale_factor(scale as f32);
        win.resolution.set_physical_resolution(pw, ph);
    }
    window_size(world, window)
}

pub(crate) fn window_size(world: &World, window: Entity) -> Value {
    match world.get::<Window>(window) {
        Some(w) => json!({
            "w": w.resolution.width(), "h": w.resolution.height(), "scale": w.resolution.scale_factor(),
            "pixelW": w.resolution.physical_width(), "pixelH": w.resolution.physical_height(),
        }),
        None => json!({ "w": 0, "h": 0, "scale": 1, "pixelW": 0, "pixelH": 0 }),
    }
}

/// Applies what each preview's viewer sent: resizes, and input to the orbit camera.
fn drain(world: &mut World) {
    let ids: Vec<u32> = world.resource::<Previews>().open.keys().copied().collect();
    for id in ids {
        let Some(shared) = world.resource::<Previews>().open.get(&id).map(|p| p.shared.clone()) else { continue };
        for (_, m) in shared.drain() {
            match m {
                crate::mach::Incoming::Gone => {
                    shared.close();
                    if let Some(p) = world.resource_mut::<Previews>().open.get_mut(&id) {
                        p.detached_at.get_or_insert_with(Instant::now);
                    }
                }
                crate::mach::Incoming::Message { method, params, port } => {
                    if let Some(port) = port {
                        crate::mach::release_port(port);
                    }
                    match method.as_str() {
                        "resize" => match serde_json::from_value::<p::ViewportResizeParams>(params) {
                            Ok(r) if r.w > 0.0 && r.h > 0.0 && r.scale > 0.0 => {
                                if let Some(hz) = r.hz {
                                    shared.set_hz(hz);
                                }
                                let window = world.resource::<Previews>().open[&id].window;
                                resize(world, window, r.w, r.h, r.scale);
                            }
                            _ => warn!("preview viewport: malformed resize"),
                        },
                        "input" => match serde_json::from_value::<ViewportInput>(params.clone()) {
                            Ok(e) => input(world, id, e),
                            Err(_) => warn!("preview viewport: malformed input event {params}"),
                        },
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

const LOOK: f32 = 0.008;

/// What a preview's orbit camera reads, which its viewer's `hello` is answered with.
const INPUTS: &[&str] = &["pointer", "wheel", "gesture", "key", "visible"];

/// Orbit on a drag, zoom on the wheel or a pinch, reframe on `f`.
fn input(world: &mut World, id: u32, e: ViewportInput) {
    let mut reframe = false;
    {
        let mut previews = world.resource_mut::<Previews>();
        let Some(p) = previews.open.get_mut(&id) else { return };
        let o = &mut p.orbit;
        match e {
            ViewportInput::PointerEvent(e) => {
                let at = Vec2::new(e.x as f32, e.y as f32);
                match e.kind {
                    p::PointerEventKind::Down => p.drag = Some(at),
                    p::PointerEventKind::Up | p::PointerEventKind::Leave => p.drag = None,
                    p::PointerEventKind::Move | p::PointerEventKind::Enter => {
                        if let Some(from) = p.drag {
                            let d = match (e.dx, e.dy) {
                                (Some(dx), Some(dy)) => Vec2::new(dx as f32, dy as f32),
                                _ => at - from,
                            };
                            o.yaw -= d.x * LOOK;
                            o.pitch = (o.pitch - d.y * LOOK).clamp(-crate::edit::PITCH_LIMIT, crate::edit::PITCH_LIMIT);
                            p.drag = Some(at);
                        }
                    }
                }
            }
            ViewportInput::WheelEvent(e) => {
                let dolly = match e.unit {
                    p::WheelEventUnit::Line => e.dy as f32 * 0.1,
                    p::WheelEventUnit::Pixel => e.dy as f32 * 0.005,
                };
                o.distance = (o.distance * (1.0 - dolly).clamp(0.2, 5.0)).max(0.01);
            }
            ViewportInput::GestureEvent(e) if e.kind == p::GestureEventKind::Pinch => {
                let d = e.delta.unwrap_or(0.0) as f32;
                o.distance = (o.distance * (1.0 - d).clamp(0.2, 5.0)).max(0.01);
            }
            ViewportInput::KeyEvent(e) if e.state == p::KeyEventState::Down && e.code == "KeyF" => reframe = true,
            ViewportInput::VisibleEvent(e) => p.shared.set_visible(e.on),
            _ => {}
        }
    }
    if reframe {
        frame(world, id);
    }
}

/// Points the camera at the scene's bounds, keeping its direction.
pub(crate) fn frame(world: &mut World, id: u32) {
    let Some(subject) = world.resource::<Previews>().open.get(&id).map(|p| p.subject) else { return };
    let Some((lo, hi)) = crate::edit::bounds(world, subject) else { return };
    let mut previews = world.resource_mut::<Previews>();
    let Some(p) = previews.open.get_mut(&id) else { return };
    let base = far(p.layer);
    let radius = ((hi - lo).length() / 2.0).max(0.01);
    p.orbit.focus = (lo + hi) / 2.0 - base;
    p.orbit.distance = radius / (std::f32::consts::FRAC_PI_4 / 2.0).sin() * 1.1;
}

/// Frames a scene once it has spawned, and gives its animation players the preview's graph.
fn settle(world: &mut World) {
    let ids: Vec<u32> = world.resource::<Previews>().open.iter().filter(|(_, p)| !p.framed).map(|(id, _)| *id).collect();
    for id in ids {
        // A glTF's scene spawns some frames after the preview opens, more on a loaded machine.
        if crate::assets::scenes_pending(world, world.resource::<Previews>().open[&id].subject) {
            continue;
        }
        let frames = {
            let mut previews = world.resource_mut::<Previews>();
            let p = previews.open.get_mut(&id).unwrap();
            p.frames += 1;
            p.frames
        };
        if frames < SETTLE_FRAMES {
            continue;
        }
        frame(world, id);
        attach_graph(world, id);
        world.resource_mut::<Previews>().open.get_mut(&id).unwrap().framed = true;
    }
}

fn attach_graph(world: &mut World, id: u32) {
    let (root, clips) = {
        let p = &world.resource::<Previews>().open[&id];
        if p.animations.clips.is_empty() || p.animations.graph.is_some() {
            return;
        }
        (p.root, p.animations.clips.clone())
    };
    let (graph, nodes) = AnimationGraph::from_clips(clips);
    let graph = world.resource_mut::<Assets<AnimationGraph>>().add(graph);
    for e in players(world, root) {
        world.entity_mut(e).insert(AnimationGraphHandle(graph.clone()));
    }
    world.resource_mut::<Previews>().open.get_mut(&id).unwrap().animations.graph = Some((graph, nodes));
}

fn control(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let params: p::AssetPreviewControlParams = required(params)?;
    let id = params.preview;
    let root = world.resource::<Previews>().open.get(&id).map(|p| p.root).ok_or_else(|| invalid(format!("no preview {id}")))?;
    attach_graph(world, id);
    {
        let mut previews = world.resource_mut::<Previews>();
        let a = &mut previews.open.get_mut(&id).unwrap().animations;
        if let Some(name) = &params.animation {
            let i = a.names.iter().position(|n| n == name).ok_or_else(|| invalid(format!("no animation {name}; it has {:?}", a.names)))?;
            a.current = Some(i);
            a.playing = params.play.unwrap_or(true);
        } else if let Some(play) = params.play {
            if a.current.is_none() && !a.names.is_empty() {
                a.current = Some(0);
            }
            a.playing = play;
        }
        if let Some(s) = params.speed {
            a.speed = s as f32;
        }
    }
    let (current, playing, speed, nodes) = {
        let a = &world.resource::<Previews>().open[&id].animations;
        (a.current, a.playing, a.speed, a.graph.as_ref().map(|g| g.1.clone()).unwrap_or_default())
    };
    for e in players(world, root) {
        let Some(mut player) = world.get_mut::<AnimationPlayer>(e) else { continue };
        if let Some(node) = current.and_then(|i| nodes.get(i)).copied() {
            if !player.is_playing_animation(node) {
                player.stop_all();
                player.play(node).repeat();
            }
            if let Some(active) = player.animation_mut(node) {
                active.set_speed(speed);
                if playing { active.resume() } else { active.pause() };
            }
        }
    }
    let a = &world.resource::<Previews>().open[&id].animations;
    to_value(p::AssetPreviewControlResult {
        preview: id,
        animation: a.current.map(|i| a.names[i].clone()),
        playing: a.playing && a.current.is_some(),
        speed: a.speed as f64,
        animations: a.names.clone(),
    })
}

/// Closes previews whose viewport has been gone [`PreviewKeepAlive`].
fn keep_alive(world: &mut World) {
    let limit = world.resource::<PreviewKeepAlive>().0;
    let due: Vec<u32> = world
        .resource::<Previews>()
        .open
        .iter()
        .filter(|(_, p)| !p.shared.attached() && p.detached_at.is_some_and(|t| t.elapsed() >= limit))
        .map(|(id, _)| *id)
        .collect();
    for id in due {
        close(world, id);
    }
}

/// Each preview camera renders only while its viewport is attached and visible, from its orbit.
fn place_cameras(previews: Res<Previews>, mut cameras: Query<(&mut Camera, &mut Transform), With<PreviewCamera>>) {
    for p in previews.open.values() {
        let Ok((mut c, mut t)) = cameras.get_mut(p.camera) else { continue };
        let on = p.shared.should_render();
        if c.is_active != on {
            c.is_active = on;
        }
        let want = Transform::from_translation(far(p.layer)) * p.orbit.transform();
        if *t != want {
            *t = want;
        }
    }
}

/// `{ translation, rotation, fov, scale }` of a preview's camera, after applying `params`.
pub(crate) fn camera(world: &mut World, viewport: u32, params: &p::ViewportCameraParams) -> BrpResult {
    let id = *world.resource::<Previews>().viewports.get(&viewport).ok_or_else(|| invalid(format!("no viewport {viewport}")))?;
    if params.frame == Some(true) {
        frame(world, id);
    }
    let (camera, base) = {
        let mut previews = world.resource_mut::<Previews>();
        let p = previews.open.get_mut(&id).ok_or_else(|| invalid(format!("no preview {id}")))?;
        let base = far(p.layer);
        if params.translation.is_some() || params.rotation.is_some() {
            let mut t = Transform::from_translation(base) * p.orbit.transform();
            if let Some(v) = &params.translation {
                t.translation = base + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32);
            }
            if let Some(q) = &params.rotation {
                t.rotation = Quat::from_xyzw(q[0] as f32, q[1] as f32, q[2] as f32, q[3] as f32).normalize();
            }
            let (yaw, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
            p.orbit = Orbit { focus: t.translation - base + t.forward().as_vec3() * p.orbit.distance, yaw, pitch, distance: p.orbit.distance };
        }
        (p.camera, base)
    };
    if let Some(f) = params.fov
        && let Some(mut proj) = world.get_mut::<Projection>(camera)
        && let Projection::Perspective(pp) = &mut *proj
    {
        pp.fov = f as f32;
    }
    let t = Transform::from_translation(base) * world.resource::<Previews>().open[&id].orbit.transform();
    let fov = match world.get::<Projection>(camera) {
        Some(Projection::Perspective(p)) => Some(p.fov),
        _ => None,
    };
    // Poses are the preview scene's own, relative to its root.
    Ok(json!({ "translation": (t.translation - base).to_array(), "rotation": t.rotation.to_array(), "fov": fov, "scale": null }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_sit_far_apart_on_their_own_layers() {
        let roots: Vec<Vec3> = PREVIEW_LAYERS.map(far).collect();
        assert_eq!(roots.len(), 10);
        for w in roots.windows(2) {
            assert!(w[0].distance(w[1]) >= 1_000.0);
        }
        assert!(roots[0].length() >= 10_000.0);
    }

    #[test]
    fn the_orbit_looks_at_its_focus() {
        let o = Orbit { focus: Vec3::new(1.0, 2.0, 3.0), yaw: 0.3, pitch: -0.4, distance: 5.0 };
        let t = o.transform();
        assert!((t.translation - o.focus).length() - 5.0 < 1e-4);
        assert!((t.translation + t.forward().as_vec3() * 5.0 - o.focus).length() < 1e-4);
    }
}
