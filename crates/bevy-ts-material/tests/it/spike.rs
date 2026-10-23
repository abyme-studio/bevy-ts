//! Task 1.1: two script-material kinds with different bind group layouts behind the one
//! `ScriptMaterial` allocator, non-bindless, drawn side by side into an offscreen image.

use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_material::layout::{Base, Bindings, ParamType, struct_name};
use bevy_ts_material::{Kind, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};
use serde_json::json;
use std::sync::{Arc, Mutex};

const W: u32 = 320;
const H: u32 = 180;

fn kind(world: &mut World, name: &str, params: &[(&str, ParamType)], textures: &[&str], body: &str) -> Arc<Kind> {
    let textures: Vec<String> = textures.iter().map(|t| t.to_string()).collect();
    let bindings = Bindings::new(name, params.iter().map(|(n, t)| (*n, t)), &textures).unwrap();
    let src = format!("import bevy_pbr::render::forward_io::VertexOutput;\n\n{body}\n{}", bindings.wgsl(name));
    let path = format!("bevy_ts_material/spike/{}.wesl", struct_name(name).to_lowercase());
    let shader = world.resource_mut::<Assets<Shader>>().add(Shader::from_wesl(src, path));
    Arc::new(Kind::new(name, bindings, Some(shader), None, AlphaMode::Opaque))
}

fn instance(kind: &Arc<Kind>, values: serde_json::Value) -> ScriptMaterial {
    let mut data = kind.bindings.params().defaults();
    for (k, v) in values.as_object().unwrap() {
        kind.bindings.params().encode(k, v, &mut data).unwrap();
    }
    ScriptMaterial { kind: kind.clone(), data, textures: vec![None; kind.bindings.textures().len()] }
}

#[test]
fn two_layouts_behind_one_allocator() {
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

    // A: one vec4 at binding 0. B: a vec3 and a scalar at 0, a texture and sampler at 1 and 2.
    let red = kind(
        world,
        "spike::Red",
        &[("color", ParamType::Base(Base::Color))],
        &[],
        "@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> { return params.color; }",
    );
    let green = kind(
        world,
        "spike::Tinted",
        &[("tint", ParamType::Base(Base::Vec3f)), ("k", ParamType::Base(Base::F32))],
        &["tex"],
        "@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    let t = textureSample(tex, tex_sampler, in.uv).rgb;\n    return vec4<f32>(params.tint * params.k * t, 1.0);\n}",
    );
    assert_ne!(red.layout.entries.len(), green.layout.entries.len());
    let a = world.resource_mut::<Assets<ScriptMaterial>>().add(instance(&red, json!({ "color": "#ff0000" })));
    let b = world.resource_mut::<Assets<ScriptMaterial>>().add(instance(&green, json!({ "tint": [0, 2, 0], "k": 0.5 })));

    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(1.5, 1.5, 1.5));
    world.spawn((Camera3d::default(), Tonemapping::None, Msaa::Off, RenderTarget::Image(target.clone().into()), Transform::from_xyz(0.0, 0.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y)));
    world.spawn((Mesh3d(cube.clone()), ScriptMaterial3d(a), Transform::from_xyz(-1.5, 0.0, 0.0)));
    world.spawn((Mesh3d(cube), ScriptMaterial3d(b), Transform::from_xyz(1.5, 0.0, 0.0)));

    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let px = |img: &Image, x: f32| {
        let (x, y) = ((x * W as f32) as usize, (H / 2) as usize);
        let i = (y * W as usize + x) * 4;
        let d = img.data.as_ref().unwrap();
        [d[i], d[i + 1], d[i + 2]]
    };
    let mut last = None;
    for frame in 0..900 {
        app.update();
        if frame % 20 == 10 {
            let s = shot.clone();
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                *s.lock().unwrap() = Some(e.image.clone());
            });
        }
        if let Some(img) = shot.lock().unwrap().take() {
            let (l, r) = (px(&img, 0.3), px(&img, 0.7));
            last = Some((l, r));
            if l == [255, 0, 0] && r == [0, 255, 0] {
                return;
            }
        }
    }
    panic!("left and right pixels never became red and green: {last:?}");
}

