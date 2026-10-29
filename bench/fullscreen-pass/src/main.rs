//! One script pass over a 2D scene at 1920×1080 (change `script-fullscreen-pass`, design 8): a
//! `Camera2d` under `PixelCamera` zoom 3 drawing 2,000 sprites from a 16×16 atlas the bench fills
//! in code, into an image (headless, nothing on screen). Arms: `off` (no pass), `passthrough`
//! (`shaders/passthrough.wgsl`, the screen read and written back) and `grade` (`shaders/grade.wgsl`,
//! a dusk-like grade driven by `time`), each declared through the `passes` section as the engine
//! does. Rendering is not pipelined and each frame waits for the GPU to finish, so the frame time
//! holds the pass's GPU time; Metal gives no timestamps, so the pass span's CPU time is printed
//! beside it. Prints one line: frame time and the pass span, median and p95, over 600 frames after
//! 120 warm-up.

use bevy::camera::RenderTarget;
use bevy::diagnostic::DiagnosticsStore;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, PollType, TextureDimension, TextureFormat};
use bevy::render::renderer::RenderDevice;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::log::ScriptLog;
use bevy_ts_material::pass::{self, SLOT_SHADERS};
use bevy_ts_material::wgsl::PipelineWatch;
use bevy_ts_material::{PixelCamera, Pixel2dPlugin, ScriptMaterialRenderPlugin, ScriptPassPlugin};
use std::path::PathBuf;
use std::time::Instant;

const W: u32 = 1920;
const H: u32 = 1080;
const SPRITES: usize = 2_000;
const WARMUP: usize = 120;
const FRAMES: usize = 600;
const SPAN: &str = "render/fullscreen_material_pass<bevy_ts_material::pass::ScriptPass<0>>/elapsed_cpu";

/// A 64×64 atlas of sixteen 16×16 tiles, each its own colour with a darker checker, sampled nearest.
fn atlas() -> Image {
    let mut data = vec![0u8; 64 * 64 * 4];
    for y in 0..64usize {
        for x in 0..64usize {
            let tile = (y / 16) * 4 + x / 16;
            let shade = if (x + y) % 2 == 0 { 255 } else { 160 };
            let c = [(tile * 47 % 256) as u8, (tile * 91 % 256) as u8, (tile * 13 % 256) as u8];
            let i = (y * 64 + x) * 4;
            for k in 0..3 {
                data[i + k] = (c[k] as u32 * shade / 255) as u8;
            }
            data[i + 3] = 255;
        }
    }
    let mut image = Image::new(Extent3d { width: 64, height: 64, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, default());
    image.sampler = ImageSampler::nearest();
    image
}

/// Waits for the GPU to finish the frame, so the frame time holds the GPU's work.
fn wait_for_gpu(device: Res<RenderDevice>) {
    device.poll(PollType::wait_indefinitely()).expect("the GPU finished");
}

fn main() {
    let arm = std::env::args().nth(1).unwrap_or_else(|| "grade".into());
    let fragment = match arm.as_str() {
        "off" => None,
        "passthrough" => Some("passthrough.wgsl"),
        "grade" => Some("grade.wgsl"),
        a => panic!("arm {a:?}: off, passthrough or grade"),
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
    .add_plugins((bevy::render::diagnostic::RenderDiagnosticsPlugin, ScriptMaterialRenderPlugin, ScriptPassPlugin, Pixel2dPlugin))
    .init_resource::<ScriptLog>();
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
    let cam = world.spawn((Camera2d, RenderTarget::Image(target.into()), PixelCamera { zoom: 3.0, ease: 0.0 })).id();
    let image = world.resource_mut::<Assets<Image>>().add(atlas());
    let layout = world.resource_mut::<Assets<TextureAtlasLayout>>().add(TextureAtlasLayout::from_grid(UVec2::splat(16), 4, 4, None, None));
    // The view at zoom 3 is 640×360 world units: sprites spread over it on a fixed pattern.
    for i in 0..SPRITES {
        let x = ((i * 7919) % 640) as f32 - 320.0;
        let y = ((i * 104_729) % 360) as f32 - 180.0;
        let sprite = Sprite::from_atlas_image(image.clone(), TextureAtlas { layout: layout.clone(), index: i % 16 });
        world.spawn((sprite, Transform::from_xyz(x, y, i as f32 * 1e-4)));
    }
    if let Some(fragment) = fragment {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders").join(fragment);
        let params = serde_json::json!([{ "name": "amount", "type": "f32" }, { "name": "tint", "type": "color" }]);
        let items = serde_json::json!({ "items": [{ "name": "bench::Pass", "params": params, "fragment": file, "order": 0 }] });
        pass::check(world, 1, &serde_json::to_vec(&items).unwrap(), None).expect("the pass declares");
        pass::apply(world, 1, true);
        let props = serde_json::json!({ "amount": 0.6, "tint": "#ff9a5c" });
        pass::set(world, cam, "bench::Pass", props.as_object().unwrap().clone(), None);
    }
    // The pass's pipeline built before the warm-up starts.
    let start = Instant::now();
    while fragment.is_some() && !app.world().resource::<PipelineWatch>().0.lock().unwrap().drawing.contains(&SLOT_SHADERS[0].id()) {
        app.update();
        assert!(start.elapsed().as_secs() < 300, "the pass's pipeline never built");
    }
    for _ in 0..WARMUP {
        app.update();
    }
    let (mut frame, mut span) = (Vec::with_capacity(FRAMES), Vec::with_capacity(FRAMES));
    let path = bevy::diagnostic::DiagnosticPath::new(SPAN);
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        frame.push(t.elapsed().as_secs_f64() * 1e3);
        if let Some(v) = app.world().resource::<DiagnosticsStore>().get(&path).and_then(|d| d.measurement()) {
            span.push(v.value);
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
        "arm={arm} size={W}x{H} sprites={SPRITES} frame_median_ms={:.3} frame_p95_ms={:.3} pass_cpu_median_ms={:.3} pass_cpu_p95_ms={:.3}",
        at(&frame, 50),
        at(&frame, 95),
        at(&span, 50),
        at(&span, 95),
    );
}
