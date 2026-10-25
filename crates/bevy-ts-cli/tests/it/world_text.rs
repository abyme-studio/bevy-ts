//! `<worldText>` through V8 (change `world-text`): the element mounts world text's components, a
//! binding rewrites only `WorldText`, a refused prop fails the load at its line, and the engine's
//! fonts fall back for every cluster. Layout and drawing are bevy-ts-material's tests.

use crate::common;

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::text::ComputedTextBlock;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_material::{ComputedWorldText, WorldText, WorldTextPlugin, WorldTextStyle};
use bevy_ts_tsx::TsxNode;
use bevy_ts_v8::V8Options;
use common::*;
use parley::PositionedLayoutItem;
use std::path::Path;
use std::sync::Arc;

const GAME: &str = r##"import { app, Component, u8, signal, Update, type Query } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export class Hit extends Component { static path = "game::Hit"; declare on: u8 }
export const [name, setName] = signal("Paris");
export function rename(q: Query<[Hit]>) {
  q.each((h) => { if (h.on === 1) setName("Rome"); });
}
app.addSystems(Update, rename);

app.scene("s", () => (
  <entity key="city">
    <Transform translation={[0, 2, 0]} />
    <worldText size={0.5} outline={{ width: 0.1, color: "#000" }}>{() => name()}</worldText>
  </entity>
));
"##;

/// The engine's headless plugins with no GPU, TSX, and world text, loading `game.tsx` from `dir`.
fn engine(dir: &Path) -> App {
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    let mut app = App::new();
    app.add_plugins(bevy_ts_cli::headless::default_plugins(true).set(render).set(bevy_ts_cli::headless::asset_plugin_unwatched(dir)))
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(WorldTextPlugin)
        .insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    app
}

fn city(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["city"]).map(|(e, _)| e).expect("the city")
}

fn drawn(app: &mut App, e: Entity) {
    for _ in 0..100 {
        app.update();
        if app.world().get::<Mesh3d>(e).is_some() {
            return;
        }
    }
    panic!("the label never drew");
}

#[test]
fn world_text_mounts_and_a_binding_writes_only_its_text() {
    let dir = project("world-text-mount", &[("game.tsx", GAME)]);
    let mut app = engine(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let e = city(&mut app);
    drawn(&mut app, e);
    let w = app.world();
    assert_eq!(w.get::<WorldText>(e).unwrap().0, "Paris");
    let style = w.get::<WorldTextStyle>(e).unwrap();
    assert_eq!((style.size, style.outline.map(|(w, _)| w)), (0.5, Some(0.1)));
    assert_eq!(w.get::<Transform>(e).unwrap().translation, Vec3::new(0.0, 2.0, 0.0));
    assert_eq!(w.get::<ComputedWorldText>(e).unwrap().quads, 5);

    let ticks = |app: &App| {
        let e = app.world().entity(e);
        (e.get_ref::<WorldTextStyle>().unwrap().last_changed(), e.get_ref::<TextFont>().unwrap().last_changed(), e.get_ref::<TextColor>().unwrap().last_changed())
    };
    let before = ticks(&app);
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Hit").unwrap();
    let hit = app.world_mut().spawn_empty().id();
    // SAFETY: Hit is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(hit), id, &[1]) };
    updates(&mut app, 3);
    assert_eq!(app.world().get::<WorldText>(e).unwrap().0, "Rome");
    assert_eq!(ticks(&app), before, "only WorldText is written");
    assert_eq!(app.world().get::<ComputedWorldText>(e).unwrap().quads, 4);
}

#[test]
fn a_refused_world_text_prop_fails_the_load_at_its_line() {
    let src = "import { app } from \"bevy-ts\";\napp.scene(\"s\", () => (\n  <entity>\n    <worldText outline={{ width: 0.3, color: \"#000\" }}>A</worldText>\n  </entity>\n));\n";
    let dir = project("world-text-refused", &[("game.tsx", src)]);
    let mut app = engine(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = r.errors.iter().find(|e| e.message.contains("<worldText>")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert!(e.message.contains("outline.width") && e.message.contains("0 to 0.125"), "{e:?}");
    assert_eq!(e.location.as_ref().map(|l| (l.file.ends_with("game.tsx"), l.line)), Some((true, 4)), "{e:?}");
}

#[test]
fn world_text_beside_a_mesh_fails_naming_both() {
    let src = "import { app } from \"bevy-ts\";\napp.scene(\"s\", () => (\n  <entity>\n    <mesh shape=\"cuboid\" />\n    <worldText>A</worldText>\n  </entity>\n));\n";
    let dir = project("world-text-mesh", &[("game.tsx", src)]);
    let mut app = engine(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = r.errors.iter().find(|e| e.message.contains("<worldText>")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert!(e.message.contains("<mesh>"), "{e:?}");
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(3), "the entity's line: {e:?}");
}

/// The fonts and fallback `<text>` has: every cluster of `Paris 中文 →` has a real glyph.
#[test]
fn world_text_falls_back_to_the_os_fonts() {
    let src = "import { app } from \"bevy-ts\";\napp.scene(\"s\", () => <entity key=\"city\"><worldText>Paris 中文 →</worldText></entity>);\n";
    let dir = project("world-text-fallback", &[("game.tsx", src)]);
    let mut app = engine(&dir);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let e = city(&mut app);
    drawn(&mut app, e);
    let text = "Paris 中文 →";
    let block = app.world().get::<ComputedTextBlock>(e).unwrap();
    let mut clusters = 0;
    for line in block.buffer().lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else { continue };
            for c in run.run().clusters() {
                if text[c.text_range()].trim().is_empty() {
                    continue;
                }
                clusters += 1;
                assert!(c.glyphs().all(|g| g.id != 0), "{:?} drew .notdef", &text[c.text_range()]);
            }
        }
    }
    assert_eq!(clusters, 8);
}
