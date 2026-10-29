//! Script passes drawn (change `script-fullscreen-pass`): Bevy's `FullscreenMaterial` on a
//! `Camera2d`, into an offscreen image, through the `passes` section's apply as the engine runs it.

use crate::alpha_render::{GPU, app_with, until_drawn};
use crate::pass::{decl_of, load};
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts_material::pass::{self, PassKinds, frame_fields, slot_bytes};
use bevy_ts_material::{PixelCamera, Pixel2dPlugin, ScriptPassPlugin};
use serde_json::{Map, Value, json};

const W: u32 = 320;
const H: u32 = 180;

fn app() -> App {
    let mut app = app_with(|p| p.add(ScriptPassPlugin).add(Pixel2dPlugin));
    app.init_resource::<ScriptLog>();
    app
}

/// A `Camera2d` drawing into a new image over white, with `extra`; the image and the camera.
fn camera(app: &mut App, extra: impl Bundle) -> (Handle<Image>, Entity) {
    let world = app.world_mut();
    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    let cam = world
        .spawn((Camera2d, Camera { clear_color: ClearColorConfig::Custom(Color::WHITE), ..default() }, Tonemapping::None, RenderTarget::Image(target.clone().into()), extra))
        .id();
    (target, cam)
}

fn props(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn centre(image: &Image) -> Srgba {
    image.get_color_at(W / 2, H / 2).unwrap().to_srgba()
}

fn is(c: Srgba, want: Srgba) -> bool {
    (c.red - want.red).abs() < 0.05 && (c.green - want.green).abs() < 0.05 && (c.blue - want.blue).abs() < 0.05
}

/// Draws until the centre is `want`.
fn until_centre(app: &mut App, target: &Handle<Image>, want: Srgba, what: &str) {
    until_drawn(app, target, |_| Ok(()), |_, img| {
        let c = centre(img);
        if is(c, want) { Ok(()) } else { Err(format!("{what}: the centre is {c:?}, not {want:?}")) }
    });
}

#[test]
fn a_red_pass_turns_the_image_red_and_no_pass_leaves_it() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let (target, cam) = camera(&mut app, ());
    until_centre(&mut app, &target, Srgba::WHITE, "no pass");
    load(&mut app, 1, vec![decl_of("game::Red", json!([]), "red.wgsl", 0.0)]).unwrap();
    pass::set(app.world_mut(), cam, "game::Red", Map::new(), None);
    until_centre(&mut app, &target, Srgba::RED, "the red pass");
    pass::remove(app.world_mut(), cam, "game::Red");
    until_centre(&mut app, &target, Srgba::WHITE, "the pass removed");
}

/// A 16×16 checker of black and red texels, sampled nearest.
fn checker() -> Image {
    let mut data = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            data.extend(if (x + y) % 2 == 0 { [0, 0, 0, 255] } else { [255, 0, 0, 255] });
        }
    }
    let mut image = Image::new(Extent3d { width: 16, height: 16, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, default());
    image.sampler = ImageSampler::nearest();
    image
}

#[test]
fn a_passthrough_at_zoom_2_draws_the_same_pixels_as_no_pass() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let (target, cam) = camera(&mut app, PixelCamera { zoom: 2.0, ease: 0.0 });
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(checker());
    app.world_mut().spawn(Sprite::from_image(image));
    // The checker's middle row: 32 pixels in runs of 2, black and red by turns.
    let row = |img: &Image| -> Vec<[u8; 4]> { (W / 2 - 16..W / 2 + 16).map(|x| img.get_color_at(x, H / 2).unwrap().to_srgba().to_u8_array()).collect() };
    let mut without = None;
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let r = row(img);
        let whole = r.chunks(2).all(|p| p[0] == p[1]) && r.chunks(2).zip(r.chunks(2).skip(1)).all(|(a, b)| a[0] != b[0]);
        if whole {
            without = Some(img.data.clone());
            Ok(())
        } else {
            Err(format!("the checker is not whole 2×2 texels: {r:?}"))
        }
    });
    load(&mut app, 1, vec![decl_of("game::Same", json!([]), "passthrough.wgsl", 0.0)]).unwrap();
    pass::set(app.world_mut(), cam, "game::Same", Map::new(), None);
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let drawing = w.resource::<bevy_ts_material::wgsl::PipelineWatch>().0.lock().unwrap().drawing.contains(&pass::SLOT_SHADERS[0].id());
        if !drawing {
            return Err("the passthrough's pipeline does not draw yet".into());
        }
        let same = Some(&img.data) == without.as_ref();
        if same { Ok(()) } else { Err("the image with the passthrough differs from the one without".into()) }
    });
}

#[test]
fn two_passes_run_in_order() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let (target, cam) = camera(&mut app, ());
    let tint = json!([{ "name": "tint", "type": "color" }]);
    load(&mut app, 1, vec![decl_of("Tint", tint.clone(), "tint.wgsl", 0.0), decl_of("Invert", json!([]), "invert.wgsl", 1.0)]).unwrap();
    pass::set(app.world_mut(), cam, "Tint", props(json!({ "tint": "#ff0000" })), None);
    pass::set(app.world_mut(), cam, "Invert", Map::new(), None);
    // White, tinted red, inverted: cyan.
    until_centre(&mut app, &target, Srgba::rgb(0.0, 1.0, 1.0), "tint then invert");
    // The orders swapped: white inverted is black, and black tinted stays black.
    load(&mut app, 2, vec![decl_of("Tint", tint, "tint.wgsl", 1.0), decl_of("Invert", json!([]), "invert.wgsl", 0.0)]).unwrap();
    until_centre(&mut app, &target, Srgba::BLACK, "invert then tint");
    let tint_bytes = slot_bytes(app.world(), cam, 1).unwrap();
    assert_eq!(f32::from_le_bytes(tint_bytes[32..36].try_into().unwrap()), 1.0, "Tint's red moved with it to slot 1");
}

#[test]
fn the_frame_fields_under_a_pixel_camera() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let (target, cam) = camera(&mut app, (PixelCamera { zoom: 2.0, ease: 0.0 }, Transform::from_xyz(10.3, 0.0, 0.0)));
    load(&mut app, 1, vec![decl_of("game::Frame", json!([]), "frame.wgsl", 0.0)]).unwrap();
    pass::set(app.world_mut(), cam, "game::Frame", Map::new(), None);
    // The shader shows them: red texel / 4, green size.x / 1000, blue view.x / 100.
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let (_, texel, size, view) = frame_fields(&slot_bytes(w, cam, 0).unwrap());
        if (texel - 2.0).abs() > 1e-4 || size != Vec2::new(320.0, 180.0) || (view.x - 10.5).abs() > 1e-4 {
            return Err(format!("texel {texel}, size {size}, view {view}"));
        }
        let c = LinearRgba::from(centre(img));
        let want = LinearRgba::rgb(0.5, 0.32, 0.105);
        let near = (c.red - want.red).abs() < 0.01 && (c.green - want.green).abs() < 0.01 && (c.blue - want.blue).abs() < 0.01;
        if near { Ok(()) } else { Err(format!("the pass drew {c:?}, not {want:?}")) }
    });
    // Time stops with the game.
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.update();
    let held = frame_fields(&slot_bytes(app.world(), cam, 0).unwrap()).0;
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(frame_fields(&slot_bytes(app.world(), cam, 0).unwrap()).0, held, "time holds while paused");
}

#[test]
fn texel_follows_the_camera_scale() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    // 320 pixels over 320 world units, scaled 2: 640 world units, half a pixel each.
    let (target, cam) = camera(&mut app, Transform::from_scale(Vec3::splat(2.0)));
    load(&mut app, 1, vec![decl_of("game::Frame", json!([]), "frame.wgsl", 0.0)]).unwrap();
    pass::set(app.world_mut(), cam, "game::Frame", Map::new(), None);
    until_drawn(&mut app, &target, |_| Ok(()), |w, _| {
        let (_, texel, _, _) = frame_fields(&slot_bytes(w, cam, 0).unwrap());
        if (texel - 0.5).abs() < 1e-4 { Ok(()) } else { Err(format!("texel {texel}, not 0.5")) }
    });
}

#[test]
fn a_replaced_shader_draws_within_frames_and_a_bad_one_keeps_the_last() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let (target, cam) = camera(&mut app, ());
    load(&mut app, 1, vec![decl_of("game::K", json!([]), "red.wgsl", 0.0)]).unwrap();
    pass::set(app.world_mut(), cam, "game::K", Map::new(), None);
    until_centre(&mut app, &target, Srgba::RED, "red.wgsl");
    load(&mut app, 2, vec![decl_of("game::K", json!([]), "blue.wgsl", 0.0)]).unwrap();
    until_centre(&mut app, &target, Srgba::BLUE, "blue.wgsl in red's place");
    load(&mut app, 3, vec![decl_of("game::K", json!([]), "bad.wgsl", 0.0)]).unwrap();
    let log = app.world().resource::<ScriptLog>();
    let e = log.since(0).filter(|e| e.level == LogLevel::Error).last().expect("an error");
    let at = e.location.clone().expect("a location");
    assert!(e.message.contains("pass game::K") && e.message.contains("the previous pass keeps drawing"), "{}", e.message);
    assert!(at.file.ends_with("bad.wgsl") && at.line == 6, "{at:?}: {}", e.message);
    assert!(app.world().resource::<PassKinds>().live["game::K"].wanted.is_some());
    until_centre(&mut app, &target, Srgba::BLUE, "blue.wgsl kept");
}
