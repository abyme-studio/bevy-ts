//! Y-sort drawn (change `pixel-art-2d`): two overlapping sprites in one layer, the lower in front.

use crate::alpha_render::{GPU, app_with, until_drawn};
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy_ts_material::{Pixel2dPlugin, YSort};

const W: u32 = 320;
const H: u32 = 180;

/// The colour at the view's centre plus `[5, 0]`, inside both sprites whichever is in front.
fn overlap(image: &Image) -> Srgba {
    image.get_color_at(W / 2 + 5, H / 2).unwrap().to_srgba()
}

fn is(c: Srgba, want: Srgba) -> bool {
    (c.red - want.red).abs() < 0.05 && (c.green - want.green).abs() < 0.05 && (c.blue - want.blue).abs() < 0.05
}

#[test]
fn the_lower_of_two_y_sorted_sprites_draws_in_front() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p.add(Pixel2dPlugin));
    let world = app.world_mut();
    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    world.spawn((Camera2d, Camera { clear_color: ClearColorConfig::Custom(Color::WHITE), ..default() }, Tonemapping::None, RenderTarget::Image(target.clone().into())));
    let sprite = |c: Srgba| Sprite::from_color(c, Vec2::splat(40.0));
    // Spawned blue last, so without y-sort the tie at z 0 would not decide for red.
    let red = world.spawn((sprite(Srgba::RED), YSort { layer: 1.0, offset: 0.0 }, Transform::from_xyz(0.0, 0.0, 0.0))).id();
    let blue = world.spawn((sprite(Srgba::BLUE), YSort { layer: 1.0, offset: 0.0 }, Transform::from_xyz(10.0, 8.0, 0.0))).id();
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
        let c = overlap(img);
        if is(c, Srgba::RED) { Ok(()) } else { Err(format!("the overlap is {c:?}, not red")) }
    });
    app.world_mut().get_mut::<Transform>(blue).unwrap().translation.y = -8.0;
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        let c = overlap(img);
        let z = |e: Entity| w.get::<GlobalTransform>(e).unwrap().translation().z;
        if is(c, Srgba::BLUE) { Ok(()) } else { Err(format!("the overlap is {c:?}, not blue; red z {}, blue z {}", z(red), z(blue))) }
    });
}

/// How many pixels of the view's middle row are red.
fn red_run(image: &Image) -> u32 {
    (0..W).filter(|x| is(image.get_color_at(*x, H / 2).unwrap().to_srgba(), Srgba::RED)).count() as u32
}

#[test]
fn a_screen_size_marker_covers_16_pixels_at_zoom_1_and_4() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p.add(Pixel2dPlugin));
    let world = app.world_mut();
    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    let cam = world.spawn((Camera2d, Camera { clear_color: ClearColorConfig::Custom(Color::WHITE), ..default() }, Tonemapping::None, RenderTarget::Image(target.clone().into()))).id();
    // At the origin of an even-sized view, so its edges fall on whole pixels at every zoom.
    world.spawn((Sprite::from_color(Srgba::RED, Vec2::splat(16.0)), bevy_ts_material::ScreenSize::default()));
    for zoom in [1.0, 4.0] {
        let Projection::Orthographic(o) = &mut *app.world_mut().get_mut::<Projection>(cam).unwrap() else { unreachable!() };
        o.scale = 1.0 / zoom;
        until_drawn(&mut app, &target, |_| Ok(()), |_, img| {
            let n = red_run(img);
            if n == 16 { Ok(()) } else { Err(format!("zoom {zoom}: {n} red pixels, not 16")) }
        });
    }
}

/// The first red x of the middle row, and how many red pixels it has.
fn red_span(image: &Image) -> Option<(u32, u32)> {
    let first = (0..W).find(|x| is(image.get_color_at(*x, H / 2).unwrap().to_srgba(), Srgba::RED))?;
    Some((first, red_run(image)))
}

#[test]
fn at_zoom_2_a_16_px_sprite_covers_32_whole_pixels_wherever_it_is() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with(|p| p.add(Pixel2dPlugin));
    let world = app.world_mut();
    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    world.spawn((
        Camera2d,
        Camera { clear_color: ClearColorConfig::Custom(Color::WHITE), ..default() },
        Tonemapping::None,
        RenderTarget::Image(target.clone().into()),
        bevy_ts_material::PixelCamera { zoom: 2.0, ease: 0.0 },
    ));
    let sprite = world.spawn((Sprite::from_color(Srgba::RED, Vec2::splat(16.0)), Transform::from_xyz(0.5, 0.0, 0.0))).id();
    let mut at_half = None;
    until_drawn(&mut app, &target, |_| Ok(()), |_, img| match red_span(img) {
        Some((x, 32)) => {
            at_half = Some(x);
            Ok(())
        }
        s => Err(format!("at x 0.5: {s:?}")),
    });
    app.world_mut().get_mut::<Transform>(sprite).unwrap().translation.x = 0.3;
    until_drawn(&mut app, &target, |_| Ok(()), |w, img| {
        assert_eq!(w.get::<Transform>(sprite).unwrap().translation.x, 0.3);
        match red_span(img) {
            s if s == Some((at_half.unwrap(), 32)) => Ok(()),
            s => Err(format!("at x 0.3: {s:?}, not the pixels at 0.5 ({at_half:?}, 32)")),
        }
    });
}
