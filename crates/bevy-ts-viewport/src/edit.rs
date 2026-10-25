//! Edit mode: the same world, paused, seen from an editor camera, with engine-drawn gizmos,
//! picking that selects what the source placed, and a selection the editor follows. Works in the
//! hole and in `bevy-ts dev`'s own window alike.

use crate::preview::{PREVIEW_LAYERS, PreviewOnly};
use crate::shared::SharedRes;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::primitives::Aabb;
use bevy::gizmos::config::{GizmoConfigGroup, GizmoConfigStore};
use bevy::gizmos::transform_gizmo::{
    TransformGizmoCamera, TransformGizmoFocus, TransformGizmoMeshMarker, TransformGizmoMode, TransformGizmoRoot, TransformGizmoSettings,
    TransformGizmoSpace, TransformGizmoState,
};
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::gestures::PinchGesture;
use bevy::picking::input::PointerInputSettings;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_ts::EngineMode;
use bevy_ts_tsx::tree::{TsxNode, TsxTree};
use bevy_ts_remote::protocol::{ViewportModeCause as ModeCause, ViewportPlayRefusedReason};
use serde_json::{Value, json};

/// The render layer the edit tools draw on; only the editor camera sees it.
pub const EDITOR_LAYER: usize = 31;
/// The render layer asset thumbnails render on; the editor camera does not see it.
pub const THUMBNAIL_LAYER: usize = 30;
/// Bevy's transform gizmo draws its handles on this layer with an overlay camera of its own.
const BEVY_GIZMO_LAYER: usize = 15;

/// On everything the edit tools spawn: never part of the game, its lists or its source. It
/// requires bevy-ts's `ScriptHidden`, so TS queries and `bevy-ts/entities` skip it.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(bevy_ts::ScriptHidden)]
pub struct EditorOnly;

/// The editor camera: a Rust controller, so it works when TS fails to load.
#[derive(Component, Debug, Clone, Copy)]
pub struct EditorCamera {
    /// The point orbit turns around and dolly moves toward.
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// The edit tools' line gizmos (grid, outlines, icons), on [`EDITOR_LAYER`].
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct EditorGizmos;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Play,
    Edit,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Play => "play",
            Mode::Edit => "edit",
        }
    }
}

/// Which camera the viewport shows: kept across Play and Stop. In edit mode `Auto` and `Editor`
/// show the editor camera with its tools, `Game` the game cameras with the tools off. Play always
/// shows the game cameras.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CameraChoice {
    #[default]
    Auto,
    Editor,
    Game,
}

impl CameraChoice {
    pub fn name(self) -> &'static str {
        match self {
            CameraChoice::Auto => "auto",
            CameraChoice::Editor => "editor",
            CameraChoice::Game => "game",
        }
    }
}

/// The mode in force and what entering edit changed, to undo on play.
#[derive(Resource, Default)]
pub struct EditState {
    pub mode: Mode,
    pub ui: bool,
    pub gamepad_unfocused: bool,
    pub camera_choice: CameraChoice,
    was_paused: bool,
    mouse_was_enabled: bool,
    /// Game cameras and whether each was active on entering edit, or on being spawned in edit:
    /// the flag Play gives back.
    game_cameras: Vec<(Entity, bool)>,
    pub editor_camera: Option<Entity>,
    /// The levels open when play was entered, for its Stop to put back.
    open_levels: Option<bevy_ts_levels::engine::OpenSet>,
}

impl EditState {
    /// Whether the editor camera and its tools are in view: edit mode, and not the game view.
    pub fn editor_view(&self) -> bool {
        self.mode == Mode::Edit && self.camera_choice != CameraChoice::Game
    }
}

/// The JSON-RPC error code of a refused Play.
pub const PLAY_REFUSED: i16 = -32000;

/// Why a Play was refused: the JSON-RPC error's message, and its `data`.
#[derive(Clone, Debug)]
pub struct PlayRefused {
    pub reason: ViewportPlayRefusedReason,
    pub message: String,
}

/// Why a `viewport_mode` call changed nothing.
#[derive(Clone, Debug)]
pub enum ModeRefused {
    Play(PlayRefused),
    /// `camera: "editor"` in, or entering, Play.
    EditorCamera,
}

impl ModeRefused {
    pub const EDITOR_CAMERA: &str = "The editor camera is not available in Play.";
}

/// The mode as last streamed: one owner, viewport's. A call records its change at once; `pause`,
/// `step` and a `restart` reload are caught by the compare in `Last`.
#[derive(Resource, Default)]
pub struct ModeWatch {
    /// Bumped on every change, for the watch.
    pub seq: u64,
    pub last: Option<Value>,
    pub cause: Option<ModeCause>,
    /// `Generations::restarts` when `last` was taken.
    restarts: u64,
}

/// Records the mode now as a change with `cause`.
fn mode_changed(world: &mut World, cause: ModeCause) {
    let now = mode_json(world);
    let restarts = world.get_resource::<bevy_ts::generation::Generations>().map_or(0, |g| g.restarts);
    let mut w = world.resource_mut::<ModeWatch>();
    w.seq += 1;
    w.last = Some(now);
    w.cause = Some(cause);
    w.restarts = restarts;
}

/// `Last`: a mode that differs from the one last streamed is a change; a `restart` reload's when
/// one ran since, else a client's (`pause`, `step`).
fn watch_mode(world: &mut World) {
    let now = mode_json(world);
    let restarts = world.get_resource::<bevy_ts::generation::Generations>().map_or(0, |g| g.restarts);
    let w = world.resource::<ModeWatch>();
    match &w.last {
        None => {
            let mut w = world.resource_mut::<ModeWatch>();
            w.last = Some(now);
            w.restarts = restarts;
        }
        Some(last) if *last != now => {
            let cause = if restarts != w.restarts {
                ModeCause::Restart
            } else if only_cameras(last, &now) {
                ModeCause::Cameras
            } else {
                ModeCause::Client
            };
            mode_changed(world, cause);
        }
        Some(_) => {
            world.resource_mut::<ModeWatch>().restarts = restarts;
        }
    }
}

/// Whether two modes differ only in `gameCameras`.
fn only_cameras(a: &Value, b: &Value) -> bool {
    let strip = |v: &Value| {
        let mut v = v.clone();
        if let Some(o) = v.as_object_mut() {
            o.remove("gameCameras");
        }
        v
    };
    strip(a) == strip(b)
}

/// Where a selection change came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Viewport,
    Editor,
    Engine,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Viewport => "viewport",
            Origin::Editor => "editor",
            Origin::Engine => "engine",
        }
    }
}

/// The selection: the first is primary, the one the gizmo acts on. Node ids are kept so a
/// reload that respawns an entity keeps it selected.
#[derive(Resource)]
pub struct Selection {
    pub items: Vec<(Entity, Option<u64>)>,
    pub origin: Origin,
    /// Bumped on every change, for the watch.
    pub seq: u64,
    pub hovered: Option<Entity>,
}

impl Default for Selection {
    fn default() -> Self {
        Selection { items: vec![], origin: Origin::Engine, seq: 0, hovered: None }
    }
}

impl Selection {
    pub fn primary(&self) -> Option<Entity> {
        self.items.first().map(|(e, _)| *e)
    }

    pub fn set(&mut self, items: Vec<(Entity, Option<u64>)>, origin: Origin) {
        self.items = items;
        self.origin = origin;
        self.seq += 1;
    }
}

/// The transform gizmo's tools: the one writer of `TransformGizmoSettings` mode, space and snap.
/// Kept for the process, across mode switches and reloads.
#[derive(Resource)]
pub struct ViewportTools {
    pub mode: TransformGizmoMode,
    pub space: TransformGizmoSpace,
    pub snap: bool,
    pub origin: Origin,
    /// Bumped on every change, for the watch.
    pub seq: u64,
}

impl Default for ViewportTools {
    fn default() -> Self {
        ViewportTools { mode: TransformGizmoMode::Translate, space: TransformGizmoSpace::World, snap: false, origin: Origin::Engine, seq: 0 }
    }
}

/// The snap steps, as Bevy's editor prototype sets them.
pub const SNAP_TRANSLATE: f64 = 0.5;
pub const SNAP_ROTATE_DEGREES: f64 = 15.0;
pub const SNAP_SCALE: f64 = 0.1;

impl ViewportTools {
    /// Sets what is given; a change bumps the counter with its origin.
    pub fn set(&mut self, mode: Option<TransformGizmoMode>, space: Option<TransformGizmoSpace>, snap: Option<bool>, origin: Origin) {
        let next = (mode.unwrap_or(self.mode), space.unwrap_or(self.space), snap.unwrap_or(self.snap));
        if next != (self.mode, self.space, self.snap) {
            (self.mode, self.space, self.snap) = next;
            self.origin = origin;
            self.seq += 1;
        }
    }
}

/// Copies the tools to Bevy's gizmo settings, writing only what differs. A change during a gizmo
/// drag waits for the release, so a drag keeps the field it started on.
fn apply_tools(tools: Res<ViewportTools>, mut settings: ResMut<TransformGizmoSettings>, gizmo: Res<TransformGizmoState>) {
    if gizmo.active {
        return;
    }
    let snap = |step: f32| tools.snap.then_some(step);
    let want = (tools.mode, tools.space, snap(SNAP_TRANSLATE as f32), snap(SNAP_ROTATE_DEGREES.to_radians() as f32), snap(SNAP_SCALE as f32));
    if (settings.mode, settings.space, settings.snap_translate, settings.snap_rotate, settings.snap_scale) != want {
        (settings.mode, settings.space, settings.snap_translate, settings.snap_rotate, settings.snap_scale) = want;
    }
}

/// How far the editor camera pitches up or down, short of the poles, wherever it turns.
pub const PITCH_LIMIT: f32 = 1.54;

pub(crate) fn build(app: &mut App) {
    app.init_resource::<EditState>()
        .init_resource::<Pending>()
        .init_resource::<Selection>()
        .init_resource::<ViewportTools>()
        .init_resource::<PointerTrack>()
        .init_resource::<ModeWatch>()
        .init_gizmo_group::<EditorGizmos>();
    if !app.is_plugin_added::<bevy::gizmos::transform_gizmo::TransformGizmoPlugin>() {
        app.add_plugins(bevy::gizmos::transform_gizmo::TransformGizmoPlugin);
    }
    {
        let mut store = app.world_mut().resource_mut::<GizmoConfigStore>();
        let (config, _) = store.config_mut::<EditorGizmos>();
        config.render_layers = RenderLayers::layer(EDITOR_LAYER);
        config.enabled = false;
        config.depth_bias = -0.0001;
    }
    app.world_mut().resource_mut::<TransformGizmoSettings>().confine_cursor = false;
    app.add_observer(mark_bevy_gizmo)
        .add_observer(|add: On<Add<TransformGizmoRoot>>, mut commands: Commands| {
            commands.entity(add.entity).insert(EditorOnly);
        })
        .add_observer(|add: On<Add<TransformGizmoMeshMarker>>, mut commands: Commands| {
            commands.entity(add.entity).insert(EditorOnly);
        })
        .add_observer(camera_added)
        .add_systems(Startup, boot_edit)
        .add_systems(First, apply_pending)
        .add_systems(
            Update,
            (
                keep_selection.run_if(|s: Res<EditState>| s.mode == Mode::Edit),
                (edit_keys, camera_controller, click_to_select, gizmo_focus, draw_gizmos).chain().run_if(|s: Res<EditState>| s.editor_view()),
            )
                .chain(),
        )
        .add_systems(Update, apply_tools.after(edit_keys))
        .add_systems(Last, watch_mode);
}

/// Bevy's transform gizmo spawns its handles and an overlay camera at startup: they are the
/// editor's, and its camera renders only in edit mode, after the editor camera.
fn mark_bevy_gizmo(add: On<Add<Camera>>, mut commands: Commands, layers: Query<&RenderLayers>) {
    let e = add.entity;
    if layers.get(e).is_ok_and(|l| *l == RenderLayers::layer(BEVY_GIZMO_LAYER)) {
        commands.entity(e).insert((EditorOnly, Camera { order: -9, is_active: false, ..default() }));
    }
}

/// A game camera spawned in edit mode (a reload's, a TS system's) keeps its flag for play; in the
/// editor view it stays off until then, in the game view it is left as it came.
fn camera_added(add: On<Add<Camera>>, mut state: ResMut<EditState>, mut cameras: Query<&mut Camera, (Without<EditorOnly>, Without<PreviewOnly>)>) {
    if state.mode != Mode::Edit {
        return;
    }
    if let Ok(mut c) = cameras.get_mut(add.entity) {
        state.game_cameras.retain(|(e, _)| *e != add.entity);
        state.game_cameras.push((add.entity, c.is_active));
        if state.editor_view() {
            c.is_active = false;
        }
    }
}

/// Cameras a mode switch turns on or off, all at the next `First`. The switch runs in
/// `RemoteLast`, after PostUpdate built what the renderer reads for each active camera (a shadowed
/// light's cascades, their frusta and what each sees): a camera turned on there renders without
/// them and panics, and one turned off there leaves the window a frame with no camera, cleared
/// blank. Deferred, the old cameras render the frame PostUpdate built for them.
#[derive(Resource, Default)]
struct Pending(Vec<(Entity, bool)>);

fn apply_pending(mut pending: ResMut<Pending>, mut cameras: Query<&mut Camera>) {
    for (e, on) in pending.0.drain(..) {
        if let Ok(mut c) = cameras.get_mut(e)
            && c.is_active != on
        {
            c.is_active = on;
        }
    }
}

/// Turns a camera on or off at the next `First`.
fn set_active(world: &mut World, e: Entity, on: bool) {
    let mut pending = world.resource_mut::<Pending>();
    pending.0.retain(|(p, _)| *p != e);
    pending.0.push((e, on));
}

/// Whether a camera is on, or will be at the next `First`.
fn is_active(world: &World, e: Entity, c: &Camera) -> bool {
    world.resource::<Pending>().0.iter().find(|(p, _)| *p == e).map_or(c.is_active, |(_, on)| *on)
}

/// `Startup`: an engine started with `bevy-ts dev --edit` (`EngineMode::Edit` from the first
/// frame) comes up in edit mode, as a later switch makes it; its first play resumes.
fn boot_edit(world: &mut World) {
    let edit = world.get_resource::<State<EngineMode>>().is_some_and(|s| *s.get() == EngineMode::Edit);
    if !edit || world.resource::<EditState>().mode == Mode::Edit {
        return;
    }
    enter_edit(world);
    world.resource_mut::<EditState>().was_paused = false;
    apply_ui(world);
    if let Some(s) = world.get_resource::<SharedRes>() {
        s.0.set_edit(true);
    }
}

/// The game cameras Play would turn on: alive, not the editor's or a preview's, and their own flag
/// on (the one remembered in edit mode; in play, `is_active` with a pending switch).
pub fn game_cameras_count(world: &World) -> u32 {
    let s = world.resource::<EditState>();
    let game = |e: Entity| world.get_entity(e).is_ok_and(|r| r.contains::<Camera>() && !r.contains::<EditorOnly>() && !r.contains::<PreviewOnly>());
    if s.mode == Mode::Edit {
        return s.game_cameras.iter().filter(|(e, on)| *on && game(*e)).count() as u32;
    }
    let Some(mut q) = world.try_query_filtered::<(Entity, &Camera), (Without<EditorOnly>, Without<PreviewOnly>)>() else { return 0 };
    q.iter(world).filter(|(e, c)| is_active(world, *e, c)).count() as u32
}

/// Whether a Play can run: a generation is live and the newest save did not fail. A Stop's
/// rebuild is not a save: it neither hides a failed compile nor counts as one.
pub fn play_refused(world: &World) -> Option<PlayRefused> {
    let newest = world.get_resource::<bevy_ts::generation::ReloadLog>().and_then(|l| l.newest_save.as_ref());
    if let Some(r) = newest.filter(|r| r.status == bevy_ts::generation::ReloadStatus::Failed) {
        let message = match r.errors.first() {
            Some(e) => match &e.location {
                Some(at) => format!("{}:{}: {}", at.file, at.line, e.message),
                None => e.message.clone(),
            },
            None => "the newest compile failed".to_string(),
        };
        return Some(PlayRefused { reason: ViewportPlayRefusedReason::CompileErrors, message });
    }
    // An app without bevy-ts plays as it is.
    let none = world.get_resource::<bevy_ts::generation::Generations>().is_some_and(|g| g.live == 0);
    none.then(|| PlayRefused { reason: ViewportPlayRefusedReason::NoGeneration, message: "The game has not compiled yet.".to_string() })
}

/// Switches mode and camera; answers the mode in force. A Play that can't run, or the editor
/// camera in Play, changes nothing.
pub fn set_mode(world: &mut World, mode: Mode, ui: Option<bool>, gamepad_unfocused: Option<bool>, camera: Option<CameraChoice>) -> Result<Value, ModeRefused> {
    let current = world.resource::<EditState>().mode;
    if mode == Mode::Play && camera == Some(CameraChoice::Editor) {
        mode_changed(world, ModeCause::Failed);
        return Err(ModeRefused::EditorCamera);
    }
    if mode == Mode::Play
        && current == Mode::Edit
        && let Some(refused) = play_refused(world)
    {
        mode_changed(world, ModeCause::Failed);
        return Err(ModeRefused::Play(refused));
    }
    let choice_was = world.resource::<EditState>().camera_choice;
    if let Some(c) = camera {
        world.resource_mut::<EditState>().camera_choice = c;
    }
    if let Some(g) = gamepad_unfocused {
        world.resource_mut::<EditState>().gamepad_unfocused = g;
        if let Some(s) = world.get_resource::<SharedRes>() {
            s.0.set_gamepad_unfocused(g);
        }
    }
    if let Some(ui) = ui {
        world.resource_mut::<EditState>().ui = ui;
    }
    if mode != current {
        // `EngineMode` follows now, so `OnExit`/`OnEnter` run before the next frame's systems
        // and before the first play's `Startup`. The out-of-band `StateTransition` also applies
        // any other pending state (a TS one too), as the next frame's would.
        if let Some(mut next) = world.get_resource_mut::<NextState<EngineMode>>() {
            next.set(match mode {
                Mode::Edit => EngineMode::Edit,
                Mode::Play => EngineMode::Play,
            });
            // Entering Play runs the stages from here on, so TS `OnEnter(EngineMode.Play)` and
            // the transitions held while editing run now too.
            let was = world.get_resource::<bevy_ts::runtime::ScriptClock>().map(|c| c.running_this_frame);
            if let (Mode::Play, Some(mut clock)) = (mode, world.get_resource_mut::<bevy_ts::runtime::ScriptClock>()) {
                clock.running_this_frame = true;
            }
            // Triggers raised in edit mode never reach the game, nor its `OnEnter(Play)`.
            if let (Mode::Play, Some(mut q)) = (mode, world.get_resource_mut::<bevy_ts::observers::ObserverQueue>()) {
                q.0.clear();
            }
            let _ = world.try_run_schedule(bevy::state::state::StateTransition);
            if let (Some(was), Some(mut clock)) = (was, world.get_resource_mut::<bevy_ts::runtime::ScriptClock>()) {
                clock.running_this_frame = was;
            }
        }
        match mode {
            Mode::Edit => {
                enter_edit(world);
                stop(world);
            }
            Mode::Play => enter_play(world),
        }
    } else if mode == Mode::Edit && world.resource::<EditState>().camera_choice != choice_was {
        // The view alone: no Stop, nothing in the world changes.
        apply_view(world);
    }
    apply_ui(world);
    if let Some(s) = world.get_resource::<SharedRes>() {
        s.0.set_edit(mode == Mode::Edit);
    }
    let now = mode_json(world);
    if world.resource::<ModeWatch>().last.as_ref() != Some(&now) {
        mode_changed(world, ModeCause::Client);
    }
    Ok(now)
}

/// A Stop, after `OnExit(Play)` and `enter_edit`: the TS world is rebuilt from the live
/// generation, the levels open at Play come back and settle, the clocks start again from 0, and
/// the selection follows its nodes.
fn stop(world: &mut World) {
    // Nothing TS made to throw away before a generation was applied.
    if world.contains_resource::<bevy_ts::generation::LiveSet>() {
        let _ = bevy_ts::generation::rebuild(world);
    }
    if world.contains_resource::<bevy_ts_levels::engine::Levels>() {
        if let Some(set) = world.resource_mut::<EditState>().open_levels.take() {
            bevy_ts_levels::engine::restore_open(world, &set);
        }
        bevy_ts_levels::engine::settle(world);
    }
    bevy_ts::runtime::reset_clock(world);
    follow_selection(world, Origin::Engine);
}

pub fn mode_json(world: &World) -> Value {
    let s = world.resource::<EditState>();
    let paused = world.get_resource::<bevy_ts::runtime::ScriptClock>().is_some_and(|c| c.paused);
    let started = bevy_ts::generation::started(world);
    json!({
        "mode": s.mode.name(),
        "ui": s.ui,
        "gamepadUnfocused": s.gamepad_unfocused,
        "paused": paused,
        "started": started,
        "camera": s.camera_choice.name(),
        "gameCameras": game_cameras_count(world),
    })
}

fn set_paused(world: &mut World, paused: bool) {
    if let Some(mut clock) = world.get_resource_mut::<bevy_ts::runtime::ScriptClock>() {
        clock.paused = paused;
        if !paused {
            clock.step_frames = 0;
        }
    } else if let Some(mut time) = world.get_resource_mut::<Time<Virtual>>() {
        if paused { time.pause() } else { time.unpause() }
    }
}

fn is_paused(world: &World) -> bool {
    match world.get_resource::<bevy_ts::runtime::ScriptClock>() {
        Some(c) => c.paused,
        None => world.get_resource::<Time<Virtual>>().is_some_and(|t| t.is_paused()),
    }
}

fn enter_edit(world: &mut World) {
    let was_paused = is_paused(world);
    set_paused(world, true);
    // Game cameras remembered; the first active one on the primary window gives the editor camera
    // its first pose.
    let mut q = world.query_filtered::<(Entity, &Camera, &GlobalTransform, Option<&Projection>, Option<&RenderTarget>), (Without<EditorOnly>, Without<PreviewOnly>)>();
    let mut cameras = vec![];
    let mut pose: Option<(GlobalTransform, Projection)> = None;
    for (e, c, gt, proj, target) in q.iter(world) {
        let active = is_active(world, e, c);
        let on_window = target.is_none_or(|t| matches!(t, RenderTarget::Window(_)));
        if active && on_window && pose.is_none() {
            pose = Some((*gt, proj.cloned().unwrap_or_default()));
        }
        cameras.push((e, active));
    }
    let mouse_was_enabled = world.get_resource::<PointerInputSettings>().is_none_or(|s| s.is_mouse_enabled);
    if let Some(mut s) = world.get_resource_mut::<PointerInputSettings>() {
        s.is_mouse_enabled = false;
    }
    let camera = world.resource::<EditState>().editor_camera.filter(|e| world.get_entity(*e).is_ok());
    let camera = match camera {
        Some(e) => e,
        None => {
            let (gt, projection) = pose.unwrap_or((GlobalTransform::from(Transform::from_xyz(-6.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)), Projection::default()));
            let t = gt.compute_transform();
            let forward = t.forward().as_vec3();
            let (yaw, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
            // Every layer but the previews' and the thumbnails'.
            let layers = RenderLayers::from_layers(&(0..PREVIEW_LAYERS.start).chain([EDITOR_LAYER]).collect::<Vec<_>>());
            world
                .spawn((
                    Name::new("bevy-ts editor camera"),
                    Camera3d::default(),
                    Camera { order: -10, is_active: false, ..default() },
                    projection,
                    t,
                    layers,
                    EditorOnly,
                    EditorCamera { focus: t.translation + forward * 10.0, yaw, pitch },
                    TransformGizmoCamera,
                ))
                .id()
        }
    };
    let mut s = world.resource_mut::<EditState>();
    s.mode = Mode::Edit;
    s.was_paused = was_paused;
    s.mouse_was_enabled = mouse_was_enabled;
    s.game_cameras = cameras;
    s.editor_camera = Some(camera);
    apply_view(world);
}

/// Edit mode's cameras and tools for the view in force, at the next `First`: the editor view turns
/// the editor and overlay cameras on and the game cameras off; the game view the other way round,
/// each game camera to its remembered flag, with the gizmos and the gizmo's focus gone.
fn apply_view(world: &mut World) {
    let s = world.resource::<EditState>();
    if s.mode != Mode::Edit {
        return;
    }
    let editor = s.editor_view();
    for (e, on) in s.game_cameras.clone() {
        set_active(world, e, on && !editor);
    }
    let tools: Vec<Entity> = world.query_filtered::<Entity, (With<Camera>, With<EditorOnly>, Without<UiSink>)>().iter(world).collect();
    for e in tools {
        set_active(world, e, editor);
    }
    world.resource_mut::<GizmoConfigStore>().config_mut::<EditorGizmos>().0.enabled = editor;
    if !editor {
        let focused: Vec<Entity> = world.query_filtered::<Entity, With<TransformGizmoFocus>>().iter(world).collect();
        for e in focused {
            world.entity_mut(e).remove::<TransformGizmoFocus>();
        }
        world.resource_mut::<Selection>().hovered = None;
    }
}

fn enter_play(world: &mut World) {
    let (cameras, was_paused, mouse) = {
        let mut s = world.resource_mut::<EditState>();
        s.mode = Mode::Play;
        (std::mem::take(&mut s.game_cameras), s.was_paused, s.mouse_was_enabled)
    };
    for (e, active) in cameras {
        set_active(world, e, active);
    }
    let editor: Vec<Entity> = world.query_filtered::<Entity, (With<Camera>, With<EditorOnly>)>().iter(world).collect();
    for e in editor {
        set_active(world, e, false);
    }
    let focused: Vec<Entity> = world.query_filtered::<Entity, With<TransformGizmoFocus>>().iter(world).collect();
    for e in focused {
        world.entity_mut(e).remove::<TransformGizmoFocus>();
    }
    if let Some(mut s) = world.get_resource_mut::<PointerInputSettings>() {
        s.is_mouse_enabled = mouse;
    }
    world.resource_mut::<GizmoConfigStore>().config_mut::<EditorGizmos>().0.enabled = false;
    // Triggers raised in edit mode never reach the game.
    if let Some(mut q) = world.get_resource_mut::<bevy_ts::observers::ObserverQueue>() {
        q.0.clear();
    }
    if world.contains_resource::<bevy_ts_levels::engine::Levels>() {
        let set = bevy_ts_levels::engine::open_set(world);
        world.resource_mut::<EditState>().open_levels = Some(set);
    }
    // The first play starts the game: its `Startup` runs now, before the stages.
    bevy_ts::generation::start_game(world);
    if !was_paused {
        set_paused(world, false);
    }
}

/// Holds `IsDefaultUiCamera` in edit mode without `ui`: a camera that never renders, so game UI
/// draws nowhere. Without it Bevy gives UI to the highest-order window camera, which in edit mode
/// is the transform gizmo's overlay, active while something is selected.
#[derive(Component)]
struct UiSink;

/// Game UI over the editor view only with `ui: true`: the editor camera is then the default UI
/// camera; otherwise [`UiSink`] is, in the editor view, and the game's own default in the game
/// view and in play.
fn apply_ui(world: &mut World) {
    let s = world.resource::<EditState>();
    let (edit, ui, camera) = (s.editor_view(), s.ui, s.editor_camera);
    let sink = world.query_filtered::<Entity, With<UiSink>>().iter(world).next();
    let sink = match sink {
        Some(e) => e,
        None if edit && !ui => world.spawn((Name::new("bevy-ts ui sink"), Camera { order: -12, is_active: false, ..default() }, EditorOnly, UiSink)).id(),
        None => return apply_default_ui(world, camera, edit && ui),
    };
    let mut e = world.entity_mut(sink);
    if edit && !ui {
        e.insert(bevy::ui::IsDefaultUiCamera);
    } else {
        e.remove::<bevy::ui::IsDefaultUiCamera>();
    }
    apply_default_ui(world, camera, edit && ui);
}

fn apply_default_ui(world: &mut World, camera: Option<Entity>, on: bool) {
    let Some(Ok(mut e)) = camera.map(|c| world.get_entity_mut(c)) else { return };
    if on {
        e.insert(bevy::ui::IsDefaultUiCamera);
    } else {
        e.remove::<bevy::ui::IsDefaultUiCamera>();
    }
}

/// Per-frame pointer bookkeeping: where the left button went down, and whether it dragged.
#[derive(Resource, Default)]
pub(crate) struct PointerTrack {
    down_at: Option<Vec2>,
    dragged: bool,
    gizmo: bool,
}

pub(crate) const CLICK_SLOP: f32 = 4.0;

/// The ray under a window point, from the editor camera.
pub(crate) fn editor_ray(world: &mut World, at: Vec2) -> Option<Ray3d> {
    let camera = world.resource::<EditState>().editor_camera?;
    let (cam, gt) = world.query::<(&Camera, &GlobalTransform)>().get(world, camera).ok()?;
    cam.viewport_to_world(gt, at).ok()
}

/// A hit under the pointer: the entity, its distance, and what a click there selects.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub entity: Entity,
    pub depth: f32,
    pub selects: Entity,
}

/// Every pickable thing a ray meets, nearest first: meshes, then light and camera icons.
pub(crate) fn hits(world: &mut World, ray: Ray3d, exact: bool) -> Vec<Hit> {
    let mut state = bevy::ecs::system::SystemState::<(
        MeshRayCast,
        Query<(), Or<(With<EditorOnly>, With<PreviewOnly>, With<TransformGizmoMeshMarker>, With<TransformGizmoRoot>)>>,
    )>::new(world);
    let mut found: Vec<(Entity, f32)> = {
        let Ok((mut cast, editor)) = state.get_mut(world) else { return vec![] };
        let filter = |e: Entity| !editor.contains(e);
        let settings = MeshRayCastSettings::default().with_filter(&filter).never_early_exit().with_visibility(RayCastVisibility::Visible);
        cast.cast_ray(ray, &settings).iter().map(|(e, h)| (*e, h.distance)).collect()
    };
    // Icons: a sphere of radius ICON at lights and cameras with no mesh.
    let mut icons = world.query_filtered::<(Entity, &GlobalTransform), (Without<Mesh3d>, Without<EditorOnly>, Without<PreviewOnly>, Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>, With<Camera>)>)>();
    for (e, gt) in icons.iter(world) {
        let c = gt.translation() - ray.origin;
        let along = c.dot(*ray.direction);
        if along > 0.0 && (c - *ray.direction * along).length() <= ICON {
            found.push((e, along));
        }
    }
    found.sort_by(|a, b| a.1.total_cmp(&b.1));
    found.into_iter().map(|(entity, depth)| Hit { entity, depth, selects: if exact { entity } else { instance_root(world, entity) } }).collect()
}

const ICON: f32 = 0.35;

/// Where the ray meets the plane y = 0 in front of the camera, if it does.
pub fn ground(ray: Ray3d) -> Option<Vec3> {
    ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)).map(|t| ray.get_point(t))
}

/// The nearest ancestor-or-self that is a prefab instance root: the highest entity of the hit's
/// own instance (same innermost call site in its TSX source chain). An entity with no call site
/// selects itself.
pub fn instance_root(world: &World, e: Entity) -> Entity {
    let Some(node) = world.get::<TsxNode>(e) else { return e };
    let Some(site) = node.source.get(1).cloned() else { return e };
    let depth = node.source.len();
    let mut root = e;
    let mut at = e;
    while let Some(parent) = world.get::<ChildOf>(at).map(ChildOf::parent) {
        match world.get::<TsxNode>(parent) {
            Some(p) if p.source.len() == depth && p.source.get(1) == Some(&site) => root = parent,
            _ => break,
        }
        at = parent;
    }
    root
}

fn node_of(world: &World, e: Entity) -> Option<u64> {
    world.get::<TsxNode>(e).map(|n| n.id)
}

/// After a reload, a selected node whose entity changed is followed to its new entity.
fn keep_selection(mut sel: ResMut<Selection>, tree: Option<Res<TsxTree>>, alive: Query<Option<&TsxNode>>) {
    let Some(tree) = tree else { return };
    let mut changed = false;
    let items: Vec<_> = sel
        .items
        .iter()
        .filter_map(|&(e, node)| {
            let ok = alive.get(e).is_ok_and(|n| node.is_none() || n.map(|n| n.id) == node);
            if ok {
                return Some((e, node));
            }
            changed = true;
            node.and_then(|id| tree.entity(id)).map(|e| (e, node))
        })
        .collect();
    if changed {
        sel.items = items;
        sel.seq += 1;
    }
}

/// A Stop's [`keep_selection`]: each selected node followed to the entity that has it now, at
/// once; an item whose entity is gone and has no node, or whose node is gone, is dropped. A
/// change is streamed with `origin`.
fn follow_selection(world: &mut World, origin: Origin) {
    let tree = world.get_resource::<TsxTree>();
    let mut changed = false;
    let items: Vec<_> = world
        .resource::<Selection>()
        .items
        .iter()
        .filter_map(|&(e, node)| {
            let ok = world.get_entity(e).is_ok_and(|r| node.is_none() || r.get::<TsxNode>().map(|n| n.id) == node);
            if ok {
                return Some((e, node));
            }
            changed = true;
            node.and_then(|id| tree?.entity(id)).map(|e| (e, node))
        })
        .collect();
    if changed {
        world.resource_mut::<Selection>().set(items, origin);
    }
}

/// `W`, `E`, `R`, `X`, `F`, `Escape`.
fn edit_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut tools: ResMut<ViewportTools>,
    mut sel: ResMut<Selection>,
    gizmo: Res<TransformGizmoState>,
    mods: Option<Res<crate::input::Modifiers>>,
    mut commands: Commands,
) {
    // Keys while the right button is held fly the camera.
    if mouse.pressed(MouseButton::Right) {
        return;
    }
    // A shortcut with ⌃, ⌘ or ⌥ is not a tool key.
    let chord = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight, KeyCode::AltLeft, KeyCode::AltRight])
        || mods.is_some_and(|m| m.ctrl || m.meta || m.alt);
    for (k, mode) in [(KeyCode::KeyW, TransformGizmoMode::Translate), (KeyCode::KeyE, TransformGizmoMode::Rotate), (KeyCode::KeyR, TransformGizmoMode::Scale)] {
        if keys.just_pressed(k) && !chord {
            tools.set(Some(mode), None, None, Origin::Viewport);
        }
    }
    if keys.just_pressed(KeyCode::KeyX) && !chord {
        let space = match tools.space {
            TransformGizmoSpace::World => TransformGizmoSpace::Local,
            TransformGizmoSpace::Local => TransformGizmoSpace::World,
        };
        tools.set(None, Some(space), None, Origin::Viewport);
    }
    // Escape mid-drag cancels the drag (edits), not the selection.
    if keys.just_pressed(KeyCode::Escape) && !gizmo.active && !sel.items.is_empty() {
        sel.set(vec![], Origin::Viewport);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        commands.queue(|world: &mut World| frame_selection(world));
    }
}

/// The world-space bounds of an entity and its descendants.
pub fn bounds(world: &World, e: Entity) -> Option<(Vec3, Vec3)> {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    let mut any = false;
    let mut stack = vec![e];
    while let Some(at) = stack.pop() {
        if let (Some(aabb), Some(gt)) = (world.get::<Aabb>(at), world.get::<GlobalTransform>(at)) {
            let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
            for i in 0..8 {
                let corner = c + h * Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
                let p = gt.transform_point(corner);
                lo = lo.min(p);
                hi = hi.max(p);
            }
            any = true;
        }
        if let Some(children) = world.get::<Children>(at) {
            stack.extend(children.iter());
        }
    }
    if !any {
        let p = world.get::<GlobalTransform>(e)?.translation();
        return Some((p - Vec3::splat(ICON), p + Vec3::splat(ICON)));
    }
    Some((lo, hi))
}

/// Moves the editor camera so the selection's bounds fill the middle of the view, keeping its
/// direction.
pub fn frame_selection(world: &mut World) {
    let sel: Vec<Entity> = world.resource::<Selection>().items.iter().map(|(e, _)| *e).collect();
    let Some(camera) = world.resource::<EditState>().editor_camera else { return };
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for e in sel {
        if let Some((a, b)) = bounds(world, e) {
            lo = lo.min(a);
            hi = hi.max(b);
        }
    }
    if lo.x > hi.x {
        return;
    }
    let center = (lo + hi) / 2.0;
    let radius = ((hi - lo).length() / 2.0).max(0.1);
    let fov = match world.get::<Projection>(camera) {
        Some(Projection::Perspective(p)) => p.fov,
        _ => std::f32::consts::FRAC_PI_4,
    };
    let distance = radius / (fov / 2.0).sin() * 1.1;
    let Some(mut t) = world.get_mut::<Transform>(camera) else { return };
    let back = t.back().as_vec3();
    t.translation = center + back * distance;
    if let Some(mut c) = world.get_mut::<EditorCamera>(camera) {
        c.focus = center;
    }
}

/// Fly with the right button and `W A S D Q E`, orbit with the middle button or ⌥ and left,
/// dolly with the wheel or a pinch.
#[allow(clippy::too_many_arguments)]
pub(crate) fn camera_controller(
    state: Res<EditState>,
    mut cameras: Query<(&mut Transform, &mut EditorCamera)>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut pinch: MessageReader<PinchGesture>,
    time: Res<Time<Real>>,
    mut track: ResMut<PointerTrack>,
    mods: Option<Res<crate::input::Modifiers>>,
) {
    let Some(camera) = state.editor_camera else { return };
    let Ok((mut t, mut cam)) = cameras.get_mut(camera) else { return };
    let delta: Vec2 = motion.read().map(|m| m.delta).sum();
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight) || mods.is_some_and(|m| m.alt);
    let orbit = mouse.pressed(MouseButton::Middle) || (alt && mouse.pressed(MouseButton::Left) && track.dragged && !track.gizmo);
    const LOOK: f32 = 0.004;
    if mouse.pressed(MouseButton::Right) {
        cam.yaw -= delta.x * LOOK;
        cam.pitch = (cam.pitch - delta.y * LOOK).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        t.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
        let mut dir = Vec3::ZERO;
        for (k, d) in [
            (KeyCode::KeyW, t.forward().as_vec3()),
            (KeyCode::KeyS, t.back().as_vec3()),
            (KeyCode::KeyA, t.left().as_vec3()),
            (KeyCode::KeyD, t.right().as_vec3()),
            (KeyCode::KeyE, Vec3::Y),
            (KeyCode::KeyQ, Vec3::NEG_Y),
        ] {
            if keys.pressed(k) {
                dir += d;
            }
        }
        let speed = if keys.pressed(KeyCode::ShiftLeft) { 20.0 } else { 6.0 };
        let step = dir.normalize_or_zero() * speed * time.delta_secs();
        t.translation += step;
        let distance = (cam.focus - t.translation).length().max(0.5);
        cam.focus = t.translation + t.forward().as_vec3() * distance;
    } else if orbit {
        cam.yaw -= delta.x * LOOK;
        cam.pitch = (cam.pitch - delta.y * LOOK).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        let distance = (t.translation - cam.focus).length();
        t.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
        t.translation = cam.focus + t.back().as_vec3() * distance;
    }
    let mut dolly = 0.0;
    for w in wheel.read() {
        dolly += match w.unit {
            MouseScrollUnit::Line => w.y * 0.1,
            MouseScrollUnit::Pixel => w.y * 0.005,
        };
    }
    for p in pinch.read() {
        dolly += p.0;
    }
    if dolly != 0.0 {
        let distance = (t.translation - cam.focus).length();
        let next = (distance * (1.0 - dolly).clamp(0.2, 5.0)).max(0.1);
        let back = t.back().as_vec3();
        t.translation = cam.focus + back * next;
    }
    if mouse.just_pressed(MouseButton::Left) {
        track.dragged = false;
    }
}

/// ⌥ and ⇧ held: as keys, or as the modifiers on the viewer's last pointer event.
fn modifiers(world: &World) -> (bool, bool) {
    let k = world.resource::<ButtonInput<KeyCode>>();
    let m = world.get_resource::<crate::input::Modifiers>();
    (
        k.pressed(KeyCode::AltLeft) || k.pressed(KeyCode::AltRight) || m.is_some_and(|m| m.alt),
        k.pressed(KeyCode::ShiftLeft) || k.pressed(KeyCode::ShiftRight) || m.is_some_and(|m| m.shift),
    )
}

/// A left click with no drag picks; ⌥ picks the exact entity, ⇧ adds or removes, nothing hit
/// clears.
fn click_to_select(world: &mut World) {
    let (pressed, released, held) = {
        let m = world.resource::<ButtonInput<MouseButton>>();
        (m.just_pressed(MouseButton::Left), m.just_released(MouseButton::Left), m.pressed(MouseButton::Left))
    };
    let cursor = world.query_filtered::<&Window, With<PrimaryWindow>>().single(world).ok().and_then(Window::cursor_position);
    // A press on the orientation gizmo is its own, as one on a gizmo handle is.
    let orient = world.get_resource::<crate::orient::OrientRect>().is_some_and(|r| crate::orient::in_square(r, cursor));
    let gizmo = {
        let g = world.resource::<TransformGizmoState>();
        g.active || g.hovered_axis.is_some() || orient
    };
    let mut track = world.resource_mut::<PointerTrack>();
    if pressed {
        track.down_at = cursor;
        track.dragged = false;
        track.gizmo = gizmo;
    }
    if held && let (Some(a), Some(b)) = (track.down_at, cursor) && a.distance(b) > CLICK_SLOP {
        track.dragged = true;
    }
    if !released {
        // Hover outline.
        let hovered = cursor.filter(|_| !orient).and_then(|c| editor_ray(world, c)).and_then(|r| hits(world, r, false).first().map(|h| h.selects));
        world.resource_mut::<Selection>().hovered = hovered;
        return;
    }
    let (dragged, on_gizmo) = (track.dragged, track.gizmo);
    track.down_at = None;
    if dragged || on_gizmo {
        return;
    }
    let Some(at) = cursor else { return };
    let (alt, shift) = modifiers(world);
    let picked = editor_ray(world, at).and_then(|r| hits(world, r, alt).first().copied()).map(|h| h.selects);
    let mut items = world.resource::<Selection>().items.clone();
    match (picked, shift) {
        (None, false) => items.clear(),
        (None, true) => {}
        (Some(e), false) => items = vec![(e, node_of(world, e))],
        (Some(e), true) => {
            if let Some(i) = items.iter().position(|(x, _)| *x == e) {
                items.remove(i);
            } else {
                items.push((e, node_of(world, e)));
            }
        }
    }
    world.resource_mut::<Selection>().set(items, Origin::Viewport);
}

/// `TransformGizmoFocus` leaves what is no longer the primary selection; edits put it on the
/// primary unless its field is locked.
fn gizmo_focus(sel: Res<Selection>, focused: Query<Entity, With<TransformGizmoFocus>>, mut commands: Commands) {
    let primary = sel.primary();
    for e in &focused {
        if Some(e) != primary {
            commands.entity(e).remove::<TransformGizmoFocus>();
        }
    }
}

/// The grid, selection and hover outlines, and icons for lights and cameras with no mesh.
fn draw_gizmos(world: &mut World) {
    let sel = world.resource::<Selection>();
    let outlines: Vec<(Entity, bool)> = sel.items.iter().map(|(e, _)| (*e, true)).chain(sel.hovered.map(|e| (e, false))).collect();
    let boxes: Vec<((Vec3, Vec3), bool)> = outlines.iter().filter_map(|(e, s)| bounds(world, *e).map(|b| (b, *s))).collect();
    let icons: Vec<(Vec3, bool)> = world
        .query_filtered::<(&GlobalTransform, Has<Camera>), (Without<Mesh3d>, Without<EditorOnly>, Without<PreviewOnly>, Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>, With<Camera>)>)>()
        .iter(world)
        .map(|(gt, cam)| (gt.translation(), cam))
        .collect();
    let mut state = bevy::ecs::system::SystemState::<Gizmos<EditorGizmos>>::new(world);
    let Ok(mut g) = state.get_mut(world) else { return };
    g.grid(Isometry3d::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), UVec2::splat(40), Vec2::ONE, Color::srgba(1.0, 1.0, 1.0, 0.12));
    for ((lo, hi), selected) in boxes {
        let color = if selected { Color::srgb(1.0, 0.6, 0.1) } else { Color::srgba(1.0, 1.0, 1.0, 0.5) };
        g.cube(Transform::from_translation((lo + hi) / 2.0).with_scale((hi - lo).max(Vec3::splat(0.01))), color);
    }
    for (p, camera) in icons {
        let color = if camera { Color::srgb(0.6, 0.8, 1.0) } else { Color::srgb(1.0, 0.9, 0.4) };
        g.sphere(Isometry3d::from_translation(p), ICON, color);
    }
    state.apply(world);
}
