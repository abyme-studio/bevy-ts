//! Bitmap text (change `pixel-art-2d`, stage 5): an XNA SpriteFont loads from its JSON and PNG, and
//! a label lays out as MonoGame does. `tests/assets/fonts/ab.*` is a two-glyph font: `A` (bearings
//! 1, 5, 1; cropping x 0) and `B` (bearings 0, 4, 1; cropping x 1), spacing 1, line spacing 6.

use bevy::asset::LoadState;
use bevy::asset::io::{AssetSource, AssetSourceBuilder};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts_material::bitmap_text::{BitmapText, BitmapTextLaid, BitmapTextPlugin, Glyph, SpriteFont, layout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const STARDEW: &str = "dev/assets/stardew";

fn stardew() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("HOME")?).join(STARDEW);
    dir.join("Fonts/SmallFont.json").is_file().then_some(dir)
}

fn app() -> App {
    let mut app = App::new();
    if let Some(dir) = stardew() {
        let path = dir.to_string_lossy().into_owned();
        app.register_asset_source("stardew", AssetSourceBuilder::new(AssetSource::get_default_reader(path)));
    }
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin { file_path: assets.to_string_lossy().into_owned(), watch_for_changes_override: Some(false), ..default() },
        ImagePlugin::default(),
        bevy::transform::TransformPlugin,
        BitmapTextPlugin,
    ))
    .init_resource::<ScriptLog>();
    app
}

/// Loads `path` as a font and waits for it to settle.
fn load(app: &mut App, path: &str) -> (Handle<SpriteFont>, LoadState) {
    let h: Handle<SpriteFont> = app.world().resource::<AssetServer>().load(path.to_string());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.update();
        let state = app.world().resource::<AssetServer>().load_state(&h);
        if matches!(state, LoadState::Loaded | LoadState::Failed(_)) {
            return (h, state);
        }
        assert!(Instant::now() < deadline, "{path} never settled");
    }
}

fn font(app: &mut App) -> (Handle<SpriteFont>, SpriteFont) {
    let (h, state) = load(app, "fonts/ab.json");
    assert!(matches!(state, LoadState::Loaded), "{state:?}");
    let f = app.world().resource::<Assets<SpriteFont>>().get(&h).unwrap().clone();
    (h, f)
}

#[test]
fn ab_lays_out_at_x_1_and_9_with_its_box_and_newlines() {
    let mut app = app();
    let (_, f) = font(&mut app);
    let l = layout(&f, "AB");
    let xs: Vec<f32> = l.glyphs.iter().map(|g| g.at.x).collect();
    // A at its left bearing 1; B at 1 + 5 + 1 + 1 + 0, plus its cropping x 1.
    assert_eq!(xs, [1.0, 9.0]);
    assert_eq!(l.size, Vec2::new(1.0 + 5.0 + 1.0 + 1.0 + 4.0 + 1.0, 6.0), "MeasureString: width with the last right bearing, one line");
    let l = layout(&f, "A\r\nB");
    assert_eq!(l.glyphs.iter().map(|g| g.at).collect::<Vec<_>>(), [Vec2::new(1.0, 0.0), Vec2::new(1.0, 6.0)], "B starts the next line, 6 lower");
    assert_eq!(l.size.y, 12.0);
}

#[test]
fn a_character_the_font_lacks_draws_the_default_or_is_skipped() {
    let mut app = app();
    let (_, f) = font(&mut app);
    let l = layout(&f, "AzB");
    assert_eq!(l.missing, ['z'], "no defaultCharacter: skipped");
    assert_eq!(l.glyphs.len(), 2);
    let json = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets/fonts/ab.json")).unwrap();
    let with_default = json.replace("\"defaultCharacter\": null", "\"defaultCharacter\": \"B\"");
    let f = SpriteFont::from_json(with_default.as_bytes(), Handle::default()).unwrap();
    let l = layout(&f, "Az");
    assert!(l.missing.is_empty());
    assert_eq!(l.glyphs.iter().map(|g| g.glyph).collect::<Vec<_>>(), [0, 1], "z draws B");
}

#[test]
fn a_broken_font_fails_naming_its_field_and_a_missing_texture_its_path() {
    let mut app = app();
    let (_, state) = load(&mut app, "fonts/broken.json");
    let LoadState::Failed(e) = state else { panic!("{state:?}") };
    let e = e.to_string();
    assert!(e.contains("cropping has 1 entries, glyphs 2"), "{e}");
    let (_, state) = load(&mut app, "fonts/no-texture.json");
    let LoadState::Failed(e) = state else { panic!("{state:?}") };
    assert!(e.to_string().contains("fonts/gone.png"), "{e}");
}

fn glyphs(app: &mut App, e: Entity) -> Vec<(Vec2, Rect)> {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&ChildOf, &Transform, &Sprite), With<Glyph>>();
    let mut out: Vec<(Vec2, Rect)> = q.iter(world).filter(|(c, ..)| c.parent() == e).map(|(_, t, s)| (t.translation.truncate(), s.rect.unwrap())).collect();
    out.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
    out
}

#[test]
fn a_label_is_one_sprite_per_glyph_laid_again_on_change_and_gone_with_it() {
    let mut app = app();
    let (h, _) = font(&mut app);
    let e = app.world_mut().spawn((BitmapText { text: "AB".into(), font: h, ..default() }, Anchor::TOP_LEFT)).id();
    app.update();
    let g = glyphs(&mut app, e);
    assert_eq!(g.len(), 2);
    // Each glyph's centre: its corner (x 1 and 9, y 0, y down) plus half its size, y up.
    assert_eq!(g[0], (Vec2::new(1.0 + 2.5, -2.5), Rect::new(0.0, 0.0, 5.0, 5.0)));
    assert_eq!(g[1], (Vec2::new(9.0 + 2.0, -2.5), Rect::new(5.0, 0.0, 9.0, 5.0)));
    assert_eq!(app.world().get::<BitmapTextLaid>(e).unwrap().glyphs, 2);

    let mut label = app.world_mut().get_mut::<BitmapText>(e).unwrap();
    label.text = "ABA".into();
    label.scale = 2.0;
    app.update();
    let g = glyphs(&mut app, e);
    assert_eq!(g.len(), 3, "the old glyphs go, the new come");
    assert_eq!(g[0].0, Vec2::new(1.0 + 2.5, -2.5) * 2.0, "scale multiplies positions");
    // The centre anchor puts the measured box's middle at the origin.
    app.world_mut().entity_mut(e).insert(Anchor::CENTER);
    app.update();
    // "ABA" is 21 by 6 font pixels: its middle, rounded down to (10, 3), times scale 2.
    assert_eq!(app.world().get::<BitmapTextLaid>(e).unwrap().size, Vec2::new(42.0, 12.0));
    assert_eq!(glyphs(&mut app, e)[0].0, Vec2::new(1.0 + 2.5, -2.5) * 2.0 - Vec2::new(10.0, -3.0) * 2.0);
    // At scale 1 half of 21 is 10.5, rounded down to 10, so A's left edge stays on a whole pixel.
    app.world_mut().get_mut::<BitmapText>(e).unwrap().scale = 1.0;
    app.update();
    let size = app.world().get::<BitmapTextLaid>(e).unwrap().size;
    assert_eq!(size.x % 2.0, 1.0, "an odd-sized box");
    let (at, rect) = glyphs(&mut app, e)[0];
    let left = at.x - rect.width() / 2.0;
    assert_eq!((left, left.fract()), (1.0 - (size.x / 2.0).floor(), 0.0), "whole pixels under the centre anchor");

    app.world_mut().entity_mut(e).remove::<BitmapText>();
    app.update();
    assert!(glyphs(&mut app, e).is_empty(), "the glyphs go with BitmapText");
}

#[test]
fn a_missing_character_with_no_default_is_named_once() {
    let mut app = app();
    let (h, _) = font(&mut app);
    for _ in 0..2 {
        app.world_mut().spawn(BitmapText { text: "AzAz".into(), font: h.clone(), ..default() });
        app.update();
    }
    let warnings: Vec<String> = app.world().resource::<ScriptLog>().since(0).filter(|e| e.level == LogLevel::Warn).map(|e| e.message.clone()).collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("'z'"), "{}", warnings[0]);
}

#[test]
fn stardews_small_font_loads_with_its_counts() {
    if stardew().is_none() {
        println!("skipped: no ~/{STARDEW} on this machine");
        return;
    }
    let mut app = app();
    let (h, state) = load(&mut app, "stardew://Fonts/SmallFont.json");
    assert!(matches!(state, LoadState::Loaded), "{state:?}");
    let fonts = app.world().resource::<Assets<SpriteFont>>();
    let f = fonts.get(&h).unwrap();
    assert_eq!(f.glyphs.len(), f.characters.len());
    assert_eq!(f.glyphs.len(), 266);
    assert_eq!((f.line_spacing, f.spacing, f.default_character), (33.0, -1.0, Some('*')));
    assert_eq!(f.texture.path().unwrap().to_string(), "stardew://Fonts/SmallFont.png");
    // A character it lacks draws its default.
    let star = f.characters.iter().position(|c| *c == '*').unwrap();
    assert_eq!(layout(f, "中").glyphs.iter().map(|g| g.glyph).collect::<Vec<_>>(), [star]);
}
