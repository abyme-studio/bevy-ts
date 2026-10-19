//! The engine binary's command line, shared by the prebuilt binary and every project engine.
//!
//! `bevy-ts dev <entry.ts> [--headless | --viewport] [--remote-port <port>] [--inspect[=<port>]]`

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
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "usage:
  bevy-ts dev <entry.ts> [--headless | --viewport] [--remote-port <port>] [--inspect[=<port>]]
  bevy-ts build [<project dir>]
  bevy-ts types [--out <file>]
  bevy-ts transform <file.ts> [--dev]
  bevy-ts skeleton [--headless]
  bevy-ts levels check [--upstream] <file.bsn>…";

/// Parsed `bevy-ts dev` options.
#[derive(Debug, Clone)]
pub struct DevArgs {
    pub entry: String,
    pub headless: bool,
    /// No window of its own: render for a viewer that attaches over the protocol (macOS).
    pub viewport: bool,
    pub remote_port: u16,
    pub inspect: Option<SocketAddr>,
}

fn parse_dev(args: &[String]) -> Result<DevArgs, String> {
    let mut entry = None;
    let mut out = DevArgs { entry: String::new(), headless: false, viewport: false, remote_port: 15702, inspect: None };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--headless" => out.headless = true,
            "--viewport" => out.viewport = true,
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
    if out.headless && out.viewport {
        return Err("--headless and --viewport cannot be combined".into());
    }
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
        Some("levels") if args.get(1).map(String::as_str) == Some("check") => {
            std::process::exit(bevy_ts_levels::check::run(&args[2..]))
        }
        Some("transform") => match args.get(1) {
            Some(file) => match crate::compile::transform(std::path::Path::new(file), args.iter().any(|a| a == "--dev")) {
                Ok(code) => print!("{code}"),
                Err(errors) => {
                    for e in errors {
                        eprintln!("{e}");
                    }
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        },
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

/// How the engine shows its frames.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Display {
    /// A window of its own.
    Window,
    /// An offscreen image, no window.
    Headless,
    /// IOSurfaces a viewer shows, no window (`--viewport`, macOS).
    Viewport,
}

/// Bevy's default plugins, bevy-ts and tsx, then `extra` (the project's plugins), so they can
/// order against `BevyTsSet` and see bevy-ts's resources. `assets` is Bevy's asset root, the
/// project's `assets/` under `bevy-ts dev`; `None` keeps Bevy's default.
pub fn base_app(engine: &Engine, extra: impl Plugin, display: Display, options: V8Options, assets: Option<&Path>) -> App {
    let mut app = App::new();
    let plugins = headless::default_plugins(display != Display::Window);
    match assets {
        Some(root) => app.add_plugins(plugins.set(headless::asset_plugin(root))),
        None => app.add_plugins(plugins),
    };
    match display {
        Display::Headless => {
            app.add_plugins(headless::HeadlessPlugin);
        }
        #[cfg(target_os = "macos")]
        Display::Viewport => {
            app.add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: false });
        }
        #[cfg(target_os = "macos")]
        Display::Window => {
            app.add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: true });
        }
        #[cfg(not(target_os = "macos"))]
        _ => {}
    }
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(options)).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_levels::engine::LevelsPlugin)
        .add_systems(Update, headless::screenshot_on_request)
        .insert_resource(engine.identity.clone())
        .add_plugins(extra);
    if let Some(root) = assets {
        app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().asset_root = root.to_path_buf();
    }
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

    let display = if args.viewport {
        if !bevy_ts_viewport::supported() {
            eprintln!("bevy-ts dev: --viewport is macOS only; this is {}", std::env::consts::OS);
            std::process::exit(2);
        }
        Display::Viewport
    } else if args.headless {
        Display::Headless
    } else {
        Display::Window
    };
    let assets = headless::asset_root(&compiler.root);
    let mut app = base_app(engine, extra, display, V8Options { inspect: args.inspect }, Some(&assets));
    // Compile only once the plugins have exported their params: the compile step reads them as
    // system param kinds.
    dev::spawn_watcher(compiler.clone(), tx);
    app.insert_resource(channel).insert_resource(DevSource(compiler.clone()));
    if let Ok(Some(project)) = crate::project::find(&compiler.root)
        && project.config.levels.prefab_edits.as_deref() == Some("source")
    {
        app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().prefab_edits = bevy_ts_levels::PrefabEdits::Source;
    }
    app.add_plugins(bevy_ts_remote::BevyTsRemotePlugin { port: args.remote_port });
    bevy_ts_tsx::protocol::add_methods(&mut app);
    bevy_ts_levels::protocol::add_methods(&mut app);
    {
        let mut info = app.world_mut().resource_mut::<RuntimeInfo>();
        info.entry = Some(compiler.entry.clone());
    }
    app.run();
}

/// Plain JS, so it writes the declarations the compile step would otherwise append.
const SKELETON_JS: &str = r#"
import { app, Time, Transform, Update } from "bevy-ts";
import { Mesh3d } from "bevy-ts/bevy";

export function bob(time, q) {
  q.each((t) => {
    t.translation.y = Math.sin(time.elapsed_secs * 2) * 1.5;
  });
}

app.addSystems(Update, bob);

__bevyts.bind("skeleton.js", { Time: () => Time, Transform: () => Transform, Mesh3d: () => Mesh3d });
__bevyts.declare(bob, {
  id: "skeleton.js#bob",
  kind: "system",
  params: [
    { kind: "res", resource: ["skeleton.js", "Time"] },
    { kind: "query", data: [{ component: ["skeleton.js", "Transform"], mut: true }], filter: [{ with: ["skeleton.js", "Mesh3d"] }] },
  ],
});
"#;

/// The walking skeleton: a cube spawned in Rust, moved by one JS system through the zero-copy
/// `Transform` column, with a Rust `Changed<Transform>` system watching.
fn skeleton(engine: &Engine, extra: impl Plugin, headless_mode: bool) {
    let (tx, channel) = DevChannel::new();
    let path = std::env::current_dir().unwrap().join("skeleton.js");
    let set = ModuleSet { entry: path.clone(), modules: vec![CompiledModule { path, code: SKELETON_JS.into(), source_map: None }] };
    tx.send(DevEvent::Compiled(Ok(set), Duration::ZERO)).unwrap();
    let display = if headless_mode { Display::Headless } else { Display::Window };
    let mut app = base_app(engine, extra, display, V8Options::default(), None);
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
