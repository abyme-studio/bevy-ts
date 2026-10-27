//! World text (change `world-text`), headless: the engine's plugins with no window and no GPU.
//! The distance field, the packer, layout in world units, and what a change writes.

use bevy::asset::AssetEvent;
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::text::{FontSource, LineBreak, TextLayout};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_material::world_text::atlas::{MAX_HEIGHT, Packer};
use bevy_ts_material::world_text::sdf::{SPREAD, sdf};
use bevy_ts_material::world_text::{RASTER, WorldTextAtlas, WorldTextMaterials};
use bevy_ts_material::{ComputedWorldText, LaidOut, WorldText, WorldTextMaterial, WorldTextPlugin, WorldTextStyle};

fn app() -> App {
    let mut app = App::new();
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(render),
    )
    .add_plugins(WorldTextPlugin);
    app.finish();
    app.cleanup();
    app
}

/// Steps until `done`, at most 100 frames: fonts register and assets arrive a frame or two in.
fn until(app: &mut App, what: &str, mut done: impl FnMut(&mut World) -> bool) {
    for _ in 0..100 {
        app.update();
        if done(app.world_mut()) {
            return;
        }
    }
    panic!("never: {what}");
}

fn label(app: &mut App, text: &str, style: WorldTextStyle) -> Entity {
    app.world_mut().spawn((WorldText::new(text), style)).id()
}

fn drawn(world: &mut World, e: Entity) -> bool {
    world.get::<Mesh3d>(e).is_some()
}

/// Each glyph quad of `e`'s mesh, in its local space.
fn quads(world: &World, e: Entity) -> Vec<Rect> {
    let h = &world.get::<Mesh3d>(e).expect("a label's mesh").0;
    let mesh = world.resource::<Assets<Mesh>>().get(h).expect("the mesh, on the CPU with no render world");
    let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap();
    pos.chunks(4).map(|q| q.iter().fold(Rect::EMPTY, |r, p| r.union_point(Vec2::new(p[0], p[1])))).collect()
}

/// A disc of radius `r` centred in an `n` × `n` raster, antialiased by 16 × 16 samples a pixel.
fn disc(n: usize, r: f32) -> Vec<u8> {
    let c = n as f32 / 2.0;
    let mut out = vec![0u8; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut hit = 0;
            for sy in 0..16 {
                for sx in 0..16 {
                    let (px, py) = (x as f32 + (sx as f32 + 0.5) / 16.0, y as f32 + (sy as f32 + 0.5) / 16.0);
                    hit += ((px - c).powi(2) + (py - c).powi(2) <= r * r) as u32;
                }
            }
            out[y * n + x] = (hit * 255 / 256) as u8;
        }
    }
    out
}

#[test]
fn a_discs_distance_field_crosses_half_at_its_radius() {
    let (n, r, s) = (40usize, 12.3f32, SPREAD as usize);
    let f = sdf(&disc(n, r), n, n, s);
    let w = n + 2 * s;
    let centre = w as f32 / 2.0;
    // Along the middle row, from the centre outward: where the field falls through 0.5.
    let row = w / 2;
    let v = |x: usize| f[row * w + x] as f32 / 255.0;
    let x = (w / 2..w - 1).find(|&x| v(x) >= 0.5 && v(x + 1) < 0.5).expect("a crossing");
    let t = (v(x) - 0.5) / (v(x) - v(x + 1));
    // A texel's value is its centre's.
    let crossing = x as f32 + 0.5 + t;
    assert!((crossing - centre - r).abs() <= 0.5, "crosses at {} from the centre, radius {r}", crossing - centre);
    assert_eq!(f[0], 0, "a corner, past the spread, is 0");
    assert!(v(w / 2) > 0.99, "the centre, deeper than the spread, is 1");
}

#[test]
fn the_packer_never_overlaps_and_grows_to_its_limit() {
    let mut p = Packer::new(64, 16, 64);
    let mut cells: Vec<URect> = Vec::new();
    while let Some(at) = p.place(13, 9) {
        let r = URect::from_corners(at, at + UVec2::new(13, 9));
        assert!(r.max.x <= 64 && r.max.y <= p.height, "{r:?} inside {}×{}", p.width, p.height);
        assert!(cells.iter().all(|c| c.intersect(r).is_empty()), "{r:?} overlaps");
        cells.push(r);
    }
    assert_eq!(p.height, 64, "grew to its limit");
    assert_eq!(cells.len(), 4 * 6, "four cells a row, six rows");
    assert!(p.place(65, 1).is_none(), "wider than the atlas");
}

#[test]
fn ab_at_size_2_from_the_bottom_left_starts_at_the_origin() {
    let mut app = app();
    let e = label(&mut app, "AB", WorldTextStyle { size: 2.0, anchor: Vec2::splat(-0.5), ..default() });
    until(&mut app, "AB drawn", |w| drawn(w, e));
    let world = app.world_mut();
    let rect = world.get::<ComputedWorldText>(e).unwrap().rect;
    assert!(rect.min.abs_diff_eq(Vec2::ZERO, 1e-4), "{rect:?}");
    let block = world.get::<bevy::text::ComputedTextBlock>(e).unwrap();
    let layout = block.buffer();
    let m = *layout.lines().next().unwrap().metrics();
    let font = (m.ascent + m.descent) / RASTER * 2.0;
    assert!((rect.height() - font).abs() <= 0.1 * font, "height {} against ascent plus descent {font}", rect.height());
    assert_eq!(world.get::<ComputedWorldText>(e).unwrap().quads, 2);
    // The quads lie inside the box, but for the field's spread around each glyph.
    let pad = SPREAD as f32 / RASTER * 2.0;
    for q in quads(world, e) {
        assert!(q.min.x >= -pad - 1e-3 && q.max.x <= rect.max.x + pad + 1e-3, "{q:?} in {rect:?}");
    }
}

#[test]
fn a_width_wraps() {
    let mut app = app();
    let e = label(&mut app, "the quick brown fox jumps over the lazy dog", WorldTextStyle { size: 1.0, width: Some(4.0), anchor: Vec2::splat(-0.5), ..default() });
    until(&mut app, "the sentence drawn", |w| drawn(w, e));
    let world = app.world_mut();
    let lines = world.get::<bevy::text::ComputedTextBlock>(e).unwrap().buffer().len();
    assert!(lines > 1, "{lines} lines");
    let pad = SPREAD as f32 / RASTER;
    for q in quads(world, e) {
        assert!(q.max.x - pad <= 4.0 + 1e-3, "{q:?} past 4 world units");
    }
}

#[test]
fn a_changed_text_rewrites_its_own_mesh_only() {
    let mut app = app();
    let hp = label(&mut app, "HP 10", WorldTextStyle::default());
    let other = label(&mut app, "MP 3", WorldTextStyle::default());
    until(&mut app, "both drawn", |w| drawn(w, hp) && drawn(w, other));
    let (before, other_mesh) = (app.world().get::<Mesh3d>(hp).unwrap().0.clone(), app.world().get::<Mesh3d>(other).unwrap().0.id());
    let mut cursor = app.world().resource::<Messages<AssetEvent<Mesh>>>().get_cursor();
    app.world_mut().get_mut::<WorldText>(hp).unwrap().0 = "HP 9".into();
    app.update();
    let world = app.world();
    assert_eq!(world.get::<Mesh3d>(hp).unwrap().0, before, "the same handle");
    assert_eq!(world.get::<ComputedWorldText>(hp).unwrap().quads, 3, "H, P and 9");
    assert_eq!(quads(world, hp).len(), 3);
    let modified: Vec<_> = cursor.read(world.resource::<Messages<AssetEvent<Mesh>>>()).filter_map(|e| if let AssetEvent::Modified { id } = e { Some(*id) } else { None }).collect();
    assert!(modified.contains(&before.id()) && !modified.contains(&other_mesh), "{modified:?}");
    assert_eq!(world.resource::<LaidOut>().0, 1);
}

#[test]
fn a_thousand_labels_share_one_atlas_one_material_and_each_glyph_once() {
    let mut app = app();
    let outline = Some((0.1, Color::BLACK));
    let labels: Vec<Entity> = (0..1000)
        .map(|i| {
            let color = if i % 2 == 0 { Color::WHITE } else { Color::srgb(1.0, 0.8, 0.0) };
            app.world_mut().spawn((WorldText::new(format!("label {i}")), WorldTextStyle { outline, ..default() }, TextColor(color))).id()
        })
        .collect();
    until(&mut app, "every label drawn", |w| labels.iter().all(|e| drawn(w, *e)));
    let world = app.world_mut();
    let atlas = world.resource::<WorldTextAtlas>();
    // l, a, b, e and ten digits: a distance field per glyph with ink, the space none.
    assert_eq!(atlas.made, 14);
    // Placed tallest first, they share the first 256 texels.
    assert_eq!(atlas.packer.height, bevy_ts_material::world_text::atlas::START_HEIGHT);
    let r8 = world.resource::<Assets<Image>>().iter().filter(|(_, i)| i.texture_descriptor.format == bevy::render::render_resource::TextureFormat::R8Unorm).count();
    assert_eq!(r8, 1, "one atlas image");
    assert_eq!(world.resource::<WorldTextMaterials>().len(), 1);
    assert_eq!(world.resource::<Assets<WorldTextMaterial>>().len(), 1);
    let m = world.get::<MeshMaterial3d<WorldTextMaterial>>(labels[0]).unwrap().0.clone();
    assert!(labels.iter().all(|e| world.get::<MeshMaterial3d<WorldTextMaterial>>(*e).unwrap().0 == m));
    // A frame with no change lays out none.
    app.update();
    app.update();
    assert_eq!(app.world().resource::<LaidOut>().0, 0);
}

#[test]
fn a_label_spawned_in_rust_has_a_quad_per_letter() {
    let mut app = app();
    let e = app.world_mut().spawn((WorldText::new("Paris"), WorldTextStyle::default(), Transform::default())).id();
    until(&mut app, "Paris drawn", |w| drawn(w, e));
    let world = app.world();
    assert_eq!(quads(world, e).len(), 5);
    assert!(world.get::<MeshMaterial3d<WorldTextMaterial>>(e).is_some());
    assert!(world.get::<bevy::light::NotShadowCaster>(e).is_some());
    assert!(world.get::<bevy::camera::primitives::Aabb>(e).is_some());
}

#[test]
fn a_label_whose_font_loads_late_draws_then() {
    let mut app = app();
    let font: Handle<Font> = app.world().resource::<Assets<Font>>().reserve_handle();
    let e = app.world_mut().spawn((WorldText::new("Late"), TextFont { font: FontSource::Handle(font.clone()), ..default() })).id();
    for _ in 0..5 {
        app.update();
    }
    assert!(!drawn(app.world_mut(), e), "nothing to draw with yet");
    assert_eq!(app.world().resource::<LaidOut>().0, 1, "tried again each frame");
    app.world_mut().resource_mut::<Assets<Font>>().insert(font.id(), Font::from_bytes(bevy::text::DEFAULT_FONT_DATA.to_vec())).unwrap();
    until(&mut app, "Late drawn", |w| drawn(w, e));
    assert_eq!(app.world().get::<ComputedWorldText>(e).unwrap().quads, 4);
}

#[test]
fn a_font_that_fails_to_load_stops_being_retried() {
    let mut app = app();
    let font: Handle<Font> = app.world().resource::<AssetServer>().load("no-such-font.ttf");
    let e = app.world_mut().spawn((WorldText::new("Lost"), TextFont { font: FontSource::Handle(font.clone()), ..default() })).id();
    // The load fails on the IO threads, in its own time: wait for it, not for a number of frames.
    let start = std::time::Instant::now();
    while !app.world().resource::<AssetServer>().load_state(&font).is_failed() {
        assert!(start.elapsed().as_secs() < 30, "the missing font never failed to load");
        app.update();
    }
    app.update();
    app.update();
    for _ in 0..3 {
        app.update();
        assert_eq!(app.world().resource::<LaidOut>().0, 0, "not laid out again");
    }
    assert!(!drawn(app.world_mut(), e));
}

#[test]
fn an_outline_bound_through_50_values_keeps_at_most_two_materials() {
    let mut app = app();
    let e = label(&mut app, "Glow", WorldTextStyle { outline: Some((0.01, Color::BLACK)), ..default() });
    until(&mut app, "Glow drawn", |w| drawn(w, e));
    for i in 1..50 {
        app.world_mut().get_mut::<WorldTextStyle>(e).unwrap().outline = Some((0.01 + i as f32 * 0.002, Color::srgb(i as f32 / 50.0, 0.0, 0.0)));
        app.update();
    }
    // Bevy frees an asset in a later frame than its last handle goes.
    app.update();
    app.update();
    let world = app.world();
    assert!(world.resource::<Assets<WorldTextMaterial>>().len() <= 2, "{} materials", world.resource::<Assets<WorldTextMaterial>>().len());
    assert!(world.resource::<WorldTextMaterials>().len() <= 2, "{} cached styles", world.resource::<WorldTextMaterials>().len());
}

#[test]
fn removing_world_text_drops_its_mesh_and_material() {
    let mut app = app();
    let e = label(&mut app, "A", WorldTextStyle::default());
    until(&mut app, "A drawn", |w| drawn(w, e));
    app.world_mut().entity_mut(e).remove::<WorldText>();
    app.update();
    let world = app.world();
    assert!(world.get::<Mesh3d>(e).is_none() && world.get::<MeshMaterial3d<WorldTextMaterial>>(e).is_none() && world.get::<ComputedWorldText>(e).is_none());
}

#[test]
fn a_growing_atlas_keeps_every_mesh() {
    let mut app = app();
    let small = WorldTextAtlas::new(&mut app.world_mut().resource_mut::<Assets<Image>>(), 32, MAX_HEIGHT);
    app.world_mut().insert_resource(small);
    let first = label(&mut app, "AB", WorldTextStyle::default());
    until(&mut app, "AB drawn", |w| drawn(w, first));
    let (mesh, height) = (app.world().get::<Mesh3d>(first).unwrap().0.id(), app.world().resource::<WorldTextAtlas>().packer.height);
    let mut cursor = app.world().resource::<Messages<AssetEvent<Mesh>>>().get_cursor();
    let second = label(&mut app, "abcdefghijklmnopqrstuvwxyzCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", WorldTextStyle::default());
    until(&mut app, "the alphabet drawn", |w| drawn(w, second));
    let world = app.world();
    assert!(world.resource::<WorldTextAtlas>().packer.height > height, "the atlas grew");
    let atlas = world.resource::<Assets<Image>>().get(&world.resource::<WorldTextAtlas>().image).unwrap();
    assert_eq!(atlas.height(), world.resource::<WorldTextAtlas>().packer.height);
    let modified = cursor.read(world.resource::<Messages<AssetEvent<Mesh>>>()).any(|e| matches!(e, AssetEvent::Modified { id } if *id == mesh));
    assert!(!modified, "the first label's mesh was not rewritten");
    assert_eq!(world.get::<ComputedWorldText>(first).unwrap().quads, 2);
}

/// Past the atlas's limit a glyph logs one error and draws nothing; the rest of its label draws.
#[test]
fn a_full_atlas_refuses_a_glyph_and_draws_the_rest() {
    let mut app = app();
    // 48 texels high: `a`'s field fits, `|`'s, taller, does not.
    let tiny = WorldTextAtlas::new(&mut app.world_mut().resource_mut::<Assets<Image>>(), 32, 48);
    app.world_mut().insert_resource(tiny);
    let e = label(&mut app, "ab|", WorldTextStyle::default());
    until(&mut app, "ab drawn", |w| drawn(w, e));
    let world = app.world();
    let atlas = world.resource::<WorldTextAtlas>();
    let quads = world.get::<ComputedWorldText>(e).unwrap().quads;
    assert!(atlas.refused >= 1 && quads >= 1, "a glyph refused, {quads} drawn");
    assert_eq!(quads as usize, 3 - atlas.refused, "the rest drawn");
    // Asked again, a refused glyph is not made or logged again.
    let refused = atlas.refused;
    app.world_mut().get_mut::<WorldText>(e).unwrap().0 = "ab| ab|".into();
    app.update();
    assert_eq!(app.world().resource::<WorldTextAtlas>().refused, refused);
}

#[test]
fn no_wrap_ignores_the_width() {
    let mut app = app();
    let e = app.world_mut().spawn((WorldText::new("one two three four five"), WorldTextStyle { width: Some(1.0), ..default() }, TextLayout::linebreak(LineBreak::NoWrap))).id();
    until(&mut app, "drawn", |w| drawn(w, e));
    assert_eq!(app.world().get::<bevy::text::ComputedTextBlock>(e).unwrap().buffer().len(), 1);
}

/// Labels spawned in one frame in another order: the same atlas, byte for byte, and each label the
/// same UVs. Glyphs placed in the order labels were laid out moved letters in the atlas, and so
/// their edges under MSAA (learn's two states in agent town, whichever records came first).
#[test]
fn labels_laid_out_in_another_order_make_the_same_atlas() {
    let texts = ["Attention", "Learn", "Rim north", "Walker 7", "Planet", "qjxz QJXZ"];
    let atlas = |order: &[usize]| {
        let mut app = app();
        let labels: Vec<(usize, Entity)> = order.iter().map(|&i| (i, label(&mut app, texts[i], WorldTextStyle { size: 0.4, outline: Some((0.1, Color::BLACK)), ..default() }))).collect();
        until(&mut app, "every label drawn", |w| labels.iter().all(|(_, e)| drawn(w, *e)));
        let world = app.world();
        let image = world.resource::<Assets<Image>>().get(&world.resource::<WorldTextAtlas>().image).unwrap().data.clone().unwrap();
        let mut uvs: Vec<(usize, Vec<[f32; 2]>)> = labels
            .iter()
            .map(|&(i, e)| {
                let mesh = world.resource::<Assets<Mesh>>().get(&world.get::<Mesh3d>(e).unwrap().0).unwrap();
                let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!("UVs") };
                (i, uv.clone())
            })
            .collect();
        uvs.sort_by_key(|(i, _)| *i);
        (image, uvs)
    };
    let (a, a_uvs) = atlas(&[0, 1, 2, 3, 4, 5]);
    let (b, b_uvs) = atlas(&[5, 3, 1, 4, 0, 2]);
    let differ = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    assert!(a.len() == b.len() && differ == 0, "atlases differ: {} and {} bytes, {differ} differ", a.len(), b.len());
    assert_eq!(a_uvs, b_uvs, "a label's UVs differ");
}
