//! `<text>` falls back to the OS's fonts (change text-font-coverage): every cluster of a mixed
//! string lays out with a real glyph, read from parley's layout, not from pixels.

use crate::common;

use bevy::prelude::*;
use bevy::text::{ComputedTextBlock, FontCx};
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_tsx::TsxNode;
use bevy_ts_v8::V8Options;
use common::*;
use parley::fontique::{FallbackKey, Script};
use parley::PositionedLayoutItem;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// DejaVu Sans cut to the symbols a host's fonts may lack: — → “ ” · ✓ (Bitstream Vera licence,
/// `fixtures/DejaVuSans-LICENSE.txt`).
const SYMBOLS_FONT: &[u8] = include_bytes!("fixtures/DejaVuSans-symbols-subset.ttf");

/// The characters `SYMBOLS_FONT` carries: the sample's must draw from it, whatever the host has.
const FIXTURE_CHARS: &str = "—→“”·✓";

/// ASCII, punctuation FiraMono lacks, CJK, an emoji, and → and ✓ inside Latin runs. (A ✓ straight
/// after the CJK or emoji would draw from their font, which is real too, but the host's.)
const SAMPLE: &str = "ASCII — → “quoted” · 中文 😀 done ✓ → next";

/// The engine's headless plugins and a TSX game holding one `<text>`, laid out with ASCII.
fn text_app(name: &str) -> App {
    let src = "import { app } from \"bevy-ts\";\napp.scene(\"s\", () => <text key=\"t\">ASCII</text>);\n";
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = App::new();
    app.add_plugins(bevy_ts_cli::headless::default_plugins(true).set(bevy_ts_cli::headless::asset_plugin_unwatched(&dir)))
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    set(&mut app, "ASCII");
    app.insert_resource(dir);
    app
}

/// Registers `SYMBOLS_FONT` in the app's font collection (`bevy_text::FontCx`, a parley
/// `FontContext`) as the only fallback for Latin and Common text, so the symbols never come from the
/// host's fonts. CJK and emoji keep the host's fallback.
fn use_symbols_font(app: &mut App) {
    let mut cx = app.world_mut().resource_mut::<FontCx>();
    let families: Vec<_> = cx.collection.register_fonts(SYMBOLS_FONT.to_vec().into(), None).into_iter().map(|(id, _)| id).collect();
    assert!(!families.is_empty(), "the fixture font registers no family");
    for script in ["Latn", "Zyyy", "Zinh"] {
        cx.collection.set_fallbacks(FallbackKey::new(Script::from_str_unchecked(script), None), families.iter().copied());
    }
}

/// Writes the `<text>`'s string and steps until it is laid out again: the first time a string
/// needs a fallback font, that includes fontique's scan of the OS's fonts.
fn set(app: &mut App, text: &str) -> Duration {
    let world = app.world_mut();
    let e = world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == ["t"]).map(|(e, _)| e).expect("the <text>");
    world.get_mut::<Text>(e).unwrap().0 = text.into();
    let start = Instant::now();
    for _ in 0..20 {
        app.update();
        if block(app).and_then(|b| b.buffer().lines().last().map(|l| l.text_range().end)) == Some(text.len()) {
            return start.elapsed();
        }
    }
    panic!("the <text> never laid out {text:?}");
}

fn block(app: &mut App) -> Option<ComputedTextBlock> {
    let world = app.world_mut();
    world.query::<(&TsxNode, &ComputedTextBlock)>().iter(world).find(|(n, _)| n.key_path == ["t"]).map(|(_, b)| b.clone())
}

/// Each cluster's text, whether any of its glyphs is `.notdef`, and whether it drew from `SYMBOLS_FONT`.
fn clusters(app: &mut App, text: &str) -> Vec<(String, bool, bool)> {
    let block = block(app).expect("a laid-out <text>");
    let mut out = Vec::new();
    for line in block.buffer().lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else { continue };
            for cluster in run.run().clusters() {
                let s = text[cluster.text_range()].to_string();
                if s.trim().is_empty() {
                    continue;
                }
                let fixture = run.run().font().data.as_ref() == SYMBOLS_FONT;
                out.push((s, cluster.glyphs().any(|g| g.id == 0), fixture));
            }
        }
    }
    out
}

#[test]
fn mixed_text_draws_without_boxes() {
    let mut app = text_app("text-fonts-mixed");
    use_symbols_font(&mut app);
    let first = set(&mut app, SAMPLE);
    eprintln!("first layout with a fallback: {:.1} ms", first.as_secs_f64() * 1e3);
    let all = clusters(&mut app, SAMPLE);
    let again = set(&mut app, "again — 中文 😀");
    eprintln!("a later layout with a fallback: {:.1} ms", again.as_secs_f64() * 1e3);
    assert!(all.len() > 20, "{all:?}");
    let missing: Vec<String> = all.iter().filter(|(_, notdef, _)| *notdef).map(|(s, _, _)| format!("{s:?} (U+{:04X})", s.chars().next().unwrap() as u32)).collect();
    let hint = if cfg!(target_os = "linux") { "; on Linux, install fonts-noto-cjk and fonts-noto-color-emoji" } else { "" };
    assert!(missing.is_empty(), "drawn as .notdef (a box): {}{hint}", missing.join(", "));
    let elsewhere: Vec<&str> = all.iter().filter(|(s, _, fixture)| !fixture && s.chars().all(|c| FIXTURE_CHARS.contains(c))).map(|(s, _, _)| s.as_str()).collect();
    assert!(elsewhere.is_empty(), "drawn from a host font, not the fixture: {elsewhere:?}");
    assert!(all.iter().any(|(s, _, fixture)| s == "✓" && *fixture), "{all:?}");
}

#[test]
fn a_character_no_font_has_draws_a_box_and_the_rest_lays_out() {
    // U+10FFFD, plane 16 private use: no installed font covers it.
    let text = "ok \u{10FFFD} ok";
    let mut app = text_app("text-fonts-none");
    set(&mut app, text);
    let all = clusters(&mut app, text);
    let boxes: Vec<&str> = all.iter().filter(|(_, notdef, _)| *notdef).map(|(s, _, _)| s.as_str()).collect();
    assert_eq!(boxes, ["\u{10FFFD}"], "{all:?}");
    assert_eq!(all.iter().filter(|(s, _, _)| s == "o" || s == "k").count(), 4, "{all:?}");
}
