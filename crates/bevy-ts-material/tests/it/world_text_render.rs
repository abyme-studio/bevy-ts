//! World text drawn (change `world-text`), into an offscreen image: in its bounds and not from
//! behind, hidden by what stands in front, crisp close up and steady far away, outlined, lit or
//! unlit. Things are found on screen by projecting where they stand through the camera.

use crate::alpha_render::{GPU, app_with, until_drawn};
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy_ts_material::world_text::sdf::SPREAD;
use bevy_ts_material::world_text::RASTER;
use bevy_ts_material::{ComputedWorldText, WorldText, WorldTextPlugin, WorldTextStyle};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

fn app() -> App {
    app_with(|p| p.add(WorldTextPlugin))
}

/// A camera at `at`, `w` × `h`, drawing over `clear` with `msaa`; the image and the camera.
fn camera(world: &mut World, at: Transform, (w, h): (u32, u32), clear: Color, msaa: Msaa) -> (Handle<Image>, Entity) {
    let mut target = Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    let cam = world
        .spawn((Camera3d::default(), Camera { clear_color: ClearColorConfig::Custom(clear), ..default() }, Tonemapping::None, msaa, RenderTarget::Image(target.clone().into()), at))
        .id();
    (target, cam)
}

/// A screen rectangle, `min` inclusive, `max` exclusive.
#[derive(Clone, Copy, Debug)]
struct Px {
    min: IVec2,
    max: IVec2,
}

impl Px {
    fn grow(self, by: i32) -> Px {
        Px { min: self.min - by, max: self.max + by }
    }

    fn contains(self, p: IVec2) -> bool {
        p.cmpge(self.min).all() && p.cmplt(self.max).all()
    }
}

/// The screen box of `e`'s label, its glyph quads' spread included, at the label's own depth.
fn label_box(world: &World, cam: Entity, e: Entity) -> Result<Px, String> {
    let rect = world.get::<ComputedWorldText>(e).ok_or("not laid out")?.rect;
    let size = world.get::<WorldTextStyle>(e).unwrap().size;
    let rect = rect.inflate(SPREAD as f32 / RASTER * size);
    let t = world.get::<GlobalTransform>(e).ok_or("no transform yet")?;
    project(world, cam, [t.transform_point(rect.min.extend(0.0)), t.transform_point(rect.max.extend(0.0))])
}

fn project(world: &World, cam: Entity, points: [Vec3; 2]) -> Result<Px, String> {
    let (c, t) = (world.get::<Camera>(cam).unwrap(), world.get::<GlobalTransform>(cam).unwrap());
    let p = |v| c.world_to_viewport(t, v).map_err(|e| format!("{e:?}"));
    let (a, b) = (p(points[0])?, p(points[1])?);
    Ok(Px { min: a.min(b).floor().as_ivec2(), max: a.max(b).ceil().as_ivec2() })
}

fn rgb(img: &Image, p: IVec2) -> [u8; 3] {
    let at = ((p.y as u32 * img.width() + p.x as u32) * 4) as usize;
    let d = img.data.as_ref().unwrap();
    [d[at], d[at + 1], d[at + 2]]
}

/// Every pixel of the image with its colour.
fn all(img: &Image) -> impl Iterator<Item = (IVec2, [u8; 3])> + '_ {
    let (w, h) = (img.width() as i32, img.height() as i32);
    (0..h).flat_map(move |y| (0..w).map(move |x| (IVec2::new(x, y), rgb(img, IVec2::new(x, y)))))
}

fn inside(img: &Image, r: Px) -> impl Iterator<Item = [u8; 3]> + '_ {
    all(img).filter(move |(p, _)| r.contains(*p)).map(|(_, c)| c)
}

fn white(p: [u8; 3]) -> bool {
    p.iter().all(|c| *c > 200)
}

fn red(p: [u8; 3]) -> bool {
    p[0] > 200 && p[1] < 60 && p[2] < 60
}

fn black(p: [u8; 3]) -> bool {
    p.iter().all(|c| *c < 40)
}

fn count(it: impl Iterator<Item = [u8; 3]>, f: impl Fn([u8; 3]) -> bool) -> usize {
    it.filter(|p| f(*p)).count()
}

/// One screenshot taken after this call.
fn shot(app: &mut App, target: &Handle<Image>) -> Image {
    let got: Arc<Mutex<Option<Image>>> = Arc::default();
    let g = got.clone();
    app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
        *g.lock().unwrap() = Some(e.image.clone());
    });
    for _ in 0..600 {
        app.update();
        if let Some(img) = got.lock().unwrap().take() {
            return img;
        }
    }
    panic!("no screenshot");
}

fn label(world: &mut World, text: &str, style: WorldTextStyle, at: Transform) -> Entity {
    world.spawn((WorldText::new(text), style, at)).id()
}

/// White on black, with MSAA off and on: white inside the label's box and nowhere else; from
/// behind, nothing.
#[test]
fn a_label_draws_in_its_bounds_and_not_from_behind() {
    for msaa in [Msaa::Off, Msaa::Sample4] {
        let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
        let mut app = app();
        let world = app.world_mut();
        let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y), (320, 180), Color::BLACK, msaa);
        let e = label(world, "HELLO", WorldTextStyle::default(), Transform::default());
        until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
            let b = label_box(w, cam, e)?;
            let (inner, outer) = (count(inside(img, b), white), count(all(img).filter(|(p, _)| !b.grow(2).contains(*p)).map(|(_, c)| c), |c| !black(c)));
            if inner > 200 && outer == 0 { Ok(()) } else { Err(format!("{msaa:?}: {inner} white inside {b:?}, {outer} drawn outside")) }
        });
        *app.world_mut().get_mut::<Transform>(cam).unwrap() = Transform::from_xyz(0.0, 0.0, -5.0).looking_at(Vec3::ZERO, Vec3::Y);
        shot(&mut app, &target);
        let img = shot(&mut app, &target);
        assert_eq!(count(all(&img).map(|(_, c)| c), |c| !black(c)), 0, "{msaa:?}: drawn from behind");
    }
}

/// A red cube between the camera and the label's left half: white only right of the cube's
/// edge, red where the left half would be.
#[test]
fn a_cube_in_front_hides_half_a_label() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y), (320, 180), Color::BLACK, Msaa::Sample4);
    let e = label(world, "HHHHHHHH", WorldTextStyle::default(), Transform::default());
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(4.0, 4.0, 0.2));
    let solid = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial { base_color: Color::srgb(1.0, 0.0, 0.0), unlit: true, ..default() });
    // Its right face at x = 0, in front of the label.
    world.spawn((Mesh3d(cube), MeshMaterial3d(solid), Transform::from_xyz(-2.0, 0.0, 1.0)));
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let b = label_box(w, cam, e)?;
        let edge = project(w, cam, [Vec3::new(0.0, 0.0, 1.1), Vec3::new(0.0, 0.0, 1.1)])?.max.x;
        let left = Px { min: b.min, max: IVec2::new(edge - 1, b.max.y) };
        let right = Px { min: IVec2::new(edge + 1, b.min.y), max: b.max };
        let (lw, lr, rw) = (count(inside(img, left), white), count(inside(img, left), red), count(inside(img, right), white));
        if lw == 0 && lr > 0 && rw > 100 { Ok(()) } else { Err(format!("left: {lw} white, {lr} red; right: {rw} white")) }
    });
}

/// Under a depth prepass, a label in front of a red cube: red shows between the glyphs.
#[test]
fn under_a_depth_prepass_the_space_between_glyphs_hides_nothing() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y), (320, 180), Color::BLACK, Msaa::Off);
    world.entity_mut(cam).insert(DepthPrepass);
    let e = label(world, "I I I I", WorldTextStyle::default(), Transform::default());
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(20.0, 20.0, 0.2));
    let solid = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial { base_color: Color::srgb(1.0, 0.0, 0.0), unlit: true, ..default() });
    world.spawn((Mesh3d(cube), MeshMaterial3d(solid), Transform::from_xyz(0.0, 0.0, -1.0)));
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let b = label_box(w, cam, e)?;
        let n = ((b.max - b.min).x * (b.max - b.min).y) as f32;
        let (r, wh) = (count(inside(img, b), red) as f32 / n, count(inside(img, b), white) as f32 / n);
        if r > 0.5 && wh > 0.02 { Ok(()) } else { Err(format!("red {r}, white {wh} in {b:?}")) }
    });
}

/// A glyph 400 pixels high: across its stem the fill goes from background to full within 2
/// pixels.
#[test]
fn close_up_an_edge_is_sharp() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y), (480, 480), Color::BLACK, Msaa::Sample4);
    let e = label(world, "I", WorldTextStyle::default(), Transform::default());
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let b = label_box(w, cam, e)?;
        let row = (b.min.y + b.max.y) / 2;
        let lum = |x: i32| rgb(img, IVec2::new(x, row))[0];
        let xs: Vec<i32> = (b.min.x.max(0)..b.max.x.min(480)).collect();
        let lit = xs.iter().filter(|x| lum(**x) > 245).count();
        if lit < 20 {
            return Err(format!("{lit} full pixels across the stem"));
        }
        // Each run of pixels between background and full colour.
        let (mut ramp, mut worst) = (0, 0);
        for x in &xs {
            let v = lum(*x);
            if v > 10 && v < 245 { ramp += 1 } else { ramp = 0 }
            worst = worst.max(ramp);
        }
        if worst <= 2 { Ok(()) } else { Err(format!("an edge {worst} pixels wide")) }
    });
}

/// An em of 6 pixels, the camera moving a tenth of a pixel a frame for 10 frames: drawn coverage
/// changes by less than 10% between any two frames.
#[test]
fn far_away_a_label_does_not_shimmer() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (w, h) = (320u32, 180u32);
    let z = 10.0;
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, z), (w, h), Color::BLACK, Msaa::Sample4);
    // Bevy's default perspective: 45° vertical. A world unit at distance z is h / (2 z tan 22.5°) pixels.
    let per_unit = h as f32 / (2.0 * z * (std::f32::consts::FRAC_PI_8).tan());
    let size = 6.0 / per_unit;
    let e = label(world, "Distant label here", WorldTextStyle { size, ..default() }, Transform::default());
    // Summed as light, not as sRGB values: a pixel half covered is half the light of one covered.
    let coverage = |img: &Image| all(img).map(|(_, c)| (c[0] as f32 / 255.0).powf(2.2)).sum::<f32>();
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        // Lit inside the label's box and dark outside it: the label drawn, not a target the
        // camera has not cleared yet.
        let b = label_box(w, cam, e)?.grow(2);
        let outside = all(img).filter(|(p, c)| !b.contains(*p) && c[0] > 0).count();
        if coverage(img) > 0.0 && outside == 0 { Ok(()) } else { Err(format!("{outside} pixels lit outside the label")) }
    });
    let mut seen = Vec::new();
    for i in 0..10 {
        app.world_mut().get_mut::<Transform>(cam).unwrap().translation.x = i as f32 * 0.1 / per_unit;
        shot(&mut app, &target);
        seen.push(coverage(&shot(&mut app, &target)));
    }
    let (lo, hi) = seen.iter().fold((f32::MAX, 0f32), |(l, h), c| (l.min(*c), h.max(*c)));
    assert!(hi - lo < 0.1 * lo, "coverage over 10 sub-pixel moves: {seen:?}");
}

/// White with a black outline of 0.1 on red: black between every white region and the red, and
/// no black farther out than the outline (projected, plus 2 pixels).
#[test]
fn an_outline_is_a_band_around_every_glyph() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y), (320, 180), Color::srgb(1.0, 0.0, 0.0), Msaa::Sample4);
    let e = label(world, "Ob", WorldTextStyle { outline: Some((0.1, Color::BLACK)), ..default() }, Transform::default());
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let b = label_box(w, cam, e)?;
        let band = project(w, cam, [Vec3::ZERO, Vec3::new(0.1, 0.0, 0.0)])?;
        let reach = (band.max.x - band.min.x) as f32 + 2.0;
        let px: Vec<(IVec2, [u8; 3])> = all(img).filter(|(p, _)| b.grow(4).contains(*p)).collect();
        let whites: HashSet<IVec2> = px.iter().filter(|(_, c)| white(*c)).map(|(p, _)| *p).collect();
        let blacks: Vec<IVec2> = px.iter().filter(|(_, c)| black(*c)).map(|(p, _)| *p).collect();
        if whites.len() < 50 || blacks.len() < 50 {
            return Err(format!("{} white, {} black", whites.len(), blacks.len()));
        }
        let touching = px.iter().filter(|(p, c)| red(*c) && [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y].iter().any(|d| whites.contains(&(*p + *d)))).count();
        let far = blacks.iter().filter(|b| whites.iter().all(|w| w.as_vec2().distance(b.as_vec2()) > reach)).count();
        if touching == 0 && far == 0 { Ok(()) } else { Err(format!("{touching} red pixels touch white, {far} black pixels farther than {reach} px")) }
    });
}

/// No light and no ambient light: the unlit label draws white, the lit one darker than 10% grey.
#[test]
fn lit_labels_take_the_light_and_unlit_ones_do_not() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    app.insert_resource(GlobalAmbientLight { brightness: 0.0, ..default() });
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y), (320, 180), Color::srgb(0.5, 0.5, 0.5), Msaa::Off);
    let unlit = label(world, "MMM", WorldTextStyle::default(), Transform::from_xyz(-1.5, 0.0, 0.0));
    let lit = label(world, "MMM", WorldTextStyle { lit: true, ..default() }, Transform::from_xyz(1.5, 0.0, 0.0));
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let (u, l) = (label_box(w, cam, unlit)?, label_box(w, cam, lit)?);
        let dark = |c: [u8; 3]| c.iter().all(|v| *v < 26);
        let (uw, ld, lw) = (count(inside(img, u), white), count(inside(img, l), dark), count(inside(img, l), white));
        if uw > 100 && ld > 100 && lw == 0 { Ok(()) } else { Err(format!("unlit: {uw} white; lit: {ld} dark, {lw} white")) }
    });
}
