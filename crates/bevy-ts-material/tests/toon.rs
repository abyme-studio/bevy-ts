//! Script materials in the prepass and the shadow pass (script-materials, "Materials draw in the
//! prepass and the shadow pass"), through the real `ScriptMaterial` path on a GPU, headless.
//! One small scene per panel under an orthographic camera: a ground, and a cube floating above it.
//! - toon: the cube's shadow darkens the ground, read in the ground's own fragment shader.
//! - ink: the same, plus ink from the depth and normal prepass along the cube's silhouette.
//! - no shadows: the cube's kind says `shadows: false`, so the ground under it stays lit.
//!
//! `TOON_OUT=<dir>` also writes each panel as raw RGBA, 960x720.

use bevy::camera::{RenderTarget, ScalingMode};
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_material::layout::{Base, Bindings, ParamType, struct_name};
use bevy_ts_material::{Kind, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};
use serde_json::json;
use std::sync::{Arc, Mutex};

const W: u32 = 960;
const H: u32 = 720;

/// The docs' toon shader (docs/materials.md, "Toon and ink"): two soft cel steps over the sun's
/// shadowed N·L, a cool shade colour, and ink where the prepass jumps.
const TOON: &str = r#"
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::{view, lights};
import bevy_pbr::render::shadows::fetch_directional_shadow;
INK_IMPORT

fn lit(in: VertexOutput) -> vec3<f32> {
    let n = normalize(in.world_normal);
    let sun = lights.directional_lights[0];
    let ndl = dot(n, sun.direction_to_light);
    let view_z = dot(vec4<f32>(view.view_from_world[0].z, view.view_from_world[1].z, view.view_from_world[2].z, view.view_from_world[3].z), in.world_position);
    let shadow = fetch_directional_shadow(0u, in.world_position, n, view_z, in.position.xy);
    let l = max(ndl, 0.0) * shadow;
    let b = 0.5 * smoothstep(0.08, 0.14, l) + 0.5 * smoothstep(0.55, 0.61, l);
    return mix(params.color.rgb * params.shade.rgb, params.color.rgb, b);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var c = lit(in);
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
        let dd = abs(prepass_depth(q, 0u) - d0) * params.depth_k;
        let dn = 1.0 - dot(prepass_normal(q, 0u), n0);
        edge = max(edge, max(smoothstep(0.4, 0.8, dd), smoothstep(0.25, 0.5, dn)));
    }
    c = mix(c, params.ink.rgb, edge * 0.9);
"#;

fn toon_kind(world: &mut World, name: &str, ink: bool, shadows: bool) -> Arc<Kind> {
    let mut params = vec![("color", ParamType::Base(Base::Color)), ("shade", ParamType::Base(Base::Color))];
    if ink {
        params.push(("ink", ParamType::Base(Base::Color)));
        params.push(("depth_k", ParamType::Base(Base::F32)));
    }
    let bindings = Bindings::new(name, params.iter().map(|(n, t)| (*n, t)), &[]).unwrap();
    let body = TOON.replace("INK_IMPORT", if ink { INK_IMPORT } else { "" }).replace("INK_BODY", if ink { INK_BODY } else { "" });
    let src = format!("{body}\n{}", bindings.wgsl(name));
    let path = format!("bevy_ts_material/toon/{}.wesl", struct_name(name).to_lowercase());
    let shader = world.resource_mut::<Assets<Shader>>().add(Shader::from_wesl(src, path));
    let mut kind = Kind::new(name, bindings, Some(shader), None, AlphaMode::Opaque);
    assert!(kind.shadows && kind.prepass, "an opaque kind with no vertex joins both passes by default");
    kind.shadows = shadows;
    Arc::new(kind)
}

fn instance(kind: &Arc<Kind>, values: serde_json::Value) -> ScriptMaterial {
    let mut data = kind.bindings.params().defaults();
    for (k, v) in values.as_object().unwrap() {
        kind.bindings.params().encode(k, v, &mut data).unwrap();
    }
    ScriptMaterial { kind: kind.clone(), data, textures: vec![] }
}

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Toon,
    Ink,
    NoShadows,
}

const PANELS: [Panel; 3] = [Panel::Toon, Panel::Ink, Panel::NoShadows];
const CUBE: Vec3 = Vec3::new(0.0, 3.0, 0.0);
/// Where the sun, from (-4, 10, 3) towards the origin, drops the cube's centre on the ground.
const SHADOW: Vec3 = Vec3::new(1.2, 0.0, -0.9);
const OPEN: Vec3 = Vec3::new(-4.0, 0.0, 4.0);

#[test]
fn toon_materials_cast_shadows_and_read_the_prepass() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(ScriptMaterialRenderPlugin);
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let world = app.world_mut();

    let toon = toon_kind(world, "toon::Toon", false, true);
    let ink = toon_kind(world, "toon::Ink", true, true);
    let shadowless = toon_kind(world, "toon::Shadowless", false, false);
    world.spawn((
        DirectionalLight { illuminance: 8000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-4.0, 10.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: 80.0, first_cascade_far_bound: 80.0, ..default() }.build(),
    ));
    world.insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() });

    let ground_mesh = world.resource_mut::<Assets<Mesh>>().add(Plane3d::default().mesh().size(18.0, 18.0));
    let cube_mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(1.0, 1.0, 1.0));
    let mut targets = vec![];
    let mut cameras = vec![];
    for (i, panel) in PANELS.into_iter().enumerate() {
        let x0 = Vec3::X * (i as f32 - 1.0) * 60.0;
        let (ground_kind, cube_kind) = match panel {
            Panel::Toon => (&toon, &toon),
            Panel::Ink => (&ink, &ink),
            Panel::NoShadows => (&toon, &shadowless),
        };
        let values = |base: &str| match panel {
            Panel::Ink => json!({ "color": base, "shade": "#5a6aa0", "ink": "#2a2030", "depth_k": 400.0 }),
            _ => json!({ "color": base, "shade": "#5a6aa0" }),
        };
        let ground = world.resource_mut::<Assets<ScriptMaterial>>().add(instance(ground_kind, values("#9cc45a")));
        let cube = world.resource_mut::<Assets<ScriptMaterial>>().add(instance(cube_kind, values("#e8d0a0")));
        world.spawn((Mesh3d(ground_mesh.clone()), ScriptMaterial3d(ground), Transform::from_translation(x0)));
        world.spawn((Mesh3d(cube_mesh.clone()), ScriptMaterial3d(cube), Transform::from_translation(x0 + CUBE)));

        let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
        target.asset_usage = bevy::asset::RenderAssetUsages::default();
        let target = world.resource_mut::<Assets<Image>>().add(target);
        let ortho = Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 13.0 },
            ..OrthographicProjection::default_3d()
        });
        let mut cam = world.spawn((
            Camera3d::default(),
            ortho,
            Msaa::Off,
            Tonemapping::None,
            RenderTarget::Image(target.clone().into()),
            Transform::from_translation(x0 + Vec3::new(0.0, 20.0, 14.0)).looking_at(x0, Vec3::Y),
        ));
        if panel == Panel::Ink {
            cam.insert((DepthPrepass, NormalPrepass));
        }
        cameras.push((cam.id(), x0));
        targets.push(target);
    }

    let shots: Arc<Mutex<Vec<Option<Image>>>> = Arc::new(Mutex::new(vec![None; PANELS.len()]));
    for frame in 0..600 {
        app.update();
        if frame == 300 {
            for (i, t) in targets.iter().enumerate() {
                let s = shots.clone();
                app.world_mut().spawn(Screenshot::image(t.clone())).observe(move |e: On<ScreenshotCaptured>| {
                    s.lock().unwrap()[i] = Some(e.image.clone());
                });
            }
        }
        if frame > 300 && shots.lock().unwrap().iter().all(Option::is_some) {
            break;
        }
    }

    let world = app.world_mut();
    let pixel = |world: &mut World, cam: Entity, p: Vec3| -> UVec2 {
        let (camera, at) = world.query::<(&Camera, &GlobalTransform)>().get(world, cam).unwrap();
        camera.world_to_viewport(at, p).unwrap().as_uvec2()
    };
    let shots = shots.lock().unwrap();
    let rgb = |i: usize, p: UVec2| -> [f32; 3] { shots[i].as_ref().expect("screenshot").get_color_at(p.x, p.y).unwrap().to_srgba().to_f32_array_no_alpha() };
    let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    // Ink is mixed in at 90% in linear light, so it reads as dark rather than as the ink colour:
    // lit faces here are above 0.6, ink below 0.3.
    let is_ink = |c: [f32; 3]| luma(c) < 0.4;
    let window = |i: usize, p: UVec2, r: u32| -> Vec<[f32; 3]> {
        let mut out = vec![];
        for y in p.y.saturating_sub(r)..=(p.y + r).min(H - 1) {
            for x in p.x.saturating_sub(r)..=(p.x + r).min(W - 1) {
                out.push(rgb(i, UVec2::new(x, y)));
            }
        }
        out
    };

    if let Ok(out) = std::env::var("TOON_OUT") {
        std::fs::create_dir_all(&out).unwrap();
        for (i, name) in ["toon", "ink", "no-shadows"].iter().enumerate() {
            std::fs::write(format!("{out}/{name}.rgba"), shots[i].as_ref().unwrap().data.as_ref().unwrap()).unwrap();
        }
    }

    for (i, panel) in PANELS.into_iter().enumerate() {
        let (cam, x0) = cameras[i];
        let shadowed = luma(rgb(i, pixel(world, cam, x0 + SHADOW)));
        let open = luma(rgb(i, pixel(world, cam, x0 + OPEN)));
        if panel == Panel::NoShadows {
            assert!((shadowed - open).abs() < 0.02, "shadows: false casts no shadow: {shadowed} vs {open}");
        } else {
            assert!(shadowed < open - 0.1, "panel {i}: the ground under the cube is darker: {shadowed} vs {open}");
        }
        let ground_ink = window(i, pixel(world, cam, x0 + OPEN), 6).into_iter().filter(|c| is_ink(*c)).count();
        assert_eq!(ground_ink, 0, "panel {i}: no ink inside the flat ground");
        // The cube's top front edge, a silhouette against the ground below it.
        let edge_ink = window(i, pixel(world, cam, x0 + CUBE + Vec3::new(0.0, 0.5, 0.5)), 4).into_iter().filter(|c| is_ink(*c)).count();
        if panel == Panel::Ink {
            assert!(edge_ink > 0, "ink along the cube's silhouette");
        } else {
            assert_eq!(edge_ink, 0, "panel {i}: no ink without the prepass");
        }
    }
}
