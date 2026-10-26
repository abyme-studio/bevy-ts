//! 2D in TSX (change script-2d): `<mesh2d>`, `<material2d>`, sprites, `Camera2d` and sprite
//! picking, through Bevy's own plugins in a headless app that renders to an offscreen image. macOS (Metal).
#![cfg(target_os = "macos")]

use crate::common;

use bevy::camera::{ImageRenderTarget, NormalizedRenderTarget};
use bevy::mesh::VertexAttributeValues;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::sprite::{Anchor, SpriteAlphaMode, SpriteImageMode, SpritePickingMode, SpritePickingSettings, SpriteScalingMode};
use bevy::sprite_render::{AlphaMode2d, ColorMaterial, MeshMaterial2d};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::{DevSource, ReloadReport, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{Offscreen, SIZE};
use bevy_ts_tsx::TsxNode;
use common::*;
use std::sync::{Arc, Mutex};

fn scene(body: &str) -> String {
    format!("import {{ app, sliced, tiled }} from \"bevy-ts\";\nimport {{ Anchor, Camera2d, Image, Projection, Sprite, TextureAtlasLayout, TilemapChunk, TilemapChunkTileData }} from \"bevy-ts/bevy\";\n\napp.scene(\"s\", () => (\n  <>\n{body}  </>\n));\n")
}

/// Bevy's default plugins with no window, every camera drawing into [`Offscreen`], and the first
/// load's report.
fn load(name: &str, body: &str) -> (App, ReloadReport) {
    load_with(name, body, |_| {})
}

/// [`load`], with `plugins` added before the app finishes.
fn load_with(name: &str, body: &str, plugins: impl FnOnce(&mut App)) -> (App, ReloadReport) {
    load_game(name, &scene(body), plugins)
}

/// [`load_with`], given the whole of `game.tsx`.
fn load_game(name: &str, game: &str, plugins: impl FnOnce(&mut App)) -> (App, ReloadReport) {
    load_game_with_assets(name, game, &[], plugins)
}

/// [`load_game`], with `assets` (path and bytes) written under the project's `assets/`, its asset root.
fn load_game_with_assets(name: &str, game: &str, assets: &[(&str, &[u8])], plugins: impl FnOnce(&mut App)) -> (App, ReloadReport) {
    let dir = project(name, &[("game.tsx", game)]);
    for (path, bytes) in assets {
        let p = dir.join("assets").join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(bevy_ts_cli::headless::asset_plugin_unwatched(&dir.join("assets"))),
    )
    .add_plugins(bevy_ts_cli::headless::HeadlessPlugin)
    .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin);
    plugins(&mut app);
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    app.insert_resource(dir);
    (app, r)
}

fn setup(name: &str, body: &str) -> App {
    let (mut app, r) = load(name, body);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    app
}

/// The one error a scene fails its load with.
fn refused(name: &str, body: &str) -> String {
    let (_, r) = load(name, body);
    assert_ne!(r.status, ReloadStatus::Applied, "{body} loaded");
    assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
    r.errors[0].to_string()
}

fn shot(app: &App) -> impl FnMut(&mut App) -> Option<[u8; 3]> + use<> {
    let target = app.world().resource::<Offscreen>().0.clone();
    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let mut frame = 0;
    move |app: &mut App| {
        app.update();
        frame += 1;
        if frame % 20 == 10 {
            let s = shot.clone();
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                *s.lock().unwrap() = Some(e.image.clone());
            });
        }
        let img = shot.lock().unwrap().take()?;
        let i = ((SIZE.1 / 2 * SIZE.0 + SIZE.0 / 2) * 4) as usize;
        let d = img.data.as_ref().unwrap();
        Some([d[i], d[i + 1], d[i + 2]])
    }
}

/// Renders until the centre pixel is `want`, or fails with the last one seen.
fn centre_becomes(app: &mut App, want: [u8; 3], what: &str) {
    let mut next = shot(app);
    let mut last = None;
    for _ in 0..600 {
        if let Some(px) = next(app) {
            last = Some(px);
            if px == want {
                return;
            }
        }
    }
    panic!("the centre pixel never became {what}: {last:?}");
}

/// Every 2D shape, as `<mesh2d>` props.
const SHAPES_2D: &[&str] = &[
    "shape=\"circle\" radius={1}",
    "shape=\"circularSector\" radius={1} angle={1}",
    "shape=\"circularSegment\" radius={1} halfAngle={1}",
    "shape=\"ellipse\" halfSize={[2, 1]}",
    "shape=\"annulus\" innerRadius={0.5} outerRadius={1}",
    "shape=\"rectangle\" size={[2, 1]}",
    "shape=\"rhombus\" horizontalDiagonal={2} verticalDiagonal={1}",
    "shape=\"regularPolygon\" circumradius={1} sides={5}",
    "shape=\"triangle2d\" vertices={[[0, 1], [-1, 0], [1, 0]]}",
    "shape=\"capsule2d\" radius={0.5} length={1}",
    "shape=\"convexPolygon\" vertices={[[0, 0], [1, 0], [1, 1], [0, 1]]}",
];

#[test]
fn every_2d_shape_builds_a_flat_mesh2d() {
    let body: String = SHAPES_2D.iter().enumerate().map(|(i, p)| format!("    <entity key=\"m{i}\"><mesh2d {p} /></entity>\n")).collect();
    let mut app = setup("2d-shapes", &body);
    for (i, p) in SHAPES_2D.iter().enumerate() {
        let e = node(&mut app, &format!("m{i}"));
        assert!(app.world().get::<Mesh3d>(e).is_none(), "{p}: a Mesh3d");
        let h = app.world().get::<Mesh2d>(e).unwrap_or_else(|| panic!("{p}: no Mesh2d")).0.clone();
        let mesh = app.world().resource::<Assets<Mesh>>().get(&h).unwrap_or_else(|| panic!("{p}: no mesh"));
        let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!("{p}: no normals") };
        assert!(normals.iter().all(|n| *n == [0.0, 0.0, 1.0]), "{p}: a normal off +Z");
    }
}

#[test]
fn a_material2d_builds_its_color_material_and_equal_props_share_handles() {
    let body = concat!(
        "    <entity key=\"a\"><mesh2d shape=\"circle\" /><material2d color=\"#ff0000\" alpha={0.5} alphaMode={{ mask: 0.25 }} texture=\"hero.png\" uvTransform={{ scale: [2, 2] }} /></entity>\n",
        "    <entity key=\"b\"><mesh2d shape=\"circle\" /><material2d color=\"#ff0000\" alpha={0.5} alphaMode={{ mask: 0.25 }} texture=\"hero.png\" uvTransform={{ scale: [2, 2] }} /></entity>\n",
        "    <entity key=\"faded\"><mesh2d shape=\"circle\" /><material2d color=\"#00ff0080\" /></entity>\n",
        "    <entity key=\"tinted\"><mesh2d shape=\"circle\" color=\"#0000ff\" /></entity>\n",
        "    <entity key=\"solid\"><mesh shape=\"circle\" /></entity>\n",
    );
    let mut app = setup("2d-material", body);
    let [a, b, faded, tinted, solid] = ["a", "b", "faded", "tinted", "solid"].map(|k| node(&mut app, k));
    let w = app.world();
    let material = |e: Entity| w.get::<MeshMaterial2d<ColorMaterial>>(e).expect("a MeshMaterial2d").0.clone();
    assert_eq!(material(a), material(b), "equal props share a material");
    assert_eq!(w.get::<Mesh2d>(a).unwrap().0, w.get::<Mesh2d>(b).unwrap().0, "equal props share a mesh");
    assert_eq!(w.get::<Mesh2d>(a).unwrap().0, w.get::<Mesh3d>(solid).unwrap().0, "a Mesh2d and a Mesh3d of one shape share the mesh");

    let materials = w.resource::<Assets<ColorMaterial>>();
    let m = materials.get(&material(a)).unwrap();
    assert_eq!(m.color, Color::srgba(1.0, 0.0, 0.0, 0.5));
    assert_eq!(m.alpha_mode, AlphaMode2d::Mask(0.25));
    assert_eq!(m.uv_transform.matrix2.x_axis.x, 2.0);
    assert_eq!(m.texture.as_ref().and_then(|t| t.path()).and_then(|p| p.path().to_str().map(String::from)).as_deref(), Some("hero.png"));
    assert_eq!(materials.get(&material(faded)).unwrap().alpha_mode, AlphaMode2d::Blend, "alpha below 1 blends");
    let tint = materials.get(&material(tinted)).unwrap();
    assert_eq!((tint.color, tint.alpha_mode), (Color::srgb(0.0, 0.0, 1.0), AlphaMode2d::Opaque), "<mesh2d color>");
}

#[test]
fn a_changed_mesh2d_or_material2d_rebuilds_and_a_dropped_color_takes_its_material() {
    let ent = |props: &str| format!("    <entity key=\"m\">{props}</entity>\n");
    let mut app = setup("2d-reload", &ent("<mesh2d shape=\"circle\" color=\"#ff0000\" />"));
    let dir = app.world().resource::<common::TestDir>().clone();
    let e = node(&mut app, "m");
    let color = |app: &App| {
        let h = &app.world().get::<MeshMaterial2d<ColorMaterial>>(e)?.0;
        Some(app.world().resource::<Assets<ColorMaterial>>().get(h)?.color)
    };
    let mesh = |app: &App| app.world().get::<Mesh2d>(e).map(|m| m.0.clone());
    assert_eq!(color(&app), Some(Color::srgb(1.0, 0.0, 0.0)));
    let circle = mesh(&app).expect("a Mesh2d");

    let save = |app: &mut App, props: &str| {
        common::write(&dir, "game.tsx", &scene(&ent(props)));
        let r = reload(app, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        app.update();
    };
    save(&mut app, "<mesh2d shape=\"rectangle\" color=\"#0000ff\" />");
    assert_ne!(mesh(&app).as_ref(), Some(&circle), "a new shape builds a new mesh");
    assert_eq!(color(&app), Some(Color::srgb(0.0, 0.0, 1.0)));

    save(&mut app, "<mesh2d shape=\"rectangle\" />");
    assert!(mesh(&app).is_some());
    assert_eq!(color(&app), None, "the colour's material goes with it");

    save(&mut app, "<mesh2d shape=\"rectangle\" /><material2d color=\"#00ff00\" />");
    assert_eq!(color(&app), Some(Color::srgb(0.0, 1.0, 0.0)));
    save(&mut app, "<mesh2d shape=\"rectangle\" /><material2d color=\"#ffffff\" />");
    assert_eq!(color(&app), Some(Color::srgb(1.0, 1.0, 1.0)), "a changed prop rebuilds the material");
    save(&mut app, "<mesh2d shape=\"rectangle\" />");
    assert_eq!(color(&app), None, "a removed <material2d> takes its material");
}

#[test]
fn mesh2d_and_material2d_refuse_what_2d_does_not_have_naming_the_line() {
    for (name, body, want) in [
        ("2d-no-depth", "<mesh2d shape=\"circle\" depth={1} />", "circle: depth: <mesh2d> is flat"),
        ("2d-no-segments", "<mesh2d shape=\"rectangle\" segments={2} />", "rectangle: segments: <mesh2d> is flat"),
        ("2d-no-3d-shape", "<mesh2d shape=\"cuboid\" />", "shape: cuboid is a 3D shape"),
        ("2d-bad-shape-prop", "<mesh2d shape=\"circle\" radius={-1} />", "<mesh2d>: circle: radius"),
        ("2d-no-metallic", "<material2d metallic={1} />", "metallic"),
        ("2d-no-add", "<material2d alphaMode=\"add\" />", "<material2d> takes \"opaque\", \"blend\" or { mask }"),
        ("2d-no-3d-pair", "<mesh shape=\"circle\" /><material2d />", "pair <mesh> with <material>, <mesh2d> with <material2d>"),
        ("2d-no-2d-pair", "<mesh2d shape=\"circle\" /><material />", "pair <mesh> with <material>, <mesh2d> with <material2d>"),
        ("2d-no-two-colours", "<mesh2d shape=\"circle\" color=\"#f00\" /><material2d />", "not both"),
    ] {
        let e = refused(name, &format!("    <entity>\n      {body}\n    </entity>\n"));
        assert!(e.contains(want), "{body}: {e}");
        assert!(e.contains("game.tsx:7:"), "{body}: names its line: {e}");
    }
}

#[test]
fn a_camera2d_draws_a_red_mesh2d_and_takes_a_projection() {
    let body = concat!(
        "    <entity key=\"cam\"><Camera2d /><Projection value={{ Orthographic: { scale: 2 } }} /></entity>\n",
        "    <entity key=\"r\"><mesh2d shape=\"rectangle\" size={[4000, 4000]} color=\"#ff0000\" /></entity>\n",
    );
    let mut app = setup("2d-camera-mesh", body);
    let cam = node(&mut app, "cam");
    let Some(Projection::Orthographic(o)) = app.world().get::<Projection>(cam) else { panic!("an orthographic Projection") };
    assert_eq!(o.scale, 2.0);
    centre_becomes(&mut app, [255, 0, 0], "the red rectangle");
}

fn node(app: &mut App, key: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == [key]).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {key}"))
}

#[test]
fn a_sprite_takes_an_image_path_and_flips() {
    let mut app = setup("2d-sprite-image", "    <entity key=\"hero\"><Sprite image=\"hero.png\" flip_x /></entity>\n");
    let e = node(&mut app, "hero");
    let sprite = app.world().get::<Sprite>(e).expect("a Sprite");
    assert!(sprite.flip_x && !sprite.flip_y);
    let path = sprite.image.path().expect("a handle loaded from a path");
    assert_eq!(path.path().to_str(), Some("hero.png"));
}

#[test]
fn a_sprites_fields_reach_bevy_in_its_reflected_form_and_anchor_sets_the_anchor() {
    let body = concat!(
        "    <entity key=\"a\"><Sprite color=\"#ff8000\" custom_size={[32, 16]} rect={{ min: [1, 2], max: [17, 18] }} image_mode={{ Scale: \"FitCenter\" }} alpha_mode={{ Mask: 0.25 }} /><Anchor value={[-0.5, 0.5]} /></entity>\n",
        "    <entity key=\"b\"><Sprite image_mode={{ Sliced: { border: { min_inset: [4, 5], max_inset: [6, 7] }, center_scale_mode: { Tile: { stretch_value: 2 } }, max_corner_scale: 0.5 } }} alpha_mode=\"Opaque\" /></entity>\n",
        "    <entity key=\"c\"><Sprite image_mode={{ Tiled: { tile_x: true, tile_y: false, stretch_value: 1.5 } }} /></entity>\n",
        "    <entity key=\"d\"><Sprite image_mode=\"Auto\" /></entity>\n",
    );
    let mut app = setup("2d-sprite-fields", body);
    let a = node(&mut app, "a");
    let s = app.world().get::<Sprite>(a).expect("a Sprite");
    assert_eq!(s.color.to_srgba().to_hex(), "#FF8000");
    assert_eq!(s.custom_size, Some(Vec2::new(32.0, 16.0)));
    assert_eq!(s.rect, Some(Rect::new(1.0, 2.0, 17.0, 18.0)));
    assert!(matches!(s.image_mode, SpriteImageMode::Scale(SpriteScalingMode::FitCenter)), "{:?}", s.image_mode);
    assert!(matches!(s.alpha_mode, SpriteAlphaMode::Mask(m) if m == 0.25), "{:?}", s.alpha_mode);
    assert_eq!(app.world().get::<Anchor>(a).expect("an Anchor").as_vec(), Vec2::new(-0.5, 0.5));

    let b = node(&mut app, "b");
    let s = app.world().get::<Sprite>(b).unwrap();
    let SpriteImageMode::Sliced(slicer) = &s.image_mode else { panic!("sliced: {:?}", s.image_mode) };
    assert_eq!((slicer.border.min_inset, slicer.border.max_inset), (Vec2::new(4.0, 5.0), Vec2::new(6.0, 7.0)));
    assert!(matches!(slicer.center_scale_mode, bevy::sprite::SliceScaleMode::Tile { stretch_value } if stretch_value == 2.0));
    assert_eq!(slicer.max_corner_scale, 0.5);
    assert!(matches!(s.alpha_mode, SpriteAlphaMode::Opaque));

    let c = node(&mut app, "c");
    let s = app.world().get::<Sprite>(c).unwrap();
    assert!(matches!(s.image_mode, SpriteImageMode::Tiled { tile_x: true, tile_y: false, stretch_value } if stretch_value == 1.5), "{:?}", s.image_mode);
    let d = node(&mut app, "d");
    assert!(matches!(app.world().get::<Sprite>(d).unwrap().image_mode, SpriteImageMode::Auto));
}

#[test]
fn ui_sliced_and_tiled_helpers_are_a_sprites_image_mode() {
    let body = concat!(
        "    <entity key=\"a\"><Sprite image_mode={sliced({ border: 8 })} /></entity>\n",
        "    <entity key=\"b\"><Sprite image_mode={sliced({ border: { left: 1, right: 2, top: 3, bottom: 4 }, center: { tile: 2 } })} /></entity>\n",
        "    <entity key=\"c\"><Sprite image_mode={tiled({ y: false, stretch: 1.5 })} /></entity>\n",
    );
    let mut app = setup("2d-sprite-helpers", body);
    let a = node(&mut app, "a");
    let SpriteImageMode::Sliced(slicer) = &app.world().get::<Sprite>(a).unwrap().image_mode else { panic!("sliced") };
    assert_eq!((slicer.border.min_inset, slicer.border.max_inset), (Vec2::splat(8.0), Vec2::splat(8.0)), "an 8 px border on every side");
    assert!(matches!(slicer.center_scale_mode, bevy::sprite::SliceScaleMode::Stretch));
    let b = node(&mut app, "b");
    let SpriteImageMode::Sliced(slicer) = &app.world().get::<Sprite>(b).unwrap().image_mode else { panic!("sliced") };
    assert_eq!((slicer.border.min_inset, slicer.border.max_inset), (Vec2::new(1.0, 3.0), Vec2::new(2.0, 4.0)), "min is left and top");
    assert!(matches!(slicer.center_scale_mode, bevy::sprite::SliceScaleMode::Tile { stretch_value } if stretch_value == 2.0));
    let c = node(&mut app, "c");
    assert!(matches!(app.world().get::<Sprite>(c).unwrap().image_mode, SpriteImageMode::Tiled { tile_x: true, tile_y: false, stretch_value } if stretch_value == 1.5));
}

#[test]
fn a_camera2d_is_orthographic_and_draws_a_sprite() {
    // Sprite's default image is white, and the default clear colour is grey: a white centre
    // pixel is the sprite drawn through the Camera2d.
    let body = "    <entity key=\"cam\"><Camera2d /></entity>\n    <entity key=\"s\"><Sprite custom_size={[2000, 2000]} /></entity>\n";
    let mut app = setup("2d-camera", body);
    let cam = node(&mut app, "cam");
    assert!(app.world().get::<Camera>(cam).is_some(), "Camera2d requires Camera");
    assert!(matches!(app.world().get::<Projection>(cam), Some(Projection::Orthographic(_))), "an orthographic Projection");

    centre_becomes(&mut app, [255, 255, 255], "the white sprite");
}

/// A mouse at `at` on the offscreen image, then `action`, then a frame.
fn pointer(app: &mut App, at: Vec2, action: PointerAction) {
    let target = app.world().resource::<Offscreen>().0.clone();
    let location = Location { target: NormalizedRenderTarget::Image(ImageRenderTarget::from(target)), position: at };
    app.world_mut().write_message(PointerInput::new(PointerId::Mouse, location, action));
    app.update();
}

/// Clicks the centre, then a corner, of a `Camera2d` scene whose entity `s` logs its clicks:
/// one click, on `s`.
fn clicks_once_at_the_centre(mut app: App) {
    // Bevy 0.20 casts a pointer's ray only when some window is primary, even into an image target;
    // `bevy-ts dev --headless` spawns this one too (mesh-picking design 5a).
    bevy_ts_viewport::spawn_windowless_primary(app.world_mut(), "two_d", (SIZE.0, SIZE.1, 1.0), ());
    let e = node(&mut app, "s");
    updates(&mut app, 3);
    let centre = Vec2::new(SIZE.0 as f32 / 2.0, SIZE.1 as f32 / 2.0);
    pointer(&mut app, centre, PointerAction::Move { delta: Vec2::ZERO });
    pointer(&mut app, centre, PointerAction::Press(PointerButton::Primary));
    pointer(&mut app, centre, PointerAction::Release(PointerButton::Primary));
    updates(&mut app, 2);
    let clicks: Vec<String> = messages(&app).into_iter().filter(|m| m.starts_with("clicked")).collect();
    assert_eq!(clicks, vec![format!("clicked {}", e.to_bits())]);

    // A click off it picks nothing.
    let corner = Vec2::new(10.0, 10.0);
    pointer(&mut app, corner, PointerAction::Move { delta: Vec2::ZERO });
    pointer(&mut app, corner, PointerAction::Press(PointerButton::Primary));
    pointer(&mut app, corner, PointerAction::Release(PointerButton::Primary));
    updates(&mut app, 2);
    assert_eq!(messages(&app).iter().filter(|m| m.starts_with("clicked")).count(), 1);
}

#[test]
fn clicking_a_sprite_runs_its_handler_once_with_its_entity() {
    let body = "    <entity key=\"cam\"><Camera2d /></entity>\n    <entity key=\"s\" onPointerClick={({ entity }) => console.log(`clicked ${entity}`)}><Sprite custom_size={[100, 100]} /></entity>\n";
    let mut app = setup("2d-pick-sprite", body);
    // No image loads: pick the sprite's rectangle, not its texels (design 6).
    app.world_mut().resource_mut::<SpritePickingSettings>().picking_mode = SpritePickingMode::BoundingBox;
    clicks_once_at_the_centre(app);
}

#[test]
fn clicking_a_mesh2d_runs_its_handler_once_with_its_entity() {
    // Bevy's MeshPickingPlugin casts into `Mesh2d`s too; `meshPicking` in bevy-ts.json adds it.
    let body = "    <entity key=\"cam\"><Camera2d /></entity>\n    <entity key=\"s\" onPointerClick={({ entity }) => console.log(`clicked ${entity}`)}><mesh2d shape=\"circle\" radius={50} color=\"#0f0\" /></entity>\n";
    let (mut app, r) = load_with("2d-pick-mesh2d", body, |app| {
        app.add_plugins(bevy::picking::mesh_picking::MeshPickingPlugin);
    });
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    clicks_once_at_the_centre(app);
}

const SCORE: &str = r##"import { app, Component, u8, signal, Update, type Query } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export class Hit extends Component { static path = "game::Hit"; declare on: u8 }
export const [score, setScore] = signal(1);
export function scored(q: Query<[Hit]>) {
  q.each((h) => { if (h.on === 1) setScore(2); });
}
app.addSystems(Update, scored);

app.scene("s", () => (
  <entity key="score">
    <Transform translation={[0, 40, 0]} />
    <text2d size={24} color="#fff" anchor={[0, -0.5]} bounds={[200, null]}>
      {() => `${score()}`}<span color="#f44">!</span>
    </text2d>
  </entity>
));
"##;

#[test]
fn a_bound_text2d_writes_only_its_text2d_and_its_span_is_a_child() {
    let (mut app, r) = load_game("two-d-text2d", SCORE, |_| {});
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let world = app.world_mut();
    let e = world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["score"]).map(|(e, _)| e).expect("the score");
    let w = app.world();
    assert_eq!(w.get::<Text2d>(e).unwrap().0, "1");
    assert_eq!(w.get::<TextFont>(e).unwrap().font_size, bevy::text::FontSize::Px(24.0));
    assert_eq!(w.get::<TextColor>(e).unwrap().0.to_srgba(), Srgba::WHITE);
    assert_eq!(w.get::<Anchor>(e).unwrap().0, Vec2::new(0.0, -0.5));
    assert_eq!(w.get::<bevy::text::TextBounds>(e).unwrap().width, Some(200.0));
    assert!(w.get::<Node>(e).is_none(), "<text2d> is no UI node");
    let span = w.get::<Children>(e).expect("the span").iter().find(|c| w.get::<TextSpan>(*c).is_some()).expect("a TextSpan child");
    assert_eq!(w.get::<TextSpan>(span).unwrap().0, "!");
    // A span takes its parent's size it does not set, as on <text>.
    assert_eq!(w.get::<TextFont>(span).unwrap().font_size, bevy::text::FontSize::Px(24.0));

    let ticks = |app: &App| {
        let e = app.world().entity(e);
        (e.get_ref::<TextFont>().unwrap().last_changed(), e.get_ref::<TextColor>().unwrap().last_changed(), e.get_ref::<Anchor>().unwrap().last_changed())
    };
    let before = ticks(&app);
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Hit").unwrap();
    let hit = app.world_mut().spawn_empty().id();
    // SAFETY: Hit is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(hit), id, &[1]) };
    updates(&mut app, 3);
    assert_eq!(app.world().get::<Text2d>(e).unwrap().0, "2");
    assert_eq!(ticks(&app), before, "only Text2d is written");
}

/// One grid, given to two sprites and a UI image.
const GRID: &str = "TextureAtlasLayout.fromGrid({ tileSize: [8, 8], columns: 8, rows: 4 })";

#[test]
fn sprites_and_a_ui_image_given_one_grid_share_one_32_tile_layout() {
    let body = format!(
        "    <entity key=\"a\"><Sprite image=\"arrow.png\" texture_atlas={{{{ layout: {GRID}, index: 3 }}}} /></entity>\n\
         <entity key=\"b\"><Sprite image=\"arrow.png\" texture_atlas={{{{ layout: {GRID}, index: 31 }}}} /></entity>\n\
         <image key=\"c\" src=\"arrow.png\" atlas={{{{ layout: {GRID}, index: 0 }}}} />\n"
    );
    let mut app = setup("two-d-atlas", &body);
    let world = app.world_mut();
    let by_key = |world: &mut World, k: &str| world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == [k]).map(|(e, _)| e).unwrap();
    let (a, b, c) = (by_key(world, "a"), by_key(world, "b"), by_key(world, "c"));
    let w = app.world();
    let atlas = |e: Entity| w.get::<Sprite>(e).unwrap().texture_atlas.clone().expect("an atlas");
    let (ta, tb) = (atlas(a), atlas(b));
    assert_eq!((ta.index, tb.index), (3, 31));
    assert_eq!(ta.layout, tb.layout, "one grid, one layout");
    let ui = w.get::<ImageNode>(c).unwrap().texture_atlas.clone().expect("the image's atlas");
    assert_eq!(ui.layout, ta.layout, "the UI image shares the sprites' layout");
    let layout = w.resource::<Assets<bevy::image::TextureAtlasLayout>>().get(&ta.layout).expect("the layout is built");
    assert_eq!(layout.textures.len(), 32);
    assert_eq!(layout.textures[3], URect::new(24, 0, 32, 8));
}

#[test]
fn a_sprite_atlas_index_past_its_grid_names_index_and_count() {
    let e = refused("two-d-atlas-index", &format!("    <entity><Sprite image=\"arrow.png\" texture_atlas={{{{ layout: {GRID}, index: 32 }}}} /></entity>\n"));
    assert!(e.contains("texture_atlas.index 32") && e.contains("32 tiles"), "{e}");
}

/// Bevy's `sprite_sheet` example as a TS system: a timer on the sprite steps its atlas index.
const SHEET: &str = r##"import { app, Component, FixedUpdate, Time, Timer, type Mut, type Query, type Res, type u32 } from "bevy-ts";
import { Sprite, TextureAtlasLayout } from "bevy-ts/bevy";

export class Frames extends Component {
  timer = Timer.fromSeconds(0.1, "Repeating");
  last: u32 = 7;
}

export function animate(time: Res<Time>, sheets: Query<[Mut<Sprite>, Mut<Frames>]>) {
  for (const [sprite, frames] of sheets) {
    if (!frames.timer.tick(time.delta_secs).just_finished) continue;
    const atlas = sprite.texture_atlas;
    if (atlas) atlas.index = atlas.index === frames.last ? 0 : atlas.index + 1;
  }
}

app.addSystems(FixedUpdate, animate);
app.scene("s", () => (
  <entity key="sheet">
    <Sprite image="arrow.png" texture_atlas={{ layout: TextureAtlasLayout.fromGrid({ tileSize: [8, 8], columns: 8, rows: 1 }), index: 0 }} />
    <Frames />
  </entity>
));
"##;

#[test]
fn a_ts_system_on_a_timer_steps_a_sprites_atlas_index() {
    let (mut app, r) = load_game("two-d-sheet", SHEET, |_| {});
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    // 35 ms steps, so no frame falls on a step's edge: the timer finishes at 0.105, 0.21 and 0.315 s.
    let step = std::time::Duration::from_millis(35);
    app.world_mut().resource_mut::<Time<Fixed>>().set_timestep(step);
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(step));
    // The first frame mounts the scene and starts the clock; each next one runs one fixed step.
    app.update();
    let world = app.world_mut();
    let e = world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["sheet"]).map(|(e, _)| e).expect("the sheet");
    let index = |app: &App| app.world().get::<Sprite>(e).unwrap().texture_atlas.as_ref().expect("an atlas").index;
    assert_eq!(index(&app), 0);
    updates(&mut app, 10);
    assert_eq!(app.world().resource::<Time<Fixed>>().elapsed(), step * 10, "0.35 s of fixed steps");
    assert_eq!(index(&app), 3);
}

// Change pixel-art-2d, stage 2: `<YSort>` from TSX, and a bound y drawn at its z the same frame.

const WALKER: &str = r##"import { app, Component, u8, signal, Update, type Query } from "bevy-ts";
import { Sprite, Transform, YSort } from "bevy-ts/bevy";

export class Step extends Component { static path = "game::Step"; declare on: u8 }
export const [y, setY] = signal(0);
export function stepped(q: Query<[Step]>) {
  q.each((s) => { if (s.on === 1) setY(40); });
}
app.addSystems(Update, stepped);

app.scene("s", () => (
  <entity key="hero">
    <Sprite custom_size={[16, 16]} />
    <Transform translation={() => [0, y(), 0]} />
    <YSort layer={1} offset={-8} />
  </entity>
));
"##;

#[test]
fn a_ysort_from_tsx_draws_a_bound_y_at_its_z_the_same_frame() {
    let (mut app, r) = load_game("two-d-ysort", WALKER, |app| {
        app.add_plugins(bevy_ts_material::Pixel2dPlugin);
    });
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let e = node(&mut app, "hero");
    assert_eq!(*app.world().get::<bevy_ts_material::YSort>(e).unwrap(), bevy_ts_material::YSort { layer: 1.0, offset: -8.0 });
    let z = |app: &App| app.world().get::<GlobalTransform>(e).unwrap().translation();
    assert_eq!(z(&app).z, bevy_ts_material::pixel2d::y_sort_z(1.0, 0.0, -8.0));
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Step").unwrap();
    let step = app.world_mut().spawn_empty().id();
    // SAFETY: Step is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(step), id, &[1]) };
    app.update();
    let drawn = z(&app);
    assert_eq!(drawn.y, 40.0, "the binding moved the hero this frame");
    assert_eq!(drawn.z, bevy_ts_material::pixel2d::y_sort_z(1.0, 40.0, -8.0), "and its z follows the same frame");
}

// Change pixel-art-2d, stage 7: a `<ScreenSize>` marker at zoom 4 is picked over its drawn size.

/// A primary click at `at` on the offscreen view.
fn click(app: &mut App, at: Vec2) {
    pointer(app, at, PointerAction::Move { delta: Vec2::ZERO });
    pointer(app, at, PointerAction::Press(PointerButton::Primary));
    pointer(app, at, PointerAction::Release(PointerButton::Primary));
    updates(app, 2);
}

#[test]
fn a_screen_size_marker_at_zoom_4_is_clicked_over_its_drawn_area() {
    let body = concat!(
        "    <entity key=\"cam\"><Camera2d /><Projection value={{ Orthographic: { scale: 0.25 } }} /></entity>\n",
        "    <entity key=\"s\" onPointerClick={({ entity }) => console.log(`clicked ${entity}`)}><Sprite custom_size={[16, 16]} /><ScreenSize /></entity>\n",
    );
    let game = scene(body).replace("import { Anchor, Camera2d,", "import { Anchor, ScreenSize, Camera2d,");
    let (mut app, r) = load_game("2d-pick-screen-size", &game, |app| {
        app.add_plugins(bevy_ts_material::Pixel2dPlugin);
    });
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.world_mut().resource_mut::<SpritePickingSettings>().picking_mode = SpritePickingMode::BoundingBox;
    bevy_ts_viewport::spawn_windowless_primary(app.world_mut(), "two_d", (SIZE.0, SIZE.1, 1.0), ());
    let e = node(&mut app, "s");
    updates(&mut app, 3);
    assert_eq!(app.world().get::<Transform>(e).unwrap().scale.truncate(), Vec2::splat(0.25));
    let centre = Vec2::new(SIZE.0 as f32 / 2.0, SIZE.1 as f32 / 2.0);
    // 12 px from the centre is outside its 16 drawn pixels, though inside the 64 an unscaled sprite would cover.
    click(&mut app, centre + Vec2::new(12.0, 0.0));
    let clicks = |app: &App| messages(app).into_iter().filter(|m| m.starts_with("clicked")).collect::<Vec<_>>();
    assert!(clicks(&app).is_empty(), "{:?}", clicks(&app));
    click(&mut app, centre + Vec2::new(6.0, 0.0));
    assert_eq!(clicks(&app), vec![format!("clicked {}", e.to_bits())]);
}

// Change pixel-art-2d, stage 3: a click at a snapped pixel camera hits what is drawn.

#[test]
fn a_click_at_a_snapped_pixel_camera_hits_the_drawn_sprite() {
    // At zoom 4 the camera at x 0.3 draws from 0.25: the 4-unit sprite at 0 spans screen -9 to 7
    // px about the centre, where unsnapped it would span -9.2 to 6.8.
    let body = concat!(
        "    <entity key=\"cam\"><Camera2d /><PixelCamera zoom={4} ease={0} /><Transform translation={[0.3, 0, 0]} /></entity>\n",
        "    <entity key=\"s\" onPointerClick={({ entity }) => console.log(`clicked ${entity}`)}><Sprite custom_size={[4, 4]} /></entity>\n",
    );
    let game = scene(body).replace("import { Anchor, Camera2d,", "import { Anchor, PixelCamera, Transform, Camera2d,");
    let (mut app, r) = load_game("2d-pick-pixel-camera", &game, |app| {
        app.add_plugins(bevy_ts_material::Pixel2dPlugin);
    });
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.world_mut().resource_mut::<SpritePickingSettings>().picking_mode = SpritePickingMode::BoundingBox;
    bevy_ts_viewport::spawn_windowless_primary(app.world_mut(), "two_d", (SIZE.0, SIZE.1, 1.0), ());
    updates(&mut app, 3);
    let centre = Vec2::new(SIZE.0 as f32 / 2.0, SIZE.1 as f32 / 2.0);
    let clicks = |app: &App| messages(app).into_iter().filter(|m| m.starts_with("clicked")).count();
    click(&mut app, centre + Vec2::new(-9.1, 0.0));
    assert_eq!(clicks(&app), 0, "left of the drawn sprite");
    click(&mut app, centre + Vec2::new(6.9, 0.0));
    assert_eq!(clicks(&app), 1, "inside the drawn sprite");
    let cam = node(&mut app, "cam");
    assert_eq!(app.world().get::<Transform>(cam).unwrap().translation.x, 0.3);
}

// Change pixel-art-2d, stage 6: `<SpriteClip>` from TSX.

#[test]
fn a_sprite_clip_from_tsx_steps_its_atlas_index() {
    let body = format!(
        "    <entity key=\"walk\"><Sprite image=\"arrow.png\" texture_atlas={{{{ layout: {GRID}, index: 0 }}}} /><SpriteClip first={{0}} last={{3}} fps={{10}} mode=\"PingPong\" /></entity>\n"
    );
    let game = scene(&body).replace("import { Anchor, Camera2d,", "import { Anchor, SpriteClip, Camera2d,");
    let (mut app, r) = load_game("two-d-clip", &game, |app| {
        app.add_plugins(bevy_ts_material::Pixel2dPlugin);
    });
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(50)));
    app.update();
    let e = node(&mut app, "walk");
    let clip = *app.world().get::<bevy_ts_material::SpriteClip>(e).unwrap();
    assert_eq!((clip.first, clip.last, clip.fps, clip.mode, clip.playing), (0, 3, 10.0, bevy_ts_material::ClipMode::PingPong, true));
    // 0.45 s: frames 0, 1, 2, 3, then 2.
    updates(&mut app, 9);
    assert_eq!(app.world().get::<Sprite>(e).unwrap().texture_atlas.as_ref().unwrap().index, 2);
}

// Change pixel-art-2d, stage 5: a bound `<bitmapText>` re-lays its glyphs.

const LABEL: &str = r##"import { app, Component, u8, signal, Update, type Query } from "bevy-ts";

export class Grow extends Component { static path = "game::Grow"; declare on: u8 }
export const [name, setName] = signal("AL");
export function grown(q: Query<[Grow]>) {
  q.each((g) => { if (g.on === 1) setName("BEA"); });
}
app.addSystems(Update, grown);

app.scene("s", () => (
  <entity key="label">
    <bitmapText font="fonts/tiny.json" scale={2}>{() => name()}</bitmapText>
  </entity>
));
"##;

#[test]
fn a_bound_bitmap_text_goes_from_two_glyphs_to_three() {
    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/shapes-2d/assets/fonts");
    let (json, png) = (std::fs::read(fonts.join("tiny.json")).unwrap(), std::fs::read(fonts.join("tiny.png")).unwrap());
    let (mut app, r) = load_game_with_assets("two-d-bitmap-text", LABEL, &[("fonts/tiny.json", &json), ("fonts/tiny.png", &png)], |app| {
        app.add_plugins(bevy_ts_material::BitmapTextPlugin);
    });
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let e = node(&mut app, "label");
    let glyphs = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&ChildOf, With<bevy_ts_material::bitmap_text::Glyph>>().iter(world).filter(|c| c.parent() == e).count()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while glyphs(&mut app) == 0 {
        assert!(std::time::Instant::now() < deadline, "the font never loaded");
        app.update();
    }
    assert_eq!(glyphs(&mut app), 2, "AL");
    let label = app.world().get::<bevy_ts_material::BitmapText>(e).unwrap();
    assert_eq!((label.text.as_str(), label.scale), ("AL", 2.0));
    let before = app.world().entity(e).get_ref::<Transform>().unwrap().last_changed();
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Grow").unwrap();
    let grow = app.world_mut().spawn_empty().id();
    // SAFETY: Grow is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(grow), id, &[1]) };
    updates(&mut app, 2);
    assert_eq!(glyphs(&mut app), 3, "BEA");
    assert_eq!(app.world().entity(e).get_ref::<Transform>().unwrap().last_changed(), before, "the entity's own components are unchanged");
}

/// A tileset of four 8-pixel tiles stacked in a column, one colour each.
fn tileset_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 8, 32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let rows: Vec<u8> = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]].iter().flat_map(|c| c.repeat(8 * 8)).collect();
    enc.write_header().unwrap().write_image_data(&rows).unwrap();
    out
}

/// A 4x4 chunk, its tile data after it in the source, with a bound display size.
const TILEMAP: &str = r##"import { app, Component, u8, signal, Update, type Query } from "bevy-ts";
import { Image, TilemapChunk, TilemapChunkTileData } from "bevy-ts/bevy";

export class Hit extends Component { static path = "game::Hit"; declare on: u8 }
export const [size, setSize] = signal(16);
export function grow(q: Query<[Hit]>) {
  q.each((h) => { if (h.on === 1) setSize(32); });
}
app.addSystems(Update, grow);

const tileset = Image.load("tiles.png", { array_layout: { RowCount: { rows: 4 } } });
const tiles = Array.from({ length: 16 }, (_, i) => (i === 5 ? null : { tileset_index: i % 4 }));
app.scene("s", () => (
  <entity key="map">
    <TilemapChunk chunk_size={[4, 4]} tile_display_size={() => [size(), size()]} tileset={tileset} />
    <TilemapChunkTileData value={tiles} />
  </entity>
));
"##;

#[test]
fn a_tilemap_chunk_mounts_with_an_array_tileset_and_a_changed_prop_inserts_it_again() {
    use bevy::sprite_render::{TilemapChunk, TilemapChunkMaterial, TilemapChunkTileData};
    let (mut app, r) = load_game_with_assets("two-d-tilemap", TILEMAP, &[("tiles.png", &tileset_png())], |_| {});
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let world = app.world_mut();
    let e = world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["map"]).map(|(e, _)| e).expect("the map");
    let w = app.world();
    let chunk = w.get::<TilemapChunk>(e).expect("a TilemapChunk").clone();
    assert_eq!((chunk.chunk_size, chunk.tile_display_size), (UVec2::splat(4), UVec2::splat(16)));
    let data = w.get::<TilemapChunkTileData>(e).expect("its tile data");
    assert_eq!(data.len(), 16);
    assert!(data[5].is_none(), "null is an empty tile");
    assert_eq!(data[6].map(|t| (t.tileset_index, t.visible)), Some((2, true)), "a tile's unset fields take TileData's default");
    // Bevy's insert hook built the chunk: it found the tile data, though the source gives it second.
    assert!(w.get::<MeshMaterial2d<TilemapChunkMaterial>>(e).is_some(), "the chunk's material");
    let mesh = w.get::<Mesh2d>(e).expect("the chunk's mesh").0.clone();
    let mut layers = None;
    for _ in 0..200 {
        app.update();
        layers = app.world().resource::<Assets<Image>>().get(&chunk.tileset).map(|i| i.texture_descriptor.array_layer_count());
        if layers.is_some() {
            break;
        }
    }
    assert_eq!(layers, Some(4), "the tileset is an array texture, one layer per tile");
    let errors: Vec<_> = logs(&app).into_iter().filter(|l| l.level == bevy_ts::log::LogLevel::Error).map(|l| l.message).collect();
    assert!(errors.is_empty(), "{errors:?}");

    // TilemapChunk is immutable: its changed prop inserts it again, and the hook builds a new mesh.
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Hit").unwrap();
    let hit = app.world_mut().spawn_empty().id();
    // SAFETY: Hit is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(hit), id, &[1]) };
    updates(&mut app, 3);
    let w = app.world();
    assert_eq!(w.get::<TilemapChunk>(e).unwrap().tile_display_size, UVec2::splat(32));
    assert_eq!(w.get::<TilemapChunk>(e).unwrap().tileset, chunk.tileset, "the other fields are kept");
    assert_ne!(w.get::<Mesh2d>(e).unwrap().0, mesh, "a 128-pixel chunk has its own mesh");
}

#[test]
fn a_tilemap_refuses_a_tile_count_off_its_size_and_a_tileset_with_no_array_layout() {
    let chunk = |tileset: &str, n: usize| {
        format!("    <entity><TilemapChunk chunk_size={{[4, 4]}} tile_display_size={{[16, 16]}} tileset={{{tileset}}} /><TilemapChunkTileData value={{Array.from({{ length: {n} }}, () => ({{ tileset_index: 0 }}))}} /></entity>\n")
    };
    let layered = "Image.load(\"tiles.png\", { array_layout: { RowCount: { rows: 4 } } })";
    let e = refused("two-d-tilemap-count", &chunk(layered, 15));
    assert!(e.contains("15") && e.contains("16") && e.contains("chunk_size"), "{e}");
    for (name, tileset) in [("two-d-tileset-path", "\"tiles.png\""), ("two-d-tileset-load", "Image.load(\"tiles.png\")")] {
        let e = refused(name, &chunk(tileset, 16));
        assert!(e.contains("tileset") && e.contains("array_layout"), "{tileset}: {e}");
    }
}
