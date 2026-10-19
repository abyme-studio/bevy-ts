//! `bevy-ts`: the precompiled engine binary.

mod headless;

use bevy::prelude::*;
use bevy_ts::backend::{CompiledModule, ModuleSet};
use bevy_ts::generation::{DevChannel, DevEvent};
use bevy_ts::BevyTsPlugin;
use bevy_ts_v8::V8Options;
use std::time::Duration;

const SKELETON_JS: &str = r#"
import { app } from "bevy-ts";
import { Transform, Mesh3d } from "bevy-ts/bevy";

app.system("bob", {
  stage: "update",
  write: [Transform],
  with: [Mesh3d],
  run(q, ctx) {
    q.each((e, t) => {
      t.translation.y = Math.sin(ctx.elapsed * 2) * 1.5;
    });
  },
});
"#;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let headless = args.iter().any(|a| a == "--headless");
    match args.first().map(String::as_str) {
        Some("skeleton") => skeleton(headless),
        _ => {
            eprintln!("usage: bevy-ts skeleton [--headless]");
            std::process::exit(2);
        }
    }
}

/// The walking skeleton: a cube spawned in Rust, moved by one JS system through the zero-copy
/// `Transform` column, with a Rust `Changed<Transform>` system watching.
fn skeleton(headless: bool) {
    let (tx, channel) = DevChannel::new();
    let path = std::env::current_dir().unwrap().join("skeleton.js");
    let set = ModuleSet { entry: path.clone(), modules: vec![CompiledModule { path, code: SKELETON_JS.into(), source_map: None }] };
    tx.send(DevEvent::Compiled(Ok(set), Duration::ZERO)).unwrap();

    let mut app = App::new();
    app.add_plugins(headless::default_plugins(headless));
    if headless {
        app.add_plugins(headless::HeadlessPlugin);
    }
    app
        .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .insert_resource(channel)
        .add_systems(Startup, scene)
        .add_systems(PostUpdate, report_changed.after(bevy_ts::BevyTsSet::PostUpdate))
        .add_systems(Update, headless::screenshot_on_request)
        .run();
}


fn scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((Camera3d::default(), Transform::from_xyz(-3.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight::default(), Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::ONE))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Srgba::hex("#ff8800").unwrap().into(), ..default() })),
        Transform::default(),
    ));
}

fn report_changed(q: Query<(Entity, &Transform), Changed<Transform>>, time: Res<Time>, mut last: Local<f32>) {
    if time.elapsed_secs() - *last >= 1.0 {
        *last = time.elapsed_secs();
        let changed: Vec<_> = q.iter().map(|(e, t)| format!("{e} y={:.2}", t.translation.y)).collect();
        info!("Changed<Transform> this frame: {} entities {changed:?}", changed.len());
    }
}
