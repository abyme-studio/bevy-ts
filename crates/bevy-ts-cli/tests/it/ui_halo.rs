//! Stage 3a of bevy-ui-coverage: `textHalo` on `<text>`, bevy-ts's `TextHalo`, drawn by bevy-ts's
//! own UI extraction as the glyphs again around the text. The component tests run headless; the
//! drawn halo needs the offscreen image, on macOS (Metal), as `headless_ui.rs`.

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::TsxNode;
use bevy_ts_tsx::halo::TextHalo;
use common::*;
use std::path::PathBuf;

/// A text whose halo follows `Phase`: 0 black 1.5 px, 1 red 3 px, 2 none.
const GAME: &str = r##"import { app, Component, u8, signal, px, Update, type Query } from "bevy-ts";

export class Phase extends Component { static path = "game::Phase"; declare v: u8 }
export const [phase, setPhase] = signal(0);
export function follow(q: Query<[Phase]>) {
  q.each((p) => { if (p.v !== phase()) setPhase(p.v); });
}
app.addSystems(Update, follow);

const halo = () => (phase() === 0 ? { width: 1.5, color: "#000000" } : phase() === 1 ? { width: 3, color: "#ff0000" } : undefined);

app.scene("ui", () => (
  <node key="row" style={{ width: px(960), height: px(540) }}>
    <node key="pale" style={{ width: px(480), height: px(540), justifyContent: "center", alignItems: "center" }} background="#e8e4d8">
      <text key="t" size={96} color="#ffffff" textHalo={halo}>Harbour</text>
    </node>
    <node key="dark" style={{ width: px(480), height: px(540), justifyContent: "center", alignItems: "center" }} background="#0b1a2e">
      <text key="t" size={96} color="#ffffff" textHalo={halo}>Harbour</text>
    </node>
    <text key="plain" style={{ display: "none" }} textHalo={{}}>defaults</text>
  </node>
));
"##;

fn node(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {path:?}"))
}

/// Sets the one `Phase` entity's `v`, spawning it the first time.
fn set_phase(app: &mut App, v: u8) {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Phase").unwrap();
    let existing = {
        let world = app.world_mut();
        let mut q = world.query::<EntityRef>();
        q.iter(world).find(|r| r.contains_id(id)).map(|r| r.id())
    };
    if let Some(e) = existing {
        app.world_mut().entity_mut(e).despawn();
    }
    let e = app.world_mut().spawn_empty().id();
    // SAFETY: Phase is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(e), id, &[v]) };
}

fn halo(app: &mut App, path: &[&str]) -> Option<TextHalo> {
    let e = node(app, path);
    app.world().get::<TextHalo>(e).copied()
}

fn failed(name: &str, src: &str) -> Vec<bevy_ts::backend::ScriptError> {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    r.errors
}

#[test]
fn text_halo_is_written_changed_and_removed() {
    let dir: PathBuf = project("ui-halo-component", &[("game.tsx", GAME)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    assert_eq!(halo(&mut app, &["row", "plain"]), Some(TextHalo { width: 1.0, color: Color::BLACK }), "the defaults: 1 px, black");
    let black = TextHalo { width: 1.5, color: Color::from(Srgba::hex("000000").unwrap()) };
    assert_eq!(halo(&mut app, &["row", "pale", "t"]), Some(black));
    set_phase(&mut app, 1);
    updates(&mut app, 2);
    assert_eq!(halo(&mut app, &["row", "pale", "t"]), Some(TextHalo { width: 3.0, color: Color::from(Srgba::hex("ff0000").unwrap()) }));
    set_phase(&mut app, 2);
    updates(&mut app, 2);
    assert_eq!(halo(&mut app, &["row", "pale", "t"]), None, "undefined removes it");
    assert_eq!(halo(&mut app, &["row", "dark", "t"]), None);
}

#[test]
fn a_halo_on_a_span_fails_naming_its_location() {
    let src = "import { app } from \"bevy-ts\";\napp.scene(\"ui\", () => (\n  <text>\n    a <span textHalo={{ width: 1 } as never}>b</span>\n  </text>\n));\n";
    let errors = failed("ui-halo-span", src);
    let e = errors.iter().find(|e| e.message.contains("textHalo")).unwrap_or_else(|| panic!("{errors:?}"));
    assert!(e.message.contains("<span>"), "{e:?}");
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(4), "{e:?}");
}

#[test]
fn a_halo_width_out_of_range_names_text_halo() {
    for (n, w) in [("zero", "0"), ("wide", "5")] {
        let src = format!("import {{ app }} from \"bevy-ts\";\napp.scene(\"ui\", () => <text textHalo={{{{ width: {w} }}}}>x</text>);\n");
        let errors = failed(&format!("ui-halo-{n}"), &src);
        assert!(errors.iter().any(|e| e.message.contains("textHalo.width") && e.message.contains("at most 4")), "{w}: {errors:?}");
    }
    let errors = failed("ui-halo-colour", "import { app } from \"bevy-ts\";\napp.scene(\"ui\", () => <text textHalo={{ color: \"blue-ish\" }}>x</text>);\n");
    assert!(errors.iter().any(|e| e.message.contains("textHalo.color")), "{errors:?}");
}

/// The halo drawn: the offscreen image, its pixels counted by colour in each half.
#[cfg(target_os = "macos")]
mod drawn {
    use super::*;
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
    use bevy::text::TextLayoutInfo;
    use bevy_ts::generation::DevSource;
    use bevy_ts_cli::cli::{Display, Engine, base_app};
    use std::sync::{Arc, Mutex};

    fn scene(name: &str) -> App {
        let dir = project(name, &[("game.tsx", GAME)]);
        let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
        let mut app = base_app(&engine, bevy_ts_cli::Prebuilt, Display::Headless, Default::default(), None, true, Default::default(), &[], false);
        app.insert_resource(DevSource(Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("game.tsx"), true).unwrap())));
        app.finish();
        app.cleanup();
        app.world_mut().spawn(Camera2d);
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        updates(&mut app, 10);
        app
    }

    fn screenshot(app: &mut App) -> Image {
        let shot = Arc::new(Mutex::new(None));
        let slot = shot.clone();
        let image = app.world().resource::<bevy_ts_cli::headless::Offscreen>().0.clone();
        app.world_mut().spawn(Screenshot::image(image)).observe(move |c: On<ScreenshotCaptured>| {
            *slot.lock().unwrap() = Some(c.image.clone());
        });
        // Frames run unpaced headless: the readback can take longer than any count of them.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            app.update();
            if let Some(i) = shot.lock().unwrap().take() {
                // Kept for a failure: the temp folder's `bevy-ts-ui-halo.png` is the last shot.
                let _ = i.clone().try_into_dynamic().map(|d| d.to_rgba8().save(std::env::temp_dir().join("bevy-ts-ui-halo.png")));
                return i;
            }
        }
        panic!("no screenshot within 10 s");
    }

    /// Pixels in columns `x0..x1` matching `want`, as 8-bit sRGB within 6.
    fn count(image: &Image, x0: u32, x1: u32, want: [u8; 3]) -> usize {
        let mut n = 0;
        for y in 0..image.height() {
            for x in x0..x1 {
                let c = image.get_color_at(x, y).unwrap().to_srgba();
                let px = [c.red, c.green, c.blue].map(|v| (v * 255.0).round() as i32);
                if px.iter().zip(want).all(|(a, b)| (a - b as i32).abs() <= 6) {
                    n += 1;
                }
            }
        }
        n
    }

    /// The white glyph pixels with a pixel darker than `ground` within 4 px, out of all white ones
    /// on the glyphs' edge (a white pixel next to a non-white one). White text antialiased into
    /// its ground is never darker than the ground; a dark halo is.
    fn edged(image: &Image, x0: u32, x1: u32, ground: [u8; 3]) -> (usize, usize) {
        let rgb = |x: i64, y: i64| {
            if x < x0 as i64 || x >= x1 as i64 || y < 0 || y >= image.height() as i64 {
                return None;
            }
            let p = image.get_color_at(x as u32, y as u32).unwrap().to_srgba();
            Some([p.red, p.green, p.blue].map(|v| (v * 255.0).round() as i32))
        };
        let is = |x: i64, y: i64, c: [u8; 3]| rgb(x, y).is_some_and(|p| p.iter().zip(c).all(|(a, b)| (a - b as i32).abs() <= 6));
        let floor = ground.iter().map(|&c| c as i32).sum::<i32>() - 12;
        let darker = |x: i64, y: i64| rgb(x, y).is_some_and(|p| p.iter().sum::<i32>() < floor);
        let (mut edges, mut haloed) = (0, 0);
        for y in 0..image.height() as i64 {
            for x in x0 as i64..x1 as i64 {
                if !is(x, y, [255, 255, 255]) {
                    continue;
                }
                if [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().all(|(dx, dy)| is(x + dx, y + dy, [255, 255, 255])) {
                    continue;
                }
                edges += 1;
                if (-4..=4).any(|dy: i64| (-4..=4).any(|dx: i64| dx * dx + dy * dy <= 16 && darker(x + dx, y + dy))) {
                    haloed += 1;
                }
            }
        }
        (haloed, edges)
    }

    #[test]
    fn a_halo_outlines_white_text_on_pale_and_dark_grounds() {
        let mut app = scene("ui-halo-drawn");
        let t = node(&mut app, &["row", "pale", "t"]);
        let layout_tick = app.world().entity(t).get_change_ticks::<TextLayoutInfo>().unwrap().changed;
        let glyphs = app.world().get::<TextLayoutInfo>(t).unwrap().glyphs.len();

        // The font may take a few frames to load: shots until the text is drawn.
        let mut shot = screenshot(&mut app);
        for _ in 0..5 {
            if count(&shot, 0, 480, [255, 255, 255]) > 1000 {
                break;
            }
            shot = screenshot(&mut app);
        }
        for (ground, x0, x1, rgb) in [("pale", 0, 480, [0xe8, 0xe4, 0xd8]), ("dark", 480, 960, [0x0b, 0x1a, 0x2e])] {
            assert!(count(&shot, x0, x1, [255, 255, 255]) > 1000, "{ground}: the glyphs stay white");
            let (haloed, edges) = edged(&shot, x0, x1, rgb);
            assert!(edges > 200 && haloed * 10 >= edges * 9, "{ground}: {haloed} of {edges} edge pixels have the halo just outside");
        }

        // A changed colour shows on the next frame drawn.
        set_phase(&mut app, 1);
        let shot = screenshot(&mut app);
        for (ground, x0, x1) in [("pale", 0, 480), ("dark", 480, 960)] {
            assert!(count(&shot, x0, x1, [255, 0, 0]) > 500, "{ground}: the halo turned red");
            assert_eq!(count(&shot, x0, x1, [0, 0, 0]), 0, "{ground}: no black halo left");
        }

        // Removed: no halo drawn, and the text was not laid out again.
        set_phase(&mut app, 2);
        let shot = screenshot(&mut app);
        for (ground, x0, x1) in [("pale", 0, 480), ("dark", 480, 960)] {
            assert_eq!(count(&shot, x0, x1, [255, 0, 0]), 0, "{ground}: the halo is gone");
            assert!(count(&shot, x0, x1, [255, 255, 255]) > 1000, "{ground}: the text is still drawn");
        }
        assert_eq!(app.world().entity(t).get_change_ticks::<TextLayoutInfo>().unwrap().changed, layout_tick, "TextLayoutInfo recomputed");
        assert_eq!(app.world().get::<TextLayoutInfo>(t).unwrap().glyphs.len(), glyphs);
    }
}
