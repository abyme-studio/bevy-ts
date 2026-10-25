//! The engine binary's command line, shared by the prebuilt binary and every project engine.
//!
//! `bevy-ts dev <entry.ts> [--headless | --viewport] [--edit] [--remote-port <port>] [--inspect[=<port>]]`,
//! `bevy-ts build`, `bevy-ts play`, and a bare start that plays the `game/` beside the binary.

use crate::project::Switches;
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::picking::mesh_picking::{MeshPickingPlugin, MeshPickingSettings};
use bevy::prelude::*;
use bevy_ts_tsx::events::TsxHandlers;
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
  bevy-ts dev <entry.ts> [--headless | --viewport] [--edit] [--remote-port <port>] [--inspect[=<port>]]
  bevy-ts build [<project dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--json]
  bevy-ts build --targets [--json]
  bevy-ts play <game folder> [--headless]
  bevy-ts types [--out <file>]
  bevy-ts transform <file.ts> [--dev]
  bevy-ts skeleton [--headless]
  bevy-ts levels check [--upstream] <file.bsn>…
from the npm launcher (`npx bevy-ts`), not this binary:
  bevy-ts new <dir> [--example <id>] [--no-install] [--json]
  bevy-ts examples [--json]";

/// Parsed `bevy-ts dev` options.
#[derive(Debug, Clone)]
pub struct DevArgs {
    pub entry: String,
    pub headless: bool,
    /// No window of its own: render for a viewer that attaches over the protocol (macOS).
    pub viewport: bool,
    /// Start in edit mode: no TS game code runs until a client sets `play`.
    pub edit: bool,
    pub remote_port: u16,
    pub inspect: Option<SocketAddr>,
}

fn parse_dev(args: &[String]) -> Result<DevArgs, String> {
    let mut entry = None;
    let mut out = DevArgs { entry: String::new(), headless: false, viewport: false, edit: false, remote_port: 15702, inspect: None };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--headless" => out.headless = true,
            "--viewport" => out.viewport = true,
            "--edit" => out.edit = true,
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
    if out.edit && out.headless {
        return Err("--edit and --headless cannot be combined".into());
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
        Some("build") => std::process::exit(crate::build::run(&args[1..])),
        Some("play") => {
            let headless = args[1..].iter().any(|a| a == "--headless");
            let folders: Vec<&String> = args[1..].iter().filter(|a| !a.starts_with("--")).collect();
            match (folders.as_slice(), args[1..].iter().find(|a| a.starts_with("--") && *a != "--headless")) {
                ([folder], None) => std::process::exit(crate::play::run(&engine, extra, Path::new(folder), headless)),
                (_, Some(bad)) => {
                    eprintln!("bevy-ts play: unknown option {bad}\n{USAGE}");
                    std::process::exit(2);
                }
                _ => {
                    eprintln!("bevy-ts play: name one game folder\n{USAGE}");
                    std::process::exit(2);
                }
            }
        }
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
        // The npm launcher's own commands: the examples ship in the npm package, not the binary.
        Some(cmd @ ("new" | "examples")) => {
            eprintln!("bevy-ts {cmd}: run it through the npm launcher, `npx bevy-ts {cmd}`; the engine binary has no examples");
            std::process::exit(2);
        }
        Some("skeleton") => skeleton(&engine, extra, args.iter().any(|a| a == "--headless")),
        None => {
            // A built game: the engine plays the `game/` beside it, wherever it was started from.
            if let Some(folder) = crate::play::beside_exe() {
                std::process::exit(crate::play::run(&engine, extra, &folder, false));
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
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
/// project's `assets/` under `bevy-ts dev`; `None` keeps Bevy's default. `dev` runs bevy-ts in dev
/// mode and watches the assets; a shipped game runs with neither. `switches` adds the Bevy plugins
/// a project opts into (`bevy-ts.json`'s, or `game.json`'s, `meshPicking`, `tabNavigation` and
/// `directionalNavigation`): the one place each is added, so dev and play cannot differ.
pub fn base_app(engine: &Engine, extra: impl Plugin, display: Display, options: V8Options, assets: Option<&Path>, dev: bool, switches: Switches) -> App {
    let mut app = App::new();
    let plugins = headless::default_plugins(display != Display::Window);
    // The compute pool runs the frame's systems: its threads are raised with the main thread's
    // while a viewport renders.
    #[cfg(target_os = "macos")]
    let plugins = if display == Display::Viewport {
        let mut options = bevy::app::TaskPoolOptions::default();
        options.compute.on_thread_spawn = Some(std::sync::Arc::new(bevy_ts_viewport::qos::register_this_thread));
        options.compute.on_thread_destroy = Some(std::sync::Arc::new(bevy_ts_viewport::qos::unregister_this_thread));
        plugins.set(bevy::app::TaskPoolPlugin { task_pool_options: options })
    } else {
        plugins
    };
    match assets {
        Some(root) if dev => app.add_plugins(plugins.set(headless::asset_plugin(root))),
        Some(root) => app.add_plugins(plugins.set(headless::asset_plugin_unwatched(root))),
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
        // The studio's edit and asset tools in the engine's own window; a shipped game has none.
        #[cfg(target_os = "macos")]
        Display::Window if studio_tools(display, dev) => {
            app.add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: true });
        }
        _ => {}
    }
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(options)).dev(dev))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_material::ScriptMaterialPlugin)
        .add_plugins(bevy_ts_animation::PlayAnimationPlugin)
        .add_plugins(crate::input::ScriptInputPlugin)
        .add_plugins(bevy_ts_levels::engine::LevelsPlugin)
        .add_plugins(crate::camera::CamerasPlugin)
        .add_plugins(crate::rust_events::RustEventsPlugin)
        .add_systems(Update, headless::screenshot_on_request)
        .add_systems(PostUpdate, warn_mesh_handlers.run_if(not(resource_exists::<MeshPickingSettings>)))
        .insert_resource(engine.identity.clone())
        .add_plugins(extra);
    headless::pipeline_trace(&mut app);
    // After `extra`: a native crate that adds one itself keeps its own settings.
    if switches.mesh_picking && !app.is_plugin_added::<MeshPickingPlugin>() {
        app.add_plugins(MeshPickingPlugin);
    }
    if switches.tab_navigation && !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    if switches.directional_navigation && !app.is_plugin_added::<DirectionalNavigationPlugin>() {
        app.add_plugins(DirectionalNavigationPlugin);
    }
    // Checked once the plugins are in, so a native crate that added it is not warned.
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_systems(PostUpdate, warn_tab_props);
    }
    if let Some(root) = assets {
        app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().asset_root = root.to_path_buf();
        // The asset list, thumbnails and previews read the project's assets, not the binary's.
        #[cfg(target_os = "macos")]
        if dev {
            app.insert_resource(bevy_ts_viewport::assets::AssetRoot(root.to_path_buf()));
        }
    }
    app
}

/// A mesh that has a pointer handler, one of the two just added.
type NewMeshHandler = (With<Mesh3d>, With<TsxHandlers>, Or<(Added<Mesh3d>, Added<TsxHandlers>)>);

/// Names, once per process, a mesh with a pointer handler while nothing picks meshes: its
/// `onPointer*` would never fire. Checks the resource, not the switch, so a native crate that adds
/// `MeshPickingPlugin` itself is not warned.
fn warn_mesh_handlers(
    added: Query<(Entity, Option<&Name>), NewMeshHandler>,
    log: Option<ResMut<bevy_ts::log::ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut warned: Local<bool>,
) {
    if *warned {
        return;
    }
    let (Some((e, name)), Some(mut log)) = (added.iter().next(), log) else { return };
    *warned = true;
    let name = name.map(|n| format!(" \"{n}\"")).unwrap_or_default();
    let message = format!(
        "entity {e}{name} has a pointer handler on a mesh, but mesh picking is off: it never fires. Set \"meshPicking\": true in bevy-ts.json"
    );
    log.push(bevy_ts::log::LogLevel::Warn, message, None, generations.map_or(0, |g| g.live));
}

/// Names, once per process, an entity whose `tabIndex` or `tabGroup` prop mounted while nothing
/// moves focus by Tab. Keys on the prop's marker, not `TabIndex`: a `<tab>` writes its own.
fn warn_tab_props(
    added: Query<(Entity, Option<&Name>), Added<bevy_ts_tsx::ui::TabProp>>,
    log: Option<ResMut<bevy_ts::log::ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut warned: Local<bool>,
) {
    if *warned {
        return;
    }
    let (Some((e, name)), Some(mut log)) = (added.iter().next(), log) else { return };
    *warned = true;
    let name = name.map(|n| format!(" \"{n}\"")).unwrap_or_default();
    let message = format!("entity {e}{name} has a tabIndex or tabGroup, but tab navigation is off: Tab does not move focus. Set \"tabNavigation\": true in bevy-ts.json");
    log.push(bevy_ts::log::LogLevel::Warn, message, None, generations.map_or(0, |g| g.live));
}

/// Whether the app carries the studio's viewport tooling (edit, assets, previews): under `bevy-ts
/// dev` with a window or a viewport, never in a shipped game.
pub fn studio_tools(display: Display, dev: bool) -> bool {
    dev && display != Display::Headless
}

/// Sets what scripts may connect to and see of the environment, from `net.allow` and `env`,
/// reading `${NAME}` entries and the variables now. Skipped entries are logged.
pub fn insert_net_policy(app: &mut App, allow: &[String], env: &[String]) {
    let (policy, warnings) = bevy_ts::net::NetPolicy::from_env(allow, env);
    app.insert_resource(policy);
    let mut log = app.world_mut().resource_mut::<bevy_ts::log::ScriptLog>();
    for w in warnings {
        log.push(bevy_ts::log::LogLevel::Warn, w, None, 0);
    }
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
    let project = match crate::project::find(&compiler.root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("bevy-ts dev: {e}");
            std::process::exit(1);
        }
    };
    let switches = project.as_ref().map(|p| p.config.switches).unwrap_or_default();
    let assets = headless::asset_root(&compiler.root);
    let mut app = base_app(engine, extra, display, V8Options { inspect: args.inspect }, Some(&assets), true, switches);
    // Compile only once the plugins have exported their params: the compile step reads them as
    // system param kinds.
    dev::spawn_watcher(compiler.clone(), tx);
    app.insert_resource(channel).insert_resource(DevSource(compiler.clone()));
    if let Some(project) = &project {
        if project.config.levels.prefab_edits.as_deref() == Some("source") {
            app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().prefab_edits = bevy_ts_levels::PrefabEdits::Source;
        }
        insert_net_policy(&mut app, &project.config.net.allow, &project.config.env);
    }
    app.add_plugins(bevy_ts_remote::BevyTsRemotePlugin { port: args.remote_port });
    if display == Display::Headless {
        // Pointer input over `bevy-ts/viewport_input`, onto the image the cameras render into.
        let image = app.world().resource::<headless::Offscreen>().0.clone();
        app.add_plugins(bevy_ts_viewport::offscreen::OffscreenPointerPlugin { image });
    }
    bevy_ts_tsx::protocol::add_methods(&mut app);
    bevy_ts_levels::protocol::add_methods(&mut app);
    {
        let mut info = app.world_mut().resource_mut::<RuntimeInfo>();
        info.entry = Some(compiler.entry.clone());
    }
    if args.edit {
        // Edit before the first frame: the viewport's `Startup` enters edit mode, and nothing of
        // the game runs until the first play.
        app.insert_state(bevy_ts::EngineMode::Edit);
        app.world_mut().resource_mut::<bevy_ts::generation::Generations>().started = false;
        app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
        // The levels the editor left open and closed come back, and every change is remembered.
        app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().state_file = Some(compiler.root.join(".bevy-ts/editor.json"));
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
    let mut app = base_app(engine, extra, display, V8Options::default(), None, true, Switches::default());
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
