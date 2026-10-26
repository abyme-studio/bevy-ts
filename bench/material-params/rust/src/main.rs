//! One run of one arm: `material-params-bench <arm> <n> [frames]`. N cubes drawn offscreen, a
//! value of each changed every frame:
//! - `set` (4.2): N owned instances (`Glow.add`), each written with `assets.set` by a system;
//! - `rekey` (4.2): N `<material kind={Glow} pulse={...} />` rows of a `<For>`, keyed by their
//!   pulse, so each frame's value is a new key (the only way to change a param from TSX before
//!   this change);
//! - `instance` (4.3): N `<material kind={Hover} hover={() => wave(i)} />`, one keyed asset, the
//!   bound instance param written to each entity's `ScriptInstance`;
//! - `instance-rekey` (4.3): `rekey` laid out as `instance` is, its pulse following the same wave.
//!
//! Prints one line, `bench: ` and JSON: mean and p95 frame time (`app.update()`, pipelined
//! rendering off, so the render world's work is in it), the most `ScriptMaterial` assets alive,
//! material prepares per frame (`AssetEvent` added or modified: each makes a uniform buffer and a
//! bind group), and the bytes of the instance buffer at the end.

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


const HOVER: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return params.color * instance(in.instance_index).hover;\n}\n";

const WGSL: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return params.color * params.pulse;\n}\n";

fn game(arm: &str, n: usize) -> String {
    let head = format!(
        "import {{ app, assets, signal, For, Update }} from \"bevy-ts\";\nimport {{ Camera3d, Transform }} from \"bevy-ts/bevy\";\nimport {{ color, f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ color: color, pulse: f32 }}, fragment: glow }});\nconst N = {n};\nconst at = (i: number): [number, number, number] => [(i % 10) * 3 - 13.5, Math.floor(i / 10) * 3 - 13.5, 0];\nlet t = 0;\n// Never repeats, so the keyed cache holds no row's next value already.\nconst pulse = (i: number) => 0.5 + t / 10000 + i / 1000000;\nconst cam = <entity name=\"cam\"><Camera3d /><Transform translation={{[0, 0, 40]}} /></entity>;\n"
    );
    let grid = format!(
        "import {{ app, signal, For, Update }} from \"bevy-ts\";\nimport {{ Camera3d, Transform }} from \"bevy-ts/bevy\";\nimport {{ color, f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\nimport hover from \"./hover.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ color: color, pulse: f32 }}, fragment: glow }});\nexport const Hover = material(\"game::Hover\", {{ params: {{ color: color }}, instance: {{ hover: f32 }}, fragment: hover }});\nconst N = {n};\nconst side = Math.ceil(Math.sqrt(N));\nconst at = (i: number): [number, number, number] => [(i % side) * 3 - side * 1.5, Math.floor(i / side) * 3 - side * 1.5, 0];\nexport const [t, setT] = signal(0);\nexport function tick() {{ setT(t() + 1); }}\napp.addSystems(Update, tick);\n// Never repeats, so the keyed cache holds no row's next value already.\nconst wave = (i: number, now: number) => 0.5 + 0.5 * Math.sin(now / 10 + i) + now / 1e6;\nconst cam = <entity name=\"cam\"><Camera3d /><Transform translation={{[0, 0, side * 4]}} /></entity>;\nexport const [rows] = signal(Array.from({{ length: N }}, (_, i) => i));\n"
    );
    let body = match arm {
        "instance" => return grid + "app.scene(\"s\", () => (<>{cam}<For each={rows} key={(i: number) => `${i}`}>{(i: number) => <entity><Transform translation={at(i)} /><mesh shape=\"cuboid\" /><material kind={Hover} color=\"#ff8800\" hover={() => wave(i, t())} /></entity>}</For></>));\n",
        "instance-rekey" => {
            return grid
                + "type Row = { i: number; pulse: number };\nconst pulses = () => Array.from({ length: N }, (_, i) => ({ i, pulse: wave(i, t()) }));\napp.scene(\"s\", () => (<>{cam}<For each={pulses} key={(r: Row) => `${r.i}-${r.pulse}`}>{(r: Row) => <entity><Transform translation={at(r.i)} /><mesh shape=\"cuboid\" /><material kind={Glow} color=\"#ff8800\" pulse={r.pulse} /></entity>}</For></>));\n";
        }
        "set" => "const gs = Array.from({ length: N }, (_, i) => Glow.add({ color: \"#ff8800\", pulse: pulse(i) }));\nexport const [rows] = signal(Array.from({ length: N }, (_, i) => i));\nexport function write() { t += 1; for (let i = 0; i < N; i++) assets.set(gs[i], { pulse: pulse(i) }); }\napp.addSystems(Update, write);\napp.scene(\"s\", () => (<>{cam}<For each={rows} key={(i: number) => `${i}`}>{(i: number) => <entity><Transform translation={at(i)} /><mesh shape=\"cuboid\" /><material asset={gs[i]} /></entity>}</For></>));\n",
        "rekey" => "type Row = { i: number; pulse: number };\nexport const [rows, setRows] = signal<Row[]>(Array.from({ length: N }, (_, i) => ({ i, pulse: pulse(i) })));\nexport function write() { t += 1; setRows(Array.from({ length: N }, (_, i) => ({ i, pulse: pulse(i) }))); }\napp.addSystems(Update, write);\napp.scene(\"s\", () => (<>{cam}<For each={rows} key={(r: Row) => `${r.i}-${r.pulse}`}>{(r: Row) => <entity><Transform translation={at(r.i)} /><mesh shape=\"cuboid\" /><material kind={Glow} color=\"#ff8800\" pulse={r.pulse} /></entity>}</For></>));\n",
        other => panic!("arm is set, rekey, instance or instance-rekey, not {other}"),
    };
    head + body
}

#[derive(Resource, Clone)]
struct Target(Handle<Image>);

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: material-params-bench <set|rekey|instance|instance-rekey> <n> [frames, 600]";
    let arm = args.next().expect(usage);
    let n: usize = args.next().and_then(|s| s.parse().ok()).expect(usage);
    let frames: usize = args.next().map_or(600, |s| s.parse().expect(usage));
    let warmup = frames / 5;
    let dir = std::env::temp_dir().join(format!("material-params-bench-{arm}-{n}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("game.tsx"), game(&arm, n)).unwrap();
    std::fs::write(dir.join("glow.wgsl"), WGSL).unwrap();
    std::fs::write(dir.join("hover.wgsl"), HOVER).unwrap();
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

    for _ in 0..warmup {
        app.update();
    }
    let pipelines_before = pipelines(&app);
    let mut times = Vec::with_capacity(frames);
    let (mut assets_max, mut prepares) = (0usize, 0usize);
    for _ in 0..frames {
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
    let mean = times.iter().sum::<f64>() / frames as f64;
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let p95 = sorted[frames * 95 / 100];
    let buffer_bytes = bevy_ts_material::instance::buffer(app.world())
        .and_then(|h| app.world().resource::<Assets<bevy::render::storage::ShaderBuffer>>().get(&h).map(|b| b.buffer_size()))
        .unwrap_or(0);
    let errors: Vec<String> = app.world().resource::<bevy_ts::log::ScriptLog>().since(0).filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message.clone()).take(3).collect();
    println!(
        "bench: {{\"arm\":\"{arm}\",\"n\":{n},\"mean_ms\":{mean:.3},\"p95_ms\":{p95:.3},\"assets_max\":{assets_max},\"prepares_per_frame\":{:.2},\"pipelines\":[{pipelines_before},{}],\"buffer_bytes\":{buffer_bytes},\"frames\":{frames},\"errors\":{:?}}}",
        prepares as f64 / frames as f64,
        pipelines(&app),
        errors
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn pipelines(app: &App) -> usize {
    app.sub_app(RenderApp).world().resource::<PipelineCache>().pipelines().count()
}
