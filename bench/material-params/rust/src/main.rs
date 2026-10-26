//! One run of one arm: `material-params-bench <set|rekey> <n>`. N cubes drawn offscreen, each with
//! its own `game::Glow`, its `pulse` changed every frame:
//! - `set`: N owned instances (`Glow.add`), each written with `assets.set` by a system;
//! - `rekey`: N `<material kind={Glow} pulse={...} />` rows of a `<For>`, keyed by their pulse, so
//!   each frame's value is a new key (today's only way to change a param from TSX).
//!
//! Prints one line, `bench: ` and JSON: mean and p95 frame time (`app.update()`, pipelined rendering off, so the
//! render world's work is in it), the most `ScriptMaterial` assets alive, and material prepares per
//! frame (`AssetEvent` added or modified: each makes a uniform buffer and a bind group).

use bevy::camera::RenderTarget;
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_resource::{PipelineCache, TextureFormat};
use bevy::render::RenderApp;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_material::{ScriptMaterial, ScriptMaterialPlugin};
use bevy_ts_v8::V8Options;
use std::sync::Arc;
use std::time::Instant;

const WARMUP: usize = 120;
const FRAMES: usize = 600;

const WGSL: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return params.color * params.pulse;\n}\n";

fn game(arm: &str, n: usize) -> String {
    let head = format!(
        "import {{ app, assets, signal, For, Update }} from \"bevy-ts\";\nimport {{ Camera3d, Transform }} from \"bevy-ts/bevy\";\nimport {{ color, f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ color: color, pulse: f32 }}, fragment: glow }});\nconst N = {n};\nconst at = (i: number): [number, number, number] => [(i % 10) * 3 - 13.5, Math.floor(i / 10) * 3 - 13.5, 0];\nlet t = 0;\nconst pulse = (i: number) => ((t + i) % 100) / 100;\nconst cam = <entity name=\"cam\"><Camera3d /><Transform translation={{[0, 0, 40]}} /></entity>;\n"
    );
    let body = match arm {
        "set" => "const gs = Array.from({ length: N }, (_, i) => Glow.add({ color: \"#ff8800\", pulse: pulse(i) }));\nexport const [rows] = signal(Array.from({ length: N }, (_, i) => i));\nexport function write() { t += 1; for (let i = 0; i < N; i++) assets.set(gs[i], { pulse: pulse(i) }); }\napp.addSystems(Update, write);\napp.scene(\"s\", () => (<>{cam}<For each={rows} key={(i: number) => `${i}`}>{(i: number) => <entity><Transform translation={at(i)} /><mesh shape=\"cuboid\" /><material asset={gs[i]} /></entity>}</For></>));\n",
        "rekey" => "type Row = { i: number; pulse: number };\nexport const [rows, setRows] = signal<Row[]>(Array.from({ length: N }, (_, i) => ({ i, pulse: pulse(i) })));\nexport function write() { t += 1; setRows(Array.from({ length: N }, (_, i) => ({ i, pulse: pulse(i) }))); }\napp.addSystems(Update, write);\napp.scene(\"s\", () => (<>{cam}<For each={rows} key={(r: Row) => `${r.i}-${r.pulse}`}>{(r: Row) => <entity><Transform translation={at(r.i)} /><mesh shape=\"cuboid\" /><material kind={Glow} color=\"#ff8800\" pulse={r.pulse} /></entity>}</For></>));\n",
        other => panic!("arm is set or rekey, not {other}"),
    };
    head + body
}

#[derive(Resource, Clone)]
struct Target(Handle<Image>);

fn main() {
    let mut args = std::env::args().skip(1);
    let arm = args.next().expect("usage: material-params-bench <set|rekey> <n>");
    let n: usize = args.next().and_then(|s| s.parse().ok()).expect("usage: material-params-bench <set|rekey> <n>");
    let dir = std::env::temp_dir().join(format!("material-params-bench-{arm}-{n}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("game.tsx"), game(&arm, n)).unwrap();
    std::fs::write(dir.join("glow.wgsl"), WGSL).unwrap();
    let entry = dir.join("game.tsx");

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins((bevy_ts_tsx::TsxPlugin, ScriptMaterialPlugin))
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(DevSource(Arc::new(Compiler::new(&entry, true).unwrap())));
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let mut image = Image::new_target_texture(640, 360, TextureFormat::Rgba8UnormSrgb, None);
    image.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.insert_resource(Target(target));
    app.add_observer(|add: On<Add<Camera>>, target: Res<Target>, mut commands: Commands| {
        commands.entity(add.entity).insert(RenderTarget::Image(target.0.clone().into()));
    });
    let r = bevy_ts::generation::reload_now(app.world_mut(), false).unwrap();
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);

    for _ in 0..WARMUP {
        app.update();
    }
    let pipelines_before = pipelines(&app);
    let mut times = Vec::with_capacity(FRAMES);
    let (mut assets_max, mut prepares) = (0usize, 0usize);
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        times.push(t.elapsed().as_secs_f64() * 1e3);
        assets_max = assets_max.max(app.world().resource::<Assets<ScriptMaterial>>().len());
        prepares += app
            .world()
            .resource::<Messages<AssetEvent<ScriptMaterial>>>()
            .iter_current_update_messages()
            .filter(|e| matches!(e, AssetEvent::Added { .. } | AssetEvent::Modified { .. }))
            .count();
    }
    let mean = times.iter().sum::<f64>() / FRAMES as f64;
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let p95 = sorted[FRAMES * 95 / 100];
    let errors: Vec<String> = app.world().resource::<bevy_ts::log::ScriptLog>().since(0).filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message.clone()).take(3).collect();
    println!(
        "bench: {{\"arm\":\"{arm}\",\"n\":{n},\"mean_ms\":{mean:.3},\"p95_ms\":{p95:.3},\"assets_max\":{assets_max},\"prepares_per_frame\":{:.2},\"pipelines\":[{pipelines_before},{}],\"errors\":{:?}}}",
        prepares as f64 / FRAMES as f64,
        pipelines(&app),
        errors
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn pipelines(app: &App) -> usize {
    app.sub_app(RenderApp).world().resource::<PipelineCache>().pipelines().count()
}
