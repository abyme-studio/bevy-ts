//! Stage 7 of bevy-ui-coverage: `camera` on a root UI element, `<viewport camera>`, and
//! `<image>`'s flip, rect, atlas and mode. Each writes its own component.

use crate::common;

use bevy::camera::RenderTarget;
use bevy::image::TextureAtlasLayout;
use bevy::prelude::*;
use bevy::sprite::{BorderRect, SliceScaleMode};
use bevy::ui::widget::{NodeImageMode, ViewportNode};
use bevy::ui::UiTargetCamera;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::TsxNode;
use common::*;
use std::path::PathBuf;

const GAME: &str = r##"import { app, px, ref, sliced, tiled } from "bevy-ts";
import { Camera3d } from "bevy-ts/bevy";

const mini = ref();
const main = ref();

app.scene("ui", () => (
  <>
    <entity key="mini" ref={mini}><Camera3d /></entity>
    <entity key="main" ref={main}><Camera3d /></entity>
    <node key="hud" camera={main} style={{ width: px(100) }}>
      <viewport key="map" camera={mini} style={{ width: px(200), height: px(120) }} />
    </node>
    <image key="flipped" src="ui/a.png" flipX flipY rect={[2, 4, 16, 8]} color="#ff0000" />
    <image key="tile0" src="ui/a.png" atlas={{ layout: { tile: [16, 16], columns: 4, rows: 2 }, index: 5 }} />
    <image key="tile1" src="ui/a.png" atlas={{ layout: { tile: [16, 16], columns: 4, rows: 2 }, index: 6 }} />
    <image key="panel" src="ui/a.png" mode={sliced({ border: 8, center: { tile: 2 }, maxCornerScale: 0.5 })} />
    <image key="sides" src="ui/a.png" mode={sliced({ border: { left: 1, right: 2, top: 3, bottom: 4 } })} />
    <image key="floor" src="ui/a.png" mode={tiled({ y: false, stretch: 1.5 })} />
    <image key="stretched" src="ui/a.png" mode="stretch" />
  </>
));
"##;

fn media_app(entry: &std::path::Path) -> App {
    let mut app = app(entry);
    app.add_plugins(AssetPlugin::default()).init_asset::<Image>().init_asset::<TextureAtlasLayout>();
    // A viewport's `PointerId` registers in picking's map, as in a game.
    app.init_resource::<bevy::picking::pointer::PointerMap>();
    app
}

fn setup(name: &str, src: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = media_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");
    (dir, app)
}

fn node(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {path:?}"))
}

fn image(app: &mut App, key: &str) -> ImageNode {
    let e = node(app, &[key]);
    app.world().get::<ImageNode>(e).unwrap().clone()
}

#[test]
fn camera_on_a_root_writes_ui_target_camera_once_the_ref_mounts() {
    let _hang = common::watchdog("camera_on_a_root_writes_ui_target_camera_once_the_ref_mounts", common::BOUND);
    let (_dir, mut app) = setup("ui-media-camera", GAME);
    let (hud, main) = (node(&mut app, &["hud"]), node(&mut app, &["main"]));
    assert_eq!(app.world().get::<UiTargetCamera>(hud).map(|c| c.0), Some(main));
}

#[test]
fn a_viewport_shows_its_camera_on_an_image_of_its_own() {
    let _hang = common::watchdog("a_viewport_shows_its_camera_on_an_image_of_its_own", common::BOUND);
    let (_dir, mut app) = setup("ui-media-viewport", GAME);
    let (map, mini) = (node(&mut app, &["hud", "map"]), node(&mut app, &["mini"]));
    assert_eq!(app.world().get::<ViewportNode>(map).unwrap().camera, Some(mini));
    assert!(app.world().get::<Node>(map).is_some(), "a viewport is a node");
    let target = app.world().get::<RenderTarget>(mini).unwrap().as_image().cloned().expect("the camera renders to an image");
    let usage = app.world().resource::<Assets<Image>>().get(&target).unwrap().texture_descriptor.usage;
    assert!(usage.contains(bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT));
    let main = node(&mut app, &["main"]);
    assert!(app.world().get::<RenderTarget>(main).and_then(RenderTarget::as_image).is_none(), "the other camera stays on the window");
}

#[test]
fn a_camera_already_on_an_image_keeps_it() {
    let _hang = common::watchdog("a_camera_already_on_an_image_keeps_it", common::BOUND);
    let src = r##"import { app, ref } from "bevy-ts";
import { Camera3d } from "bevy-ts/bevy";
const mini = ref();
app.scene("ui", () => (
  <>
    <entity key="mini" ref={mini}><Camera3d /></entity>
    <viewport key="map" camera={mini} />
  </>
));
"##;
    let dir = project("ui-media-own-image", &[("game.tsx", src)]);
    let mut app = media_app(&dir.join("game.tsx"));
    // The game's own image, set before the viewport mounts: the camera's spawn puts it on.
    let own = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    let target = own.clone();
    app.add_observer(move |add: On<Add<Camera>>, mut commands: Commands| {
        commands.entity(add.entity).insert(RenderTarget::Image(target.clone().into()));
    });
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 3);
    let mini = node(&mut app, &["mini"]);
    assert_eq!(app.world().get::<RenderTarget>(mini).unwrap().as_image(), Some(&own));
}

#[test]
fn a_camera_on_the_window_image_of_a_windowless_run_gets_its_own() {
    let _hang = common::watchdog("a_camera_on_the_window_image_of_a_windowless_run_gets_its_own", common::BOUND);
    let src = r##"import { app, ref } from "bevy-ts";
import { Camera3d } from "bevy-ts/bevy";
const mini = ref();
app.scene("ui", () => (
  <>
    <entity key="game"><Camera3d /></entity>
    <entity key="mini" ref={mini}><Camera3d /></entity>
    <viewport key="map" camera={mini} />
  </>
));
"##;
    let dir = project("ui-media-window-image", &[("game.tsx", src)]);
    let mut app = media_app(&dir.join("game.tsx"));
    // As --headless does: every camera renders to the one image that stands for the window.
    let window = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    app.insert_resource(bevy_ts::WindowImage(window.clone()));
    let target = window.clone();
    app.add_observer(move |add: On<Add<Camera>>, mut commands: Commands| {
        commands.entity(add.entity).insert(RenderTarget::Image(target.clone().into()));
    });
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 3);
    let (game, mini) = (node(&mut app, &["game"]), node(&mut app, &["mini"]));
    assert_eq!(app.world().get::<RenderTarget>(game).unwrap().as_image(), Some(&window), "the game camera stays on the window");
    let own = app.world().get::<RenderTarget>(mini).unwrap().as_image().cloned().unwrap();
    assert_ne!(own, window, "the viewport's camera leaves the window");
}

#[test]
fn image_flip_rect_and_tint() {
    let _hang = common::watchdog("image_flip_rect_and_tint", common::BOUND);
    let (_dir, mut app) = setup("ui-media-flip", GAME);
    let i = image(&mut app, "flipped");
    assert!(i.flip_x && i.flip_y);
    assert_eq!(i.rect, Some(Rect::new(2.0, 4.0, 18.0, 12.0)));
    assert_eq!(i.color, Color::from(Srgba::hex("#ff0000").unwrap()));
    assert_eq!(i.image_mode, NodeImageMode::Auto);
}

#[test]
fn equal_atlas_grids_share_one_layout() {
    let _hang = common::watchdog("equal_atlas_grids_share_one_layout", common::BOUND);
    let (_dir, mut app) = setup("ui-media-atlas", GAME);
    let (a, b) = (image(&mut app, "tile0").texture_atlas.unwrap(), image(&mut app, "tile1").texture_atlas.unwrap());
    assert_eq!((a.index, b.index), (5, 6));
    assert_eq!(a.layout, b.layout, "one layout for one grid");
    let layout = app.world().resource::<Assets<TextureAtlasLayout>>().get(&a.layout).unwrap();
    assert_eq!(layout.textures.len(), 8);
    assert_eq!(layout.textures[5], URect::new(16, 16, 32, 32));
}

#[test]
fn sliced_tiled_and_stretch_modes() {
    let _hang = common::watchdog("sliced_tiled_and_stretch_modes", common::BOUND);
    let (_dir, mut app) = setup("ui-media-modes", GAME);
    let NodeImageMode::Sliced(s) = image(&mut app, "panel").image_mode else { panic!("not sliced") };
    assert_eq!(s.border, BorderRect::all(8.0));
    assert_eq!(s.center_scale_mode, SliceScaleMode::Tile { stretch_value: 2.0 });
    assert_eq!(s.sides_scale_mode, SliceScaleMode::Stretch);
    assert_eq!(s.max_corner_scale, 0.5);
    let NodeImageMode::Sliced(s) = image(&mut app, "sides").image_mode else { panic!("not sliced") };
    assert_eq!(s.border, BorderRect { min_inset: Vec2::new(1.0, 3.0), max_inset: Vec2::new(2.0, 4.0) });
    assert_eq!(image(&mut app, "floor").image_mode, NodeImageMode::Tiled { tile_x: true, tile_y: false, stretch_value: 1.5 });
    assert_eq!(image(&mut app, "stretched").image_mode, NodeImageMode::Stretch);
}

#[test]
fn camera_inside_another_ui_element_fails_naming_its_location() {
    let _hang = common::watchdog("camera_inside_another_ui_element_fails_naming_its_location", common::BOUND);
    let src = "import { app, ref } from \"bevy-ts\";\nconst c = ref();\napp.scene(\"ui\", () => (\n  <node>\n    <node camera={c} />\n  </node>\n));\n";
    let dir = project("ui-media-nested-camera", &[("game.tsx", src)]);
    let mut app = media_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = r.errors.iter().find(|e| e.message.contains("camera")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(5), "{e:?}");
}

#[test]
fn an_atlas_index_past_its_grid_fails() {
    let _hang = common::watchdog("an_atlas_index_past_its_grid_fails", common::BOUND);
    let src = "import { app } from \"bevy-ts\";\napp.scene(\"ui\", () => <image src=\"a.png\" atlas={{ layout: { tile: [8, 8], columns: 2, rows: 2 }, index: 4 }} />);\n";
    let dir = project("ui-media-atlas-index", &[("game.tsx", src)]);
    let mut app = media_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors.iter().any(|e| e.message.contains("past the layout's 4 tiles")), "{:?}", r.errors);
}

#[test]
fn camera_inside_show_inside_a_ui_element_fails_too() {
    let _hang = common::watchdog("camera_inside_show_inside_a_ui_element_fails_too", common::BOUND);
    let src = "import { app, ref, Show } from \"bevy-ts\";\nconst c = ref();\napp.scene(\"ui\", () => (\n  <node>\n    <Show when={true}>\n      <node camera={c} />\n    </Show>\n  </node>\n));\n";
    let dir = project("ui-media-show-camera", &[("game.tsx", src)]);
    let mut app = media_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    updates(&mut app, 2);
    let mut errors: Vec<String> = r.errors.iter().map(|e| e.message.clone()).collect();
    errors.extend(logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message));
    assert!(errors.iter().any(|m| m.contains("camera goes on a root UI element")), "{errors:?}");
}

#[test]
fn a_rect_with_a_word_in_it_fails() {
    let _hang = common::watchdog("a_rect_with_a_word_in_it_fails", common::BOUND);
    let src = "import { app } from \"bevy-ts\";\napp.scene(\"ui\", () => <image src=\"a.png\" rect={[0, \"x\", 1, 2, 3] as never} />);\n";
    let dir = project("ui-media-rect", &[("game.tsx", src)]);
    let mut app = media_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors.iter().any(|e| e.message.contains("rect takes")), "{:?}", r.errors);
}
