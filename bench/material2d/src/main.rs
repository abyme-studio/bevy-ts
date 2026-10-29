//! Script 2D materials at 1920×1080 (change `script-material2d`, design 8): a `Camera2d` under
//! `PixelCamera` zoom 3 drawing pixel-art planets into an image (headless, nothing on screen). Each
//! planet is a parent at whole world units with one rectangle child per layer, at local z steps of
//! 0.00001; each layer is a script 2D kind declared through the `materials` section as the engine
//! does, its seed and size instance params, so each kind is one asset. Kinds stand in for
//! learn's planet layers at the research's hashes per pixel: `sea` (12, opaque), `land` (120,
//! blend), `cloud` (17, blend), `ring` (16, blend).
//!
//! Arms: `off` (the camera, no planets); `one` (a 200-art-px planet of sea, land and cloud);
//! `n20`, `n100`, `n200`, `n500` (48-art-px planets of three layers, a ring on every fifth). `n200`
//! is recorded for learn-planets and gates nothing. The view is
//! 640×360 world units, so 48-unit planets fit 13 by 7 without overlapping: the planets sit on a
//! 14 by 8 grid 45 units apart, and past 112 the grid repeats shifted, overlapping.
//!
//! Rendering is not pipelined and each frame waits for the GPU, so the frame time holds the GPU's
//! work; Metal gives no timestamps, so the 2D main passes' CPU spans are printed beside it. Prints
//! one line: frame time and the spans, median and p95, over 600 frames after 120 warm-up.

use bevy::camera::RenderTarget;
use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use bevy::render::render_resource::{PollType, TextureFormat};
use bevy::render::renderer::RenderDevice;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::log::ScriptLog;
use bevy_ts_material::wgsl::PipelineWatch;
use bevy_ts_material::{MaterialKinds, MeshScriptMaterial2d, PixelCamera, Pixel2dPlugin, ScriptMaterialRenderPlugin, instance, kinds};
use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

const W: u32 = 1920;
const H: u32 = 1080;
const WARMUP: usize = 120;
const FRAMES: usize = 600;
const OPAQUE: &str = "render/main_opaque_pass_2d/elapsed_cpu";
const TRANSPARENT: &str = "render/main_transparent_pass_2d/elapsed_cpu";

/// Each kind: its name, file and alpha mode.
const KINDS: [(&str, &str, &str); 4] = [("bench::Sea", "sea.wgsl", "opaque"), ("bench::Land", "land.wgsl", "blend"), ("bench::Cloud", "cloud.wgsl", "blend"), ("bench::Ring", "ring.wgsl", "blend")];

/// Waits for the GPU to finish the frame, so the frame time holds the GPU's work.
fn wait_for_gpu(device: Res<RenderDevice>) {
    device.poll(PollType::wait_indefinitely()).expect("the GPU finished");
}

/// Declares the four kinds as `material2d()` writes them.
fn declare(world: &mut World) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders");
    let items: Vec<_> = KINDS
        .iter()
        .map(|(name, file, alpha)| {
            json!({
                "name": name, "params": [{ "name": "tint", "type": "color" }], "textures": [],
                "instance": [{ "name": "pixels", "type": "f32" }, { "name": "seed", "type": "f32" }],
                "fragment": dir.join(file), "alphaMode": alpha, "space": "2d",
            })
        })
        .collect();
    kinds::check(world, 1, &serde_json::to_vec(&json!({ "items": items })).unwrap(), None).expect("the kinds declare");
    kinds::apply(world, 1, true);
}

/// A planet of `size` art pixels at `at` (whole world units): its layers as children.
fn planet(world: &mut World, at: Vec2, size: f32, seed: f32, layers: &[(&str, &str)], quad: &Handle<Mesh>) {
    let parent = world.spawn((Transform::from_xyz(at.x, at.y, seed * 1e-3), Visibility::default())).id();
    for (i, (kind, tint)) in layers.iter().enumerate() {
        // A ring's quad is half again the planet's, so its art pixels stay the same size.
        let pixels = if *kind == "bench::Ring" { size * 1.5 } else { size };
        let e = world.spawn((Mesh2d(quad.clone()), Transform::from_xyz(0.0, 0.0, i as f32 * 1e-5).with_scale(Vec3::new(pixels, pixels, 1.0)), ChildOf(parent))).id();
        let props = json!({ "tint": tint, "pixels": pixels, "seed": seed });
        let h = instance::apply2d(world, e, kind, props.as_object().unwrap(), None).expect("an instance");
        world.entity_mut(e).insert(MeshScriptMaterial2d(h));
    }
}

fn main() {
    let arm = std::env::args().nth(1).unwrap_or_else(|| "n100".into());
    let (count, size) = match arm.as_str() {
        "off" => (0, 0.0),
        "one" => (1, 200.0),
        "n20" => (20, 48.0),
        "n100" => (100, 48.0),
        "n200" => (200, 48.0),
        "n500" => (500, 48.0),
        a => panic!("arm {a:?}: off, one, n20, n100, n200 or n500"),
    };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins((bevy::render::diagnostic::RenderDiagnosticsPlugin, ScriptMaterialRenderPlugin, Pixel2dPlugin))
    .init_resource::<ScriptLog>()
    .init_resource::<MaterialKinds>();
    app.sub_app_mut(RenderApp).add_systems(Render, wait_for_gpu.in_set(RenderSystems::Cleanup));
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();

    let world = app.world_mut();
    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    world.spawn((Camera2d, RenderTarget::Image(target.into()), Msaa::Off, PixelCamera { zoom: 3.0, ease: 0.0 }));
    declare(world);
    // Unit quads, scaled to each layer's size: one mesh asset for every layer.
    let quad = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(1.0, 1.0));
    let three = [("bench::Sea", "#2a5d9e"), ("bench::Land", "#5f8f3a"), ("bench::Cloud", "#f4f1e8")];
    let four = [three[0], three[1], three[2], ("bench::Ring", "#c9a46a")];
    for i in 0..count {
        let at = if count == 1 {
            Vec2::ZERO
        } else {
            let (cell, pass) = (i % 112, (i / 112) as f32);
            Vec2::new(-298.0 + (cell % 14) as f32 * 45.0 + pass * 15.0, -158.0 + (cell / 14) as f32 * 45.0 + pass * 11.0)
        };
        let layers: &[(&str, &str)] = if count > 1 && i % 5 == 4 { &four } else { &three };
        planet(world, at, size, i as f32, layers, &quad);
    }
    // Every kind drawn has built its pipeline before the warm-up starts.
    let used: Vec<_> = {
        let kinds = app.world().resource::<MaterialKinds>();
        let n = match count {
            0 => 0,
            1 => 3,
            _ => 4,
        };
        KINDS[..n].iter().map(|(k, ..)| kinds.live[*k].fragment.handle.id()).collect()
    };
    let start = Instant::now();
    while !used.iter().all(|id| app.world().resource::<PipelineWatch>().0.lock().unwrap().drawing.contains(id)) {
        app.update();
        assert!(start.elapsed().as_secs() < 300, "the kinds' pipelines never built");
    }
    for _ in 0..WARMUP {
        app.update();
    }
    let (mut frame, mut opaque, mut transparent) = (Vec::with_capacity(FRAMES), Vec::new(), Vec::new());
    let (op, tp) = (bevy::diagnostic::DiagnosticPath::new(OPAQUE), bevy::diagnostic::DiagnosticPath::new(TRANSPARENT));
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        frame.push(t.elapsed().as_secs_f64() * 1e3);
        let store = app.world().resource::<DiagnosticsStore>();
        if let Some(v) = store.get(&op).and_then(|d| d.measurement()) {
            opaque.push(v.value);
        }
        if let Some(v) = store.get(&tp).and_then(|d| d.measurement()) {
            transparent.push(v.value);
        }
    }
    let at = |v: &[f64], p: usize| {
        if v.is_empty() {
            return f64::NAN;
        }
        let mut v = v.to_vec();
        v.sort_by(f64::total_cmp);
        v[v.len() * p / 100]
    };
    println!(
        "arm={arm} size={W}x{H} planets={count} frame_median_ms={:.3} frame_p95_ms={:.3} opaque_cpu_median_ms={:.3} transparent_cpu_median_ms={:.3} transparent_cpu_p95_ms={:.3}",
        at(&frame, 50),
        at(&frame, 95),
        at(&opaque, 50),
        at(&transparent, 50),
        at(&transparent, 95),
    );
}
