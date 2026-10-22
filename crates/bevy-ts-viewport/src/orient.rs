//! The orientation gizmo: in edit mode, the world's axes as the editor camera sees them, in the
//! game viewport's top-left corner, as Bevy's editor prototype draws them. A click on an axis end
//! turns the editor camera to look down that axis. Not in play, previews or the engine's own window.
//!
//! As in the prototype, a small camera renders the axes into a texture, and the editor view shows
//! it: here on a quad just in front of the editor camera, over the corner. The small camera renders
//! only on the two frames after what it shows changed (the editor camera turned), so a
//! still view costs one textured quad. It is orthographic, on a layer no light shares (so no shadow
//! pass), and draws the axes flat, projected here through the editor camera's rotation; the hit
//! test uses the same projection.

use crate::edit::{CLICK_SLOP, EDITOR_LAYER, EditState, EditorCamera, EditorOnly, PITCH_LIMIT};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gizmos::config::{GizmoConfigGroup, GizmoConfigStore};
use bevy::gizmos::transform_gizmo::{TransformGizmoState, TransformGizmoSystems};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::window::PrimaryWindow;
use std::f32::consts::{FRAC_PI_2, PI};

/// The render layer the orientation gizmo draws on; only its camera sees it. 15 is Bevy's gizmo
/// overlay, 20-29 previews, 30 thumbnails, 31 the editor.
pub const ORIENT_LAYER: usize = 32;
/// The square's side and inset from the top and left edges, in logical px.
pub const SIZE: f32 = 96.0;
pub const INSET: f32 = 8.0;
/// Logical px from the square's middle to an axis end, an end's radius, and how near a click must be.
const REACH: f32 = 34.0;
const END: f32 = 8.0;
const HIT: f32 = 12.0;
/// The texture's scale: the square at 3x, shown smaller on a window of a lower scale.
const TEXTURE_SCALE: f32 = 3.0;

/// The orientation gizmo's camera, rendering into [`OrientImage`].
#[derive(Component)]
pub struct OrientCamera;

/// The quad that shows the gizmo in the editor view, a child of the editor camera.
#[derive(Component)]
pub struct OrientQuad;

/// A filled `+` axis end.
#[derive(Component)]
pub struct OrientEnd(pub Vec3);

/// The orientation gizmo's lines and hollow ends, on [`ORIENT_LAYER`].
#[derive(Default, Reflect, GizmoConfigGroup)]
struct OrientGizmos;

/// The square in the window, logical px, while the gizmo is drawn.
#[derive(Resource, Default, PartialEq)]
pub struct OrientRect(pub Option<Rect>);

/// The texture the gizmo renders into, one for the process.
#[derive(Resource)]
pub struct OrientImage(pub Handle<Image>);

/// What the texture last showed: the editor camera's rotation and projection; `None` when it is
/// to be drawn afresh. Then frames still to draw: two after any change, since gizmo lines reach
/// the render a frame after they are drawn. Then when the first draw began: Bevy compiles the
/// small camera's pipelines in the background, so it draws every frame for [`WARM_UP`] once a
/// process, until they are surely ready.
#[derive(Resource, Default)]
struct Drawn(Option<(Quat, Mat4)>, u8, Option<std::time::Instant>);

const WARM_UP: std::time::Duration = std::time::Duration::from_millis(1500);

const AXES: [(Vec3, Color); 3] = [
    (Vec3::X, Color::srgb(0.92, 0.26, 0.32)),
    (Vec3::Y, Color::srgb(0.45, 0.82, 0.25)),
    (Vec3::Z, Color::srgb(0.25, 0.52, 0.96)),
];

pub(crate) fn build(app: &mut App) {
    // `BEVY_TS_VIEWPORT_ORIENT=0` leaves it out, for measuring its cost.
    if std::env::var("BEVY_TS_VIEWPORT_ORIENT").is_ok_and(|v| v == "0") {
        return;
    }
    app.init_resource::<OrientRect>().init_resource::<Drawn>().init_gizmo_group::<OrientGizmos>();
    {
        let mut store = app.world_mut().resource_mut::<GizmoConfigStore>();
        let (config, _) = store.config_mut::<OrientGizmos>();
        config.render_layers = RenderLayers::layer(ORIENT_LAYER);
        config.line.width = 2.0;
    }
    let edit = |s: Res<EditState>| s.editor_view();
    app.add_systems(Startup, spawn)
        .add_systems(Update, (click, follow).chain().after(crate::edit::camera_controller).run_if(edit))
        .add_systems(Update, hide.run_if(|s: Res<EditState>| !s.editor_view()))
        .add_systems(PostUpdate, claim.before(TransformGizmoSystems).run_if(edit));
}

fn spawn(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let layer = RenderLayers::layer(ORIENT_LAYER);
    // One texture for the process, at the largest scale it serves: a new one is taken up as a
    // target some frames late (more under load), and one resized in place not at all.
    let px = (SIZE * TEXTURE_SCALE) as u32;
    let image = images.add(Image::new_target_texture(px, px, TextureFormat::Rgba8UnormSrgb, None));
    // Its world is the square in logical px, y up, seen from +Z.
    let projection = OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: SIZE, height: SIZE }, near: 0.0, far: 100.0, ..OrthographicProjection::default_3d() };
    commands.spawn((
        Name::new("bevy-ts orientation gizmo camera"),
        Camera3d::default(),
        Projection::Orthographic(projection),
        Transform::from_xyz(0.0, 0.0, 50.0),
        // Before the editor camera (-10), which shows the texture.
        Camera { order: -11, is_active: false, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        RenderTarget::Image(ImageRenderTarget { handle: image.clone(), scale_factor: TEXTURE_SCALE }),
        Tonemapping::None,
        bevy::core_pipeline::tonemapping::DebandDither::Disabled,
        layer.clone(),
        EditorOnly,
        OrientCamera,
    ));
    let disc = meshes.add(Circle::new(END));
    for (axis, color) in AXES {
        commands.spawn((OrientEnd(axis), Mesh3d(disc.clone()), MeshMaterial3d(materials.add(StandardMaterial { base_color: color, unlit: true, ..default() })), Transform::default(), Visibility::Hidden, layer.clone(), EditorOnly));
    }
    let material = materials.add(StandardMaterial { base_color_texture: Some(image.clone()), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
    commands.spawn((
        Name::new("bevy-ts orientation gizmo"),
        Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::Hidden,
        RenderLayers::layer(EDITOR_LAYER),
        EditorOnly,
        OrientQuad,
    ));
    commands.insert_resource(OrientImage(image));
}

/// Where a world direction lands in the square, from its middle (logical px, y down), and how
/// near the viewer it is.
fn project(view: Quat, dir: Vec3) -> (Vec2, f32) {
    let v = view.inverse() * dir;
    (Vec2::new(v.x, -v.y) * REACH, v.z)
}

/// Where an axis end is drawn in the window, logical px, for a view and square.
pub fn end_position(view: Quat, rect: Rect, dir: Vec3) -> Vec2 {
    rect.center() + project(view, dir).0
}

/// The square for a window, if it fits.
fn square(window: &Window) -> Option<Rect> {
    let (w, h) = (window.resolution.width(), window.resolution.height());
    (w >= SIZE + 2.0 * INSET && h >= SIZE + 2.0 * INSET).then(|| Rect::new(INSET, INSET, INSET + SIZE, INSET + SIZE))
}

/// The quad's place in the editor camera's space: over `rect` of a `size` window, just past the
/// near plane.
fn quad_transform(view_from_clip: Mat4, perspective: bool, size: Vec2, rect: Rect) -> Transform {
    let at = |p: Vec2| {
        let ndc = Vec2::new(p.x / size.x * 2.0 - 1.0, 1.0 - p.y / size.y * 2.0);
        let v = view_from_clip.project_point3(ndc.extend(1.0));
        if perspective { v * 1.05 } else { v - Vec3::Z * 0.05 }
    };
    let (a, b) = (at(rect.min), at(rect.max));
    Transform::from_translation((a + b) / 2.0).with_scale(Vec3::new((b.x - a.x).abs(), (b.y - a.y).abs(), 1.0))
}

/// The editor camera, apart from the gizmo's own entities.
type EditorView = (With<EditorCamera>, Without<OrientEnd>, Without<OrientQuad>);

/// Each frame in edit mode: the quad over the square; and, for two frames after the view changed
/// (or through the warm-up), the ends turned as the editor camera sees them, the lines and hollow
/// ends drawn, and the camera on.
#[allow(clippy::too_many_arguments)]
fn follow(
    mut commands: Commands,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    state: Res<EditState>,
    editor: Query<(&Transform, &Camera, &Projection), EditorView>,
    mut camera: Query<&mut Camera, (With<OrientCamera>, Without<EditorCamera>)>,
    mut ends: Query<(&OrientEnd, &mut Transform, &mut Visibility), Without<OrientQuad>>,
    mut quad: Query<(Entity, &mut Transform, &mut Visibility, Option<&ChildOf>), With<OrientQuad>>,
    mut rect: ResMut<OrientRect>,
    mut drawn: ResMut<Drawn>,
    mut g: Gizmos<OrientGizmos>,
) {
    let (Ok(mut cam), Ok((q, mut qt, mut qv, parent))) = (camera.single_mut(), quad.single_mut()) else { return };
    let editor_cam = state.editor_camera.and_then(|e| editor.get(e).ok().map(|x| (e, x)));
    let at = window.as_deref().and_then(|w| square(w).map(|r| (r, w.resolution.size())));
    let (Some((e, (et, ec, proj))), Some((r, size))) = (editor_cam, at) else {
        off(&mut rect, &mut drawn, &mut cam, &mut qv, ends.iter_mut().map(|(_, _, v)| v));
        return;
    };
    rect.set_if_neq(OrientRect(Some(r)));
    if parent.is_none_or(|p| p.parent() != e) {
        commands.entity(q).insert(ChildOf(e));
    }
    let clip = ec.clip_from_view();
    if clip.determinant() == 0.0 {
        // An editor camera not yet computed: draw once it is.
        if cam.is_active {
            cam.is_active = false;
        }
        return;
    }
    let t = quad_transform(clip.inverse(), matches!(proj, Projection::Perspective(_)), size, r);
    if *qt != t {
        *qt = t;
    }
    qv.set_if_neq(Visibility::Inherited);
    for (_, _, mut v) in &mut ends {
        v.set_if_neq(Visibility::Inherited);
    }
    let now = (et.rotation, clip);
    if drawn.0 != Some(now) {
        drawn.0 = Some(now);
        drawn.1 = 2;
    }
    let warming = drawn.2.get_or_insert_with(std::time::Instant::now).elapsed() < WARM_UP;
    if drawn.1 == 0 && !warming {
        if cam.is_active {
            cam.is_active = false;
        }
        return;
    }
    if !cam.is_active {
        cam.is_active = true;
    }
    drawn.1 = drawn.1.saturating_sub(1);
    // The nearer end on top.
    let place = |dir: Vec3| {
        let (p, z) = project(et.rotation, dir);
        Vec3::new(p.x, -p.y, z * 10.0)
    };
    for (end, mut t, _) in &mut ends {
        t.translation = place(end.0);
    }
    for (axis, color) in AXES {
        let tip = place(axis);
        g.line(Vec3::new(0.0, 0.0, tip.z.min(0.0) - 11.0), tip.truncate().extend(tip.z - 1.0), color);
        let back = place(-axis);
        g.circle(Isometry3d::from_translation(back), END, color);
    }
}

/// Out of the editor view: no square, no camera, no quad, no ends.
fn hide(
    mut rect: ResMut<OrientRect>,
    mut drawn: ResMut<Drawn>,
    mut camera: Query<&mut Camera, With<OrientCamera>>,
    mut quad: Query<&mut Visibility, With<OrientQuad>>,
    mut ends: Query<&mut Visibility, (With<OrientEnd>, Without<OrientQuad>)>,
) {
    if let (Ok(mut cam), Ok(mut v)) = (camera.single_mut(), quad.single_mut()) {
        off(&mut rect, &mut drawn, &mut cam, &mut v, ends.iter_mut());
    }
}

fn off<'a>(rect: &mut ResMut<OrientRect>, drawn: &mut ResMut<Drawn>, cam: &mut Mut<Camera>, quad: &mut Mut<Visibility>, ends: impl Iterator<Item = Mut<'a, Visibility>>) {
    rect.set_if_neq(OrientRect(None));
    if drawn.0.is_some() {
        drawn.0 = None;
    }
    if cam.is_active {
        cam.is_active = false;
    }
    quad.set_if_neq(Visibility::Hidden);
    for mut v in ends {
        v.set_if_neq(Visibility::Hidden);
    }
}

/// The axis end under a window point, if any: the nearest within reach, the one in front when two
/// overlap.
pub fn end_at(view: Quat, rect: Rect, at: Vec2) -> Option<Vec3> {
    let mut best: Option<(f32, f32, Vec3)> = None;
    for (axis, _) in AXES {
        for dir in [axis, -axis] {
            let z = project(view, dir).1;
            let d = end_position(view, rect, dir).distance(at);
            if d > HIT {
                continue;
            }
            let better = match best {
                None => true,
                Some((bd, bz, _)) => d + 1.0 < bd || (d < bd + 1.0 && z > bz),
            };
            if better {
                best = Some((d, z, dir));
            }
        }
    }
    best.map(|(_, _, dir)| dir)
}

/// The editor camera's yaw and pitch to look down `dir` toward the origin; looking along Y keeps
/// the yaw and stops at the pitch limit orbiting has, so the next orbit does not jump.
fn look_down(dir: Vec3, yaw: f32) -> (f32, f32) {
    match dir {
        d if d == Vec3::Y => (yaw, -PITCH_LIMIT),
        d if d == Vec3::NEG_Y => (yaw, PITCH_LIMIT),
        d if d == Vec3::X => (FRAC_PI_2, 0.0),
        d if d == Vec3::NEG_X => (-FRAC_PI_2, 0.0),
        d if d == Vec3::Z => (0.0, 0.0),
        _ => (PI, 0.0),
    }
}

/// A click with no drag on an end turns the editor camera down that axis at its focus, keeping
/// its distance.
fn click(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    rect: Res<OrientRect>,
    state: Res<EditState>,
    mut editor: Query<(&mut Transform, &mut EditorCamera)>,
    mut down: Local<Option<Vec2>>,
) {
    let cursor = window.and_then(|w| w.cursor_position());
    let Some(r) = rect.0 else {
        *down = None;
        return;
    };
    if mouse.just_pressed(MouseButton::Left) {
        *down = cursor.filter(|c| r.contains(*c));
    }
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let (Some(a), Some(b)) = (down.take(), cursor) else { return };
    if a.distance(b) > CLICK_SLOP {
        return;
    }
    let Some((mut t, mut cam)) = state.editor_camera.and_then(|e| editor.get_mut(e).ok()) else { return };
    let Some(dir) = end_at(t.rotation, r, b) else { return };
    let distance = (t.translation - cam.focus).length();
    (cam.yaw, cam.pitch) = look_down(dir, cam.yaw);
    t.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
    t.translation = cam.focus + t.back().as_vec3() * distance;
}

/// A press in the square never starts a transform gizmo drag.
fn claim(window: Option<Single<&Window, With<PrimaryWindow>>>, rect: Res<OrientRect>, mut gizmo: ResMut<TransformGizmoState>) {
    if gizmo.active || gizmo.hovered_axis.is_none() {
        return;
    }
    if in_square(&rect, window.and_then(|w| w.cursor_position())) {
        gizmo.hovered_axis = None;
    }
}

/// Whether a window point is in the drawn square.
pub fn in_square(rect: &OrientRect, at: Option<Vec2>) -> bool {
    matches!((rect.0, at), (Some(r), Some(c)) if r.contains(c))
}
