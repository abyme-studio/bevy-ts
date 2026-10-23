//! What the toon look costs on 100 skinned, animated characters (script-material-passes, task 3.1).
//! Bevy's `Fox.glb`, fetched once into `~/.cache/bevy-ts/` from Bevy's v0.20.0 tag, 100 running
//! foxes in a 10x10 grid on a ground, an orthographic camera, one shadowed sun, 1920x1080
//! offscreen. Three arms:
//! - pbr: the glTF's own `StandardMaterial`, the baseline.
//! - toon: every fox and the ground with a toon script material; shadows, no prepass.
//! - toon+ink: the same, plus `DepthPrepass` and `NormalPrepass`, read by the material for ink.
//!
//! Run in release: `cargo test --release -p bevy-ts-material --test toon_bench -- --ignored --nocapture`.
//! It prints mean and p95 wall time per frame and each render pass's GPU time (Bevy's
//! `RenderDiagnosticsPlugin`), which holds up on a loaded machine; results go in bench/toon/results/.

use bevy::camera::{RenderTarget, ScalingMode};
use bevy::diagnostic::DiagnosticsStore;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_material::layout::{Base, Bindings, ParamType, struct_name};
use bevy_ts_material::{Kind, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const FOX: &str = "https://raw.githubusercontent.com/bevyengine/bevy/v0.20.0/assets/models/animated/Fox.glb";
const WARMUP: usize = 300;
const FRAMES: usize = 600;

/// The toon shader of `toon.rs`, with ink switched by a shader-side constant per kind.
const TOON: &str = r#"
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::{view, lights};
import bevy_pbr::render::shadows::fetch_directional_shadow;
INK_IMPORT

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let sun = lights.directional_lights[0];
    let view_z = dot(vec4<f32>(view.view_from_world[0].z, view.view_from_world[1].z, view.view_from_world[2].z, view.view_from_world[3].z), in.world_position);
    let l = max(dot(n, sun.direction_to_light), 0.0) * fetch_directional_shadow(0u, in.world_position, n, view_z, in.position.xy);
    let b = 0.5 * smoothstep(0.08, 0.14, l) + 0.5 * smoothstep(0.55, 0.61, l);
    var c = mix(params.color.rgb * params.shade.rgb, params.color.rgb, b);
    INK_BODY
    return vec4<f32>(c, 1.0);
}
"#;

const INK_IMPORT: &str = "import bevy_pbr::prepass::utils::{prepass_depth, prepass_normal};";
const INK_BODY: &str = r#"
    let p = in.position;
    let d0 = prepass_depth(p, 0u);
    let n0 = prepass_normal(p, 0u);
    var edge = 0.0;
    let offs = array<vec2<f32>, 4>(vec2(1.5, 0.0), vec2(-1.5, 0.0), vec2(0.0, 1.5), vec2(0.0, -1.5));
    for (var i = 0; i < 4; i++) {
        let q = vec4<f32>(clamp(p.xy + offs[i], view.viewport.xy + 0.5, view.viewport.xy + view.viewport.zw - 0.5), p.zw);
        let dd = abs(prepass_depth(q, 0u) - d0) * 400.0;
        let dn = 1.0 - dot(prepass_normal(q, 0u), n0);
        edge = max(edge, max(smoothstep(0.4, 0.8, dd), smoothstep(0.25, 0.5, dn)));
    }
    c = mix(c, vec3<f32>(0.16, 0.13, 0.19), edge * 0.9);
"#;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Arm {
    Pbr,
    Toon,
    ToonInk,
}

#[derive(Resource)]
struct Toon(Option<(Handle<ScriptMaterial>, Handle<ScriptMaterial>)>);

#[derive(Resource)]
struct Run(Handle<AnimationGraph>, AnimationNodeIndex);

fn fox() -> PathBuf {
    let dir = std::env::var_os("HOME").map(PathBuf::from).unwrap().join(".cache/bevy-ts");
    let path = dir.join("Fox.glb");
    if !path.exists() {
        std::fs::create_dir_all(&dir).unwrap();
        let ok = std::process::Command::new("curl").args(["-fsSL", "-o"]).arg(&path).arg(FOX).status().unwrap().success();
        assert!(ok, "could not fetch {FOX}");
    }
    dir
}

fn kind(world: &mut World, name: &str, ink: bool) -> Arc<Kind> {
    let params = [("color", ParamType::Base(Base::Color)), ("shade", ParamType::Base(Base::Color))];
    let bindings = Bindings::new(name, params.iter().map(|(n, t)| (*n, t)), &[]).unwrap();
    let body = TOON.replace("INK_IMPORT", if ink { INK_IMPORT } else { "" }).replace("INK_BODY", if ink { INK_BODY } else { "" });
    let src = format!("{body}\n{}", bindings.wgsl(name));
    let path = format!("bevy_ts_material/bench/{}.wesl", struct_name(name).to_lowercase());
    let shader = world.resource_mut::<Assets<Shader>>().add(Shader::from_wesl(src, path));
    Arc::new(Kind::new(name, bindings, Some(shader), None, AlphaMode::Opaque))
}

fn material(kind: &Arc<Kind>, color: &str) -> ScriptMaterial {
    let mut data = kind.bindings.params().defaults();
    kind.bindings.params().encode("color", &json!(color), &mut data).unwrap();
    kind.bindings.params().encode("shade", &json!("#5a6aa0"), &mut data).unwrap();
    ScriptMaterial { kind: kind.clone(), data, textures: vec![] }
}

/// Puts the toon material on every glTF primitive as it spawns.
fn swap(mut commands: Commands, toon: Res<Toon>, meshes: Query<Entity, With<MeshMaterial3d<StandardMaterial>>>) {
    let Some((fox, _)) = &toon.0 else { return };
    for e in &meshes {
        commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert(ScriptMaterial3d(fox.clone()));
    }
}

fn run(mut commands: Commands, anim: Res<Run>, mut players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>) {
    for (i, (e, mut player)) in players.iter_mut().enumerate() {
        // Each fox starts at a different point of its run, as a crowd would.
        player.play(anim.1).repeat().seek_to(i as f32 * 0.037);
        commands.entity(e).insert(AnimationGraphHandle(anim.0.clone()));
    }
}

fn arm(arm: Arm) -> (f64, f64, Vec<(String, f64)>) {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: fox().to_string_lossy().into(), ..default() }),
    )
    .add_plugins((ScriptMaterialRenderPlugin, RenderDiagnosticsPlugin))
    .add_systems(Update, (swap, run));
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let world = app.world_mut();

    let toon = (arm != Arm::Pbr).then(|| {
        let k = kind(world, &format!("bench::{arm:?}"), arm == Arm::ToonInk);
        let mut mats = world.resource_mut::<Assets<ScriptMaterial>>();
        (mats.add(material(&k, "#e08a40")), mats.add(material(&k, "#9cc45a")))
    });
    let server = world.resource::<AssetServer>().clone();
    let clip = server.load(GltfAssetLabel::Animation(2).from_asset("Fox.glb"));
    let (graph, node) = AnimationGraph::from_clip(clip);
    let graph = world.resource_mut::<Assets<AnimationGraph>>().add(graph);
    world.insert_resource(Run(graph, node));
    let scene: Handle<bevy::world_serialization::WorldAsset> = server.load(GltfAssetLabel::Scene(0).from_asset("Fox.glb"));
    for i in 0..100 {
        let (x, z) = ((i % 10) as f32 - 4.5, (i / 10) as f32 - 4.5);
        world.spawn((WorldAssetRoot(scene.clone()), Transform::from_xyz(x * 4.0, 0.0, z * 4.0).with_scale(Vec3::splat(0.02))));
    }
    let ground = world.resource_mut::<Assets<Mesh>>().add(Plane3d::default().mesh().size(50.0, 50.0));
    let mut g = world.spawn((Mesh3d(ground), Transform::default()));
    match &toon {
        Some((_, mat)) => g.insert(ScriptMaterial3d(mat.clone())),
        None => {
            let m = g.world_scope(|w| w.resource_mut::<Assets<StandardMaterial>>().add(Color::srgb_u8(0x9c, 0xc4, 0x5a)));
            g.insert(MeshMaterial3d(m))
        }
    };
    world.insert_resource(Toon(toon));
    world.spawn((
        DirectionalLight { illuminance: 8000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-4.0, 10.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: 120.0, first_cascade_far_bound: 120.0, ..default() }.build(),
    ));

    let mut target = Image::new_target_texture(1920, 1080, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    let mut cam = world.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 44.0 },
            ..OrthographicProjection::default_3d()
        }),
        Msaa::Off,
        RenderTarget::Image(target.into()),
        Transform::from_xyz(0.0, 40.0, 28.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    if arm != Arm::Pbr {
        cam.insert(Tonemapping::None);
    }
    if arm == Arm::ToonInk {
        cam.insert((DepthPrepass, NormalPrepass));
    }

    for _ in 0..WARMUP {
        app.update();
    }
    let world = app.world_mut();
    let players = world.query::<&AnimationPlayer>().iter(world).count();
    assert_eq!(players, 100, "every fox loaded and animating");
    if arm != Arm::Pbr {
        let left = world.query_filtered::<(), With<MeshMaterial3d<StandardMaterial>>>().iter(world).count();
        assert_eq!(left, 0, "every fox primitive is toon");
    }
    let mut times: Vec<f64> = (0..FRAMES)
        .map(|_| {
            let t = Instant::now();
            app.update();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    let store = app.world().resource::<DiagnosticsStore>();
    let mut gpu: Vec<(String, f64)> = store
        .iter()
        .filter(|d| d.path().as_str().ends_with("/elapsed_gpu"))
        .filter_map(|d| Some((d.path().as_str().trim_end_matches("/elapsed_gpu").to_string(), d.average()?)))
        .collect();
    gpu.sort_by(|a, b| a.0.cmp(&b.0));
    (times.iter().sum::<f64>() / times.len() as f64, times[times.len() * 95 / 100], gpu)
}

#[test]
#[ignore = "a release bench; it fetches Fox.glb and renders 100 skinned characters"]
fn hundred_foxes() {
    for a in [Arm::Pbr, Arm::Toon, Arm::ToonInk] {
        let (mean, p95, gpu) = arm(a);
        println!("{a:?}: mean {mean:.2} ms, p95 {p95:.2} ms over {FRAMES} frames");
        for (pass, ms) in gpu {
            println!("{a:?} gpu {pass}: {ms:.3} ms");
        }
    }
}
