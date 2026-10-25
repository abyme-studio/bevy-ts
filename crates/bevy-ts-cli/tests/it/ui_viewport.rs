//! `<viewport>` under `--headless`: every camera renders to the offscreen image that stands for
//! the window, so a viewport's camera must leave it while the game camera stays, and the
//! screenshot image keeps its size. On the GPU, as `bevy-ts dev --headless` builds the app.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::ui::widget::ViewportNode;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{self, Offscreen, SIZE};
use bevy_ts_tsx::TsxNode;
use common::*;
use std::sync::Arc;

const GAME: &str = r##"import { app, px, ref } from "bevy-ts";
import { Camera3d } from "bevy-ts/bevy";
const mini = ref();
app.scene("ui", () => (
  <>
    <entity key="game"><Camera3d /></entity>
    <entity key="mini" ref={mini}><Camera3d /></entity>
    <viewport key="map" camera={mini} style={{ width: px(200), height: px(120) }} />
  </>
));
"##;

fn node(app: &mut App, key: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == [key]).map(|(e, _)| e).unwrap()
}

#[test]
fn the_game_camera_stays_on_the_screenshot_after_a_viewport_mounts() {
    let _hang = common::watchdog("the_game_camera_stays_on_the_screenshot_after_a_viewport_mounts", common::BOUND);
    let dir = project("ui-viewport-headless", &[("game.tsx", GAME), ("package.json", "{}")]);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    let mut app = App::new();
    app.add_plugins(headless::default_plugins(true).set(headless::asset_plugin_unwatched(&dir.join("assets"))))
        .add_plugins(headless::HeadlessPlugin)
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 10);

    let offscreen = app.world().resource::<Offscreen>().0.clone();
    assert_eq!(app.world().resource::<bevy_ts::WindowImage>().0, offscreen, "headless names its window image");
    let (game, mini, map) = (node(&mut app, "game"), node(&mut app, "mini"), node(&mut app, "map"));
    assert_eq!(app.world().get::<RenderTarget>(game).unwrap().as_image(), Some(&offscreen), "the game camera stays on the screenshot");
    let own = app.world().get::<RenderTarget>(mini).unwrap().as_image().cloned().expect("the viewport's camera renders to an image");
    assert_ne!(own, offscreen, "the viewport's camera leaves the screenshot");
    assert_eq!(app.world().get::<ViewportNode>(map).unwrap().camera, Some(mini));
    let images = app.world().resource::<Assets<Image>>();
    let shot = images.get(&offscreen).unwrap().texture_descriptor.size;
    assert_eq!((shot.width, shot.height), SIZE, "the screenshot image keeps its size");
    let size = images.get(&own).unwrap().texture_descriptor.size;
    assert_eq!((size.width, size.height), (200, 120), "Bevy sizes the viewport's image to the node");
}
