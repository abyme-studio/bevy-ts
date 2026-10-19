//! The engine binary's command line, shared by the prebuilt binary and every project engine.
//!
//! `bevy-ts dev <entry.ts> [--headless] [--remote-port <port>] [--inspect[=<port>]]`

use bevy::prelude::*;
use bevy_ts::backend::{CompiledModule, ModuleSet};
use bevy_ts::generation::{DevChannel, DevEvent, DevSource};
use bevy_ts::runtime::RuntimeInfo;
use bevy_ts::BevyTsPlugin;
use crate::compile::Compiler;
use crate::{dev, headless};
use bevy_ts::engine::EngineIdentity;
use bevy_ts_v8::V8Options;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "usage:
  bevy-ts dev <entry.ts> [--headless] [--remote-port <port>] [--inspect[=<port>]]
  bevy-ts build [<project dir>]
  bevy-ts types [--out <file>]
  bevy-ts skeleton [--headless]";

/// Parsed `bevy-ts dev` options.
#[derive(Debug, Clone)]
pub struct DevArgs {
    pub entry: String,
    pub headless: bool,
    pub remote_port: u16,
    pub inspect: Option<SocketAddr>,
}

fn parse_dev(args: &[String]) -> Result<DevArgs, String> {
    let mut entry = None;
    let mut out = DevArgs { entry: String::new(), headless: false, remote_port: 15702, inspect: None };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--headless" => out.headless = true,
            "--remote-port" => {
                out.remote_port = it.next().and_then(|p| p.parse().ok()).ok_or("--remote-port takes a port")?;
            }
            "--inspect" => out.inspect = Some(SocketAddr::from(([127, 0, 0, 1], 9229))),
            s if s.starts_with("--inspect=") => {
                let v = &s["--inspect=".len()..];
                out.inspect = Some(match v.parse::<u16>() {
                    Ok(port) => SocketAddr::from(([127, 0, 0, 1], port)),
                    Err(_) => v.parse().map_err(|_| format!("--inspect={v}: expected a port or host:port"))?,
                });
            }
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s if entry.is_none() => entry = Some(s.to_string()),
            s => return Err(format!("unexpected argument {s}")),
        }
    }
    out.entry = entry.ok_or("missing <entry.ts>")?;
    Ok(out)
}

/// No extra plugins: what the prebuilt binary passes to [`run`].
pub struct Prebuilt;

impl Plugin for Prebuilt {
    fn build(&self, _: &mut App) {}
}

/// The engine entry. The prebuilt binary calls `run(Prebuilt)`; a project engine's generated
/// `main` calls [`run_engine`] with its identity and a plugin adding the project's, the adopted
/// crates' and the ports' plugins.
pub fn run(extra: impl Plugin) {
    run_engine(EngineIdentity::prebuilt(crate::engine::VERSION, crate::engine::TARGET), extra)
}

/// [`run`] with the engine's identity, for project engines.
pub fn run_engine(identity: EngineIdentity, extra: impl Plugin) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let engine = Engine { identity };
    match args.first().map(String::as_str) {
        Some("dev") => match parse_dev(&args[1..]) {
            Ok(a) => {
                if engine.identity.kind == bevy_ts::engine::EngineKind::Prebuilt
                    && let Some(code) = crate::supervise::dev_in_project(&a, &args[1..])
                {
                    std::process::exit(code);
                }
                run_dev(&engine, extra, a)
            }
            Err(e) => {
                eprintln!("bevy-ts dev: {e}\n{USAGE}");
                std::process::exit(2);
            }
        },
        Some("build") => std::process::exit(crate::supervise::build(args.get(1).map(String::as_str))),
        Some("types") => {
            if engine.identity.kind == bevy_ts::engine::EngineKind::Prebuilt
                && let Some(code) = crate::supervise::types_in_project(&args[1..])
            {
                std::process::exit(code);
            }
            std::process::exit(crate::types::run(&engine, extra, &args[1..]))
        }
        Some("skeleton") => skeleton(&engine, extra, args.iter().any(|a| a == "--headless")),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

/// What the running binary is.
pub struct Engine {
    pub identity: EngineIdentity,
}

/// Bevy's default plugins, bevy-ts and tsx, then `extra` (the project's plugins), so they can
/// order against `BevyTsSet` and see bevy-ts's resources.
pub fn base_app(engine: &Engine, extra: impl Plugin, headless_mode: bool, options: V8Options) -> App {
    let mut app = App::new();
    app.add_plugins(headless::default_plugins(headless_mode));
    if headless_mode {
        app.add_plugins(headless::HeadlessPlugin);
    }
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(options)).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_systems(Update, headless::screenshot_on_request)
        .insert_resource(engine.identity.clone())
        .add_plugins(extra);
    app
}

fn run_dev(engine: &Engine, extra: impl Plugin, args: DevArgs) {
    let compiler = match Compiler::new(&args.entry, true) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("bevy-ts dev: {}: {e}", args.entry);
            std::process::exit(1);
        }
    };
    let (tx, channel) = DevChannel::new();
    dev::spawn_watcher(compiler.clone(), tx);

    let mut app = base_app(engine, extra, args.headless, V8Options { inspect: args.inspect });
    app.insert_resource(channel).insert_resource(DevSource(compiler.clone()));
    app.add_plugins(bevy_ts_remote::BevyTsRemotePlugin { port: args.remote_port });
    {
        let mut info = app.world_mut().resource_mut::<RuntimeInfo>();
        info.entry = Some(compiler.entry.clone());
    }
    app.run();
}

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

/// The walking skeleton: a cube spawned in Rust, moved by one JS system through the zero-copy
/// `Transform` column, with a Rust `Changed<Transform>` system watching.
fn skeleton(engine: &Engine, extra: impl Plugin, headless_mode: bool) {
    let (tx, channel) = DevChannel::new();
    let path = std::env::current_dir().unwrap().join("skeleton.js");
    let set = ModuleSet { entry: path.clone(), modules: vec![CompiledModule { path, code: SKELETON_JS.into(), source_map: None }] };
    tx.send(DevEvent::Compiled(Ok(set), Duration::ZERO)).unwrap();
    let mut app = base_app(engine, extra, headless_mode, V8Options::default());
    app.insert_resource(channel)
        .add_systems(Startup, scene)
        .add_systems(PostUpdate, report_changed.after(bevy_ts::BevyTsSet::PostUpdate))
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
