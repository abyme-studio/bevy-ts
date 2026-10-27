//! Pixel-art 2D (openspec change `pixel-art-2d`). [`YSort`] draws an entity by its base y within a
//! layer and owns `Transform` z; [`ScreenSize`] keeps it one size on screen at every zoom and owns
//! `Transform` scale; [`PixelCamera`] zooms a `Camera2d` by whole numbers and owns its projection's
//! scale. The pass in [`Pixel2dSystems`] writes them before Bevy's transform propagation, so
//! `GlobalTransform` stays Bevy's own and every reader of it (picking, visibility, extraction) sees
//! one value. Nothing here writes `GlobalTransform`. After propagation, one system snaps the pixel
//! camera's view to whole screen pixels through its projection's `viewport_origin`. [`SpriteClip`]
//! steps a sprite's atlas index in `Update`, as Bevy's `sprite_animation` example does.

use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use std::time::Duration;
use bevy_ts::log::{LogLevel, ScriptLog};
use std::collections::{HashMap, HashSet};

/// The largest `YSort.layer`, either side of 0: under 128 an f32's step is finer than one pixel's
/// share of z, so a one-pixel move of y always changes z.
pub const MAX_LAYER: f32 = 127.0;
/// The base y past which y-sorted sprites tie, either side of 0: the y term stays inside ±0.5, so
/// layers a whole unit apart never mix.
pub const MAX_BASE: f32 = 49_999.0;
/// z per pixel of base y.
pub const Z_PER_PX: f64 = 1e-5;

/// Draws the entity by its base y within `layer`: lower on screen is in front. Its `Transform` z is
/// `layer - clamp(y + offset, ±49,999) × 10⁻⁵`, `y` its own `Transform` y (in its parent's space
/// when it has one). `offset` moves the base from the origin to the feet. It owns `Transform` z: a
/// write to z is overwritten the same frame; write `layer` instead.
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
#[require(Transform)]
pub struct YSort {
    /// The whole layer the entity draws in, from -127 to 127.
    pub layer: f32,
    /// Added to y for the base, in pixels: `-8` puts it 8 px below the origin.
    pub offset: f32,
}

/// The z a y-sorted entity at `y` draws at, for a `layer` within ±[`MAX_LAYER`].
pub fn y_sort_z(layer: f32, y: f32, offset: f32) -> f32 {
    let base = (y + offset).clamp(-MAX_BASE, MAX_BASE);
    (layer as f64 - base as f64 * Z_PER_PX) as f32
}

/// Keeps the entity one size on screen at every zoom: its `Transform` scale x and y are `scale`
/// times the ruling camera's orthographic scale (the [`PixelCamera`]'s, else the first `Camera2d`
/// by `Camera.order`, then by entity), so one image pixel is `scale` screen pixels. Its descendants scale with it. It owns
/// `Transform` scale x and y: a write to them is overwritten the same frame; write `scale`.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
#[require(Transform)]
pub struct ScreenSize {
    /// Screen pixels per image pixel.
    pub scale: f32,
}

impl Default for ScreenSize {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

/// Zooms its `Camera2d` by whole numbers: the orthographic projection's `scale` is `1 / zoom`, so a
/// texel covers `zoom` screen pixels. A change of `zoom` moves the scale there over `ease` seconds
/// of real time with a cubic ease-out (`ease` 0 jumps), so a paused game still zooms. It owns the
/// projection's `scale` and turns its `Msaa` off: a write to either is overwritten. Its view is drawn from the camera's position
/// rounded to whole screen pixels; the camera's `Transform` keeps the script's value.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct PixelCamera {
    /// Screen pixels per texel: a whole number from 1.
    pub zoom: f32,
    /// Seconds a change of `zoom` takes.
    pub ease: f32,
}

impl Default for PixelCamera {
    fn default() -> Self {
        Self { zoom: 1.0, ease: 0.25 }
    }
}

/// A pixel camera's ease and snap, kept by this module: not reflected.
#[derive(Component, Clone, Copy, Debug)]
pub struct PixelCameraState {
    from: f32,
    to: f32,
    since: Duration,
    /// The script's `viewport_origin`, and the snapped one last written over it.
    origin: Vec2,
    written: Option<Vec2>,
}

/// The cubic ease-out from 0 to 1.
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// The pass that writes what [`YSort`], [`ScreenSize`] and [`PixelCamera`] own, in `PostUpdate` after the TSX reconcile and before
/// `TransformSystems::Propagate`. It writes a field only when its value changes, so a still entity
/// is not propagated again.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pixel2dSystems;

/// [`YSort`] and its pass. The engine adds it; a Rust app adds it beside `DefaultPlugins`.
pub struct Pixel2dPlugin;

impl Plugin for Pixel2dPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<YSort>()
            .register_type::<ScreenSize>()
            .register_type::<PixelCamera>()
            .register_type::<SpriteClip>()
            .register_type::<ClipMode>()
            .add_systems(Update, sprite_clips.run_if(any_with_component::<SpriteClip>))
            .init_resource::<RulingPixelCamera>()
            .configure_sets(PostUpdate, Pixel2dSystems.before(TransformSystems::Propagate))
            .add_systems(
                PostUpdate,
                (
                    pixel_camera.run_if(any_with_component::<PixelCamera>),
                    screen_size.run_if(any_with_component::<ScreenSize>),
                    y_sort.run_if(any_with_component::<YSort>),
                )
                    .chain()
                    .in_set(Pixel2dSystems),
            )
            .add_systems(PostUpdate, snap_pixel_camera.after(TransformSystems::Propagate).before(CameraUpdateSystems).run_if(any_with_component::<PixelCamera>))
            .add_systems(PostUpdate, requeue_new_z.after(TransformSystems::Propagate).run_if(any_with_component::<YSort>));
    }
}

/// What the pass has said, so each problem is logged once per entity and value.
#[derive(Default)]
struct Told {
    /// An entity's out-of-range `layer`, by its bits.
    layer: HashMap<Entity, u32>,
    /// Entities whose base went past the clamp.
    clamped: HashSet<Entity>,
}

fn log(log: &mut Option<ResMut<ScriptLog>>, generations: &Option<Res<bevy_ts::generation::Generations>>, level: LogLevel, message: String) {
    match log {
        Some(l) => l.push(level, message, None, generations.as_ref().map_or(0, |g| g.live)),
        None => warn!("{message}"),
    }
}

fn named(e: Entity, name: Option<&Name>) -> String {
    match name {
        Some(n) => format!("entity {e} \"{n}\""),
        None => format!("entity {e}"),
    }
}

fn y_sort(
    mut sorted: Query<(Entity, &YSort, &mut Transform, Option<&Name>)>,
    mut script_log: Option<ResMut<ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut told: Local<Told>,
) {
    for (e, y, mut t, name) in &mut sorted {
        if !(-MAX_LAYER..=MAX_LAYER).contains(&y.layer) {
            if told.layer.insert(e, y.layer.to_bits()) != Some(y.layer.to_bits()) {
                let message = format!("{}: YSort.layer {} is out of range: a layer is from -127 to 127", named(e, name), y.layer);
                log(&mut script_log, &generations, LogLevel::Error, message);
            }
            continue;
        }
        told.layer.remove(&e);
        let base = t.translation.y + y.offset;
        if base.abs() > MAX_BASE && told.clamped.insert(e) {
            let message = format!("{}: YSort base y {base} is past ±49999, where y-sorted sprites tie and draw in no set order", named(e, name));
            log(&mut script_log, &generations, LogLevel::Warn, message);
        }
        let z = y_sort_z(y.layer, t.translation.y, y.offset);
        if t.translation.z != z {
            t.translation.z = z;
        }
    }
}

/// Bevy 0.20 sorts a transparent 2D mesh (a sprite is one) by the z it had when it was queued, and
/// queues it again only when its mesh or material changes
/// (`bevy_sprite_render-0.20.0/src/mesh2d/material.rs:718-729, 1191`), so a new z alone keeps the
/// old draw order. This marks `Mesh2d` changed on each 2D mesh whose global z moved, so it is queued
/// again with the new z: a y-sorted entity and its children, as they walk. It runs while a `YSort`
/// exists; Bevy's check sees the change that frame or the next.
pub fn requeue_new_z(
    mut moved: Query<(Entity, &GlobalTransform, &mut Mesh2d), Changed<GlobalTransform>>,
    mut queued: Local<bevy::ecs::entity::EntityHashMap<f32>>,
    mut gone: RemovedComponents<Mesh2d>,
) {
    for e in gone.read() {
        queued.remove(&e);
    }
    for (e, at, mut mesh) in &mut moved {
        let z = at.translation().z;
        if queued.insert(e, z).is_some_and(|was| was != z) {
            mesh.set_changed();
        }
    }
}

fn ortho_scale(p: &Projection) -> Option<f32> {
    match p {
        Projection::Orthographic(o) => Some(o.scale),
        _ => None,
    }
}

fn screen_size(cameras: Query<(Entity, &Camera, &Projection), With<Camera2d>>, ruling: Res<RulingPixelCamera>, mut sized: Query<(&ScreenSize, &mut Transform)>) {
    // The ruling pixel camera, else the first Camera2d by order, then by entity.
    let pixel = ruling.0.and_then(|e| cameras.get(e).ok()).and_then(|(_, _, p)| ortho_scale(p));
    let first = || cameras.iter().filter_map(|(e, c, p)| Some(((c.order, e), ortho_scale(p)?))).min_by_key(|(k, _)| *k).map(|(_, s)| s);
    let Some(camera) = pixel.or_else(first) else { return };
    for (s, mut t) in &mut sized {
        let scale = s.scale * camera;
        if t.scale.x != scale || t.scale.y != scale {
            t.scale.x = scale;
            t.scale.y = scale;
        }
    }
}

/// The `PixelCamera` that rules, the first added; the others are ignored, with one warning.
#[derive(Resource, Default)]
pub struct RulingPixelCamera(pub Option<Entity>);

/// The scale a pixel camera's projection has now, from its state at real time `now`.
fn eased(state: &PixelCameraState, ease: f32, now: Duration) -> f32 {
    if ease <= 0.0 {
        return state.to;
    }
    let t = (now.saturating_sub(state.since)).as_secs_f32() / ease;
    if t >= 1.0 { state.to } else { state.from + (state.to - state.from) * ease_out(t) }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pixel_camera(
    mut commands: Commands,
    mut cameras: Query<(Entity, &PixelCamera, &mut Projection, Option<&mut PixelCameraState>, Option<&mut Msaa>, Option<&Name>)>,
    added: Query<Entity, Added<PixelCamera>>,
    mut ruling: ResMut<RulingPixelCamera>,
    time: Res<Time<Real>>,
    mut script_log: Option<ResMut<ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut told: Local<(Option<(Entity, Entity)>, HashMap<Entity, u32>)>,
) {
    if ruling.0.is_none_or(|e| cameras.get(e).is_err()) {
        ruling.0 = added.iter().min().or_else(|| cameras.iter().map(|c| c.0).min());
    }
    let Some(ruler) = ruling.0 else { return };
    if let Some(other) = cameras.iter().map(|c| c.0).find(|e| *e != ruler)
        && told.0 != Some((ruler, other))
    {
        told.0 = Some((ruler, other));
        let message = format!("two PixelCameras, entity {ruler} and entity {other}: entity {ruler}, the first added, rules; the other zooms nothing");
        log(&mut script_log, &generations, LogLevel::Warn, message);
    }
    let Ok((e, pixel, mut projection, state, msaa, name)) = cameras.get_mut(ruler) else { return };
    // Multisampling would blend a sub-pixel sprite's edge pixels: a pixel is filled when its centre
    // is inside the quad, or not at all.
    match msaa {
        Some(mut m) if *m != Msaa::Off => *m = Msaa::Off,
        Some(_) => {}
        None => {
            commands.entity(e).insert(Msaa::Off);
        }
    }
    let Projection::Orthographic(ortho) = &mut *projection else { return };
    if !(pixel.zoom >= 1.0 && pixel.zoom.fract() == 0.0) {
        if told.1.insert(e, pixel.zoom.to_bits()) != Some(pixel.zoom.to_bits()) {
            let message = format!("{}: PixelCamera.zoom {} is not a whole number from 1", named(e, name), pixel.zoom);
            log(&mut script_log, &generations, LogLevel::Error, message);
        }
        return;
    }
    told.1.remove(&e);
    let (now, to) = (time.elapsed(), 1.0 / pixel.zoom);
    let scale = match state {
        None => {
            commands.entity(e).insert(PixelCameraState { from: to, to, since: now, origin: ortho.viewport_origin, written: None });
            to
        }
        Some(mut state) => {
            if state.to != to {
                state.from = eased(&state, pixel.ease, now);
                state.to = to;
                state.since = now;
            }
            eased(&state, pixel.ease, now)
        }
    };
    if ortho.scale != scale {
        ortho.scale = scale;
    }
}

/// The ruling pixel camera's view drawn from its position rounded to whole screen pixels: its
/// step is its scale in world units, with half a step more on an axis whose size in pixels is odd,
/// so texel edges at whole world units land on pixel edges. Shifts `viewport_origin` by the
/// difference, over the projection's size; writes neither `Transform` nor `GlobalTransform`.
pub fn snap_pixel_camera(ruling: Res<RulingPixelCamera>, mut cameras: Query<(&Camera, &GlobalTransform, &mut Projection, &mut PixelCameraState), With<PixelCamera>>) {
    let Some((camera, at, mut projection, mut state)) = ruling.0.and_then(|e| cameras.get_mut(e).ok()) else { return };
    let Some(size) = camera.logical_viewport_size() else { return };
    let Projection::Orthographic(ortho) = &*projection else { return };
    // A script's own viewport_origin is the base the snap shifts.
    if state.written != Some(ortho.viewport_origin) {
        state.origin = ortho.viewport_origin;
    }
    let step = ortho.scale;
    let snap = |x: f32, pixels: f32| {
        let half = if (pixels.round() as i64) % 2 == 1 { 0.5 } else { 0.0 };
        ((x / step - half).round() + half) * step
    };
    let p = at.translation().truncate();
    let snapped = Vec2::new(snap(p.x, size.x), snap(p.y, size.y));
    // Bevy draws x from `-viewport_origin.x × width × scale` about the camera: a larger origin
    // moves the view left, so drawing from `snapped` adds `(p - snapped)` over the view's size.
    let origin = state.origin + (p - snapped) / (step * size);
    state.written = Some(origin);
    if ortho.viewport_origin != origin
        && let Projection::Orthographic(o) = &mut *projection
    {
        o.viewport_origin = origin;
    }
}

/// Where a pixel camera's view is drawn from: its `GlobalTransform` x and y with its projection's
/// shift of `viewport_origin` taken out. For tests and tools.
pub fn drawn_from(camera: &Camera, at: &GlobalTransform, projection: &Projection, state: &PixelCameraState) -> Option<Vec2> {
    let size = camera.logical_viewport_size()?;
    let Projection::Orthographic(o) = projection else { return None };
    Some(at.translation().truncate() - (o.viewport_origin - state.origin) * o.scale * size)
}

/// How a [`SpriteClip`] runs past its last frame.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[reflect(Default)]
pub enum ClipMode {
    /// From `first` to `last`, then `first` again.
    #[default]
    Repeat,
    /// From `first` to `last`, held there, and `finished` set.
    Once,
    /// From `first` to `last` and back, again and again.
    PingPong,
}

/// Steps its sprite's `texture_atlas.index` from `first` to `last` at `fps` frames a second of
/// virtual time, as Bevy's `sprite_animation` example's timer does: a paused game holds the frame.
/// A change of `first`, `last` or `mode` starts again at `first`.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct SpriteClip {
    pub first: u32,
    pub last: u32,
    pub fps: f32,
    pub mode: ClipMode,
    /// `false` holds the frame.
    pub playing: bool,
    /// Set by the clip when a `Once` clip has shown `last` its full time.
    pub finished: bool,
}

impl Default for SpriteClip {
    fn default() -> Self {
        Self { first: 0, last: 0, fps: 10.0, mode: ClipMode::Repeat, playing: true, finished: false }
    }
}

/// A clip's clock, kept by this module: not reflected.
#[derive(Component, Clone, Copy, Debug)]
pub struct SpriteClipState {
    elapsed: Duration,
    key: (u32, u32, ClipMode),
}

/// The atlas index `clip` shows `elapsed` in, and whether a `Once` clip is done.
pub fn clip_frame(clip: &SpriteClip, elapsed: Duration) -> (u32, bool) {
    let n = (clip.last - clip.first + 1) as u64;
    let frame = (elapsed.as_secs_f64() * clip.fps as f64).floor() as u64;
    let offset = match clip.mode {
        ClipMode::Repeat => frame % n,
        ClipMode::Once => frame.min(n - 1),
        ClipMode::PingPong if n == 1 => 0,
        ClipMode::PingPong => {
            let f = frame % (2 * (n - 1));
            if f < n { f } else { 2 * (n - 1) - f }
        }
    };
    (clip.first + offset as u32, clip.mode == ClipMode::Once && frame >= n)
}

/// What is wrong with `clip` on `sprite`, if anything; `tiles` is its layout's count, once loaded.
fn clip_error(clip: &SpriteClip, sprite: &Sprite, tiles: Option<usize>) -> Option<String> {
    if clip.last < clip.first {
        return Some(format!("SpriteClip.last {} is below SpriteClip.first {}", clip.last, clip.first));
    }
    if clip.fps.is_nan() || clip.fps <= 0.0 {
        return Some(format!("SpriteClip.fps {} is not above 0", clip.fps));
    }
    if sprite.texture_atlas.is_none() {
        return Some("SpriteClip on a Sprite with no texture_atlas: give it one".into());
    }
    match tiles {
        Some(n) if clip.last as usize >= n => Some(format!("SpriteClip.last {} is past the layout's {n} tiles", clip.last)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sprite_clips(
    mut commands: Commands,
    time: Res<Time>,
    layouts: Option<Res<Assets<TextureAtlasLayout>>>,
    mut clips: Query<(Entity, &mut SpriteClip, &mut Sprite, Option<&mut SpriteClipState>, Option<&Name>)>,
    mut script_log: Option<ResMut<ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut told: Local<HashMap<Entity, String>>,
) {
    for (e, mut clip, mut sprite, state, name) in &mut clips {
        let tiles = sprite.texture_atlas.as_ref().and_then(|a| layouts.as_ref()?.get(&a.layout)).map(|l| l.len());
        if let Some(problem) = clip_error(&clip, &sprite, tiles) {
            if told.get(&e) != Some(&problem) {
                log(&mut script_log, &generations, LogLevel::Error, format!("{}: {problem}", named(e, name)));
                told.insert(e, problem);
            }
            continue;
        }
        told.remove(&e);
        let key = (clip.first, clip.last, clip.mode);
        let elapsed = match state {
            Some(mut s) if s.key == key => {
                if clip.playing {
                    s.elapsed += time.delta();
                }
                s.elapsed
            }
            Some(mut s) => {
                *s = SpriteClipState { elapsed: Duration::ZERO, key };
                Duration::ZERO
            }
            None => {
                commands.entity(e).insert(SpriteClipState { elapsed: Duration::ZERO, key });
                Duration::ZERO
            }
        };
        let (index, finished) = clip_frame(&clip, elapsed);
        if clip.finished != finished {
            clip.finished = finished;
        }
        if let Some(atlas) = sprite.texture_atlas.as_ref()
            && atlas.index != index as usize
        {
            sprite.texture_atlas.as_mut().unwrap().index = index as usize;
        }
    }
}
