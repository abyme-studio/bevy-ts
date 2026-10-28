//! The engine binary's command line, shared by the prebuilt binary and every project engine.
//!
//! `bevy-ts dev <entry.ts> [--headless | --viewport [--audio]] [--edit] [--remote-port <port>] [--inspect[=<port>]] [-- <args>…]`,
//! `bevy-ts build`, `bevy-ts play`, and an engine with a `game/` beside it, which plays that game
//! with every argument it was started with.

use crate::project::{DefaultSampler, Switches};
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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "usage:
  bevy-ts dev <entry.ts> [--headless | --viewport [--audio]] [--edit] [--remote-port <port>] [--inspect[=<port>]] [-- <args>…]
  bevy-ts build [<project dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--copy-sources] [--json]
  bevy-ts build --targets [--json]
  bevy-ts play <game folder> [--headless] [-- <args>…]
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
    /// Play sound in viewport mode (a window always does; headless never).
    pub audio: bool,
    /// Start in edit mode: no TS game code runs until a client sets `play`.
    pub edit: bool,
    pub remote_port: u16,
    pub inspect: Option<SocketAddr>,
    /// The game's arguments, after `--`: `process.argv.slice(2)`.
    pub game_args: Vec<String>,
}

fn parse_dev(args: &[String]) -> Result<DevArgs, String> {
    let mut entry = None;
    let mut out = DevArgs { entry: String::new(), headless: false, viewport: false, audio: false, edit: false, remote_port: 15702, inspect: None, game_args: vec![] };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--" => {
                out.game_args = it.by_ref().cloned().collect();
                break;
            }
            "--headless" => out.headless = true,
            "--viewport" => out.viewport = true,
            "--edit" => out.edit = true,
            "--audio" => out.audio = true,
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
    if out.audio && !out.viewport {
        return Err("--audio is for --viewport: a window always plays sound, and --headless never does".into());
    }
    if out.edit && out.headless {
        return Err("--edit and --headless cannot be combined".into());
    }
    Ok(out)
}

/// `bevy-ts play <game folder> [--headless] [-- <args>…]`: the folder, headless, the game's arguments.
fn parse_play(args: &[String]) -> Result<(String, bool, Vec<String>), String> {
    let (own, game_args) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], args[i + 1..].to_vec()),
        None => (args, vec![]),
    };
    let headless = own.iter().any(|a| a == "--headless");
    if let Some(bad) = own.iter().find(|a| a.starts_with("--") && *a != "--headless") {
        return Err(format!("unknown option {bad}"));
    }
    match own.iter().filter(|a| !a.starts_with("--")).collect::<Vec<_>>().as_slice() {
        [folder] => Ok((folder.to_string(), headless, game_args)),
        _ => Err("name one game folder".into()),
    }
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
    // A built game: the engine plays the `game/` beside it, wherever it was started from, and its
    // arguments are the game's, never a command.
    if let Some(folder) = crate::play::beside_exe() {
        std::process::exit(crate::play::run(&engine, extra, &folder, false, &args));
    }
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
        Some("build") => {
            if engine.identity.kind == bevy_ts::engine::EngineKind::Prebuilt
                && let Some(code) = crate::build::in_project(&args[1..])
            {
                std::process::exit(code);
            }
            std::process::exit(crate::build::run(&engine, extra, &args[1..]))
        }
        Some("play") => match parse_play(&args[1..]) {
            Ok((folder, headless, game_args)) => std::process::exit(crate::play::run(&engine, extra, Path::new(&folder), headless, &game_args)),
            Err(e) => {
                eprintln!("bevy-ts play: {e}\n{USAGE}");
                std::process::exit(2);
            }
        },
        Some("types") => {
            if engine.identity.kind == bevy_ts::engine::EngineKind::Prebuilt
                && let Some(code) = crate::supervise::types_in_project(&args[1..])
            {
                std::process::exit(code);
            }
            std::process::exit(crate::types::run(&engine, extra, &args[1..]))
        }
        Some("levels") if args.get(1).map(String::as_str) == Some("check") => {
            let sources = |first: &Path| Ok(crate::project::find(first)?.map(|p| p.asset_sources().into_iter().collect()).unwrap_or_default());
            std::process::exit(bevy_ts_levels::check::run(&args[2..], sources))
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

/// Every plugin that gives scripts a built-in module (`bevy-ts/ready`) or element: the run's app and
/// `bevy-ts types` both add this group, so the generated types declare every module a game can
/// import.
pub struct ScriptPlugins;

impl PluginGroup for ScriptPlugins {
    fn build(self) -> bevy::app::PluginGroupBuilder {
        bevy::app::PluginGroupBuilder::start::<Self>()
            .add(bevy_ts_tsx::TsxPlugin)
            .add(bevy_ts_material::ScriptMaterialPlugin)
            .add(bevy_ts_material::WorldTextPlugin)
            .add(bevy_ts_animation::PlayAnimationPlugin)
            .add(crate::input::ScriptInputPlugin)
            .add(crate::audio::ScriptAudioPlugin)
            .add(crate::time::ScriptTimePlugin)
            .add(crate::camera::CamerasPlugin)
            .add(crate::gizmos::GizmosPlugin)
            .add(crate::rust_events::RustEventsPlugin)
            .add(crate::diagnostics::ScriptDiagnosticsPlugin)
            .add(crate::session::ScriptSessionPlugin)
            .add(crate::ready::ScriptReadyPlugin)
            .add(crate::assets::ScriptAssetsPlugin)
    }
}

/// Bevy's default plugins, bevy-ts and tsx, then `extra` (the project's plugins), so they can
/// order against `BevyTsSet` and see bevy-ts's resources. `assets` is Bevy's asset root, the
/// project's `assets/` under `bevy-ts dev`; `None` keeps Bevy's default. `dev` runs bevy-ts in dev
/// mode and watches the assets; a shipped game runs with neither. `switches` adds the Bevy plugins
/// a project opts into (`bevy-ts.json`'s, or `game.json`'s, `meshPicking`, `tabNavigation` and
/// `directionalNavigation`) and sets `imageSampler`'s default sampler, and `diagnostics` the Bevy diagnostics plugins listed there: the one
/// place each is added, so dev and play cannot differ. `audio` plays sound in a viewport
/// (`--audio`); a window always does, headless never (`crate::audio::plays`).
#[allow(clippy::too_many_arguments)]
pub fn base_app(engine: &Engine, extra: impl Plugin, display: Display, options: V8Options, assets: Option<&Path>, dev: bool, switches: Switches, diagnostics: &[String], audio: bool) -> App {
    base_app_with_sources(&[], engine, extra, display, options, assets, dev, switches, diagnostics, audio)
}

/// [`base_app`] with the extra asset sources `sources` (name and absolute folder), loaded as
/// `name://path` and watched when `dev` watches the assets. A folder that does not exist is named
/// in one warning, and loads from it fail.
#[allow(clippy::too_many_arguments)]
pub fn base_app_with_sources(
    sources: &[(String, PathBuf)],
    engine: &Engine,
    extra: impl Plugin,
    display: Display,
    options: V8Options,
    assets: Option<&Path>,
    dev: bool,
    switches: Switches,
    diagnostics: &[String],
    audio: bool,
) -> App {
    let mut app = App::new();
    headless::register_sources(&mut app, sources);
    let inspect = options.inspect.is_some();
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
    let plugins = match switches.image_sampler {
        DefaultSampler::Nearest => plugins.set(ImagePlugin::default_nearest()),
        DefaultSampler::Linear => plugins,
    };
    match assets {
        Some(root) if dev => app.add_plugins(plugins.set(headless::asset_plugin(root))),
        Some(root) => app.add_plugins(plugins.set(headless::asset_plugin_unwatched(root))),
        None => app.add_plugins(plugins),
    };
    match display {
        Display::Headless => {
            match headless_size(std::env::var_os("BEVY_TS_SIZE").as_deref()) {
                Ok(Some(size)) => {
                    app.insert_resource(headless::HeadlessSize(size));
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("bevy-ts: {e}");
                    std::process::exit(2);
                }
            }
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
    crate::audio::audio_output(&mut app, display, audio);
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(options)).dev(dev))
        .add_plugins(ScriptPlugins)
        .add_plugins(bevy_ts_material::Pixel2dPlugin)
        .add_plugins(bevy_ts_material::BitmapTextPlugin)
        .add_plugins(bevy_ts_levels::engine::LevelsPlugin)
        .add_plugins(bevy_ts_remote::screenshot::ReadyPlugin)
        .add_systems(PostUpdate, warn_mesh_handlers.run_if(not(resource_exists::<MeshPickingSettings>)))
        .insert_resource(engine.identity.clone())
        .add_plugins(extra);
    for (name, folder) in sources.iter().filter(|(_, f)| !f.is_dir()) {
        let message = format!("asset source \"{name}\": {} does not exist; loads from {name}:// fail", folder.display());
        app.world_mut().resource_mut::<bevy_ts::log::ScriptLog>().push(bevy_ts::log::LogLevel::Warn, message, None, 0);
    }
    headless::pipeline_trace(&mut app);
    shot_from_env(&mut app);
    stall_watchdog(&mut app, display, inspect);
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
    crate::diagnostics::add_plugins(&mut app, diagnostics);
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

/// `BEVY_TS_SCREENSHOT=<path>` saves one shot at the 60th ready frame, on a fixed step; a shot
/// that cannot be ready, or that Bevy never writes, saves nothing and the run exits 1.
/// `BEVY_TS_FIXED_STEP=1` turns on the fixed step alone, for a run driven over the protocol.
fn shot_from_env(app: &mut App) {
    use bevy_ts_remote::screenshot::{FixedStep, ShotWhenReady};
    let shot = std::env::var_os("BEVY_TS_SCREENSHOT");
    if shot.is_some() || std::env::var_os("BEVY_TS_FIXED_STEP").is_some_and(|v| v == "1") {
        app.insert_resource(FixedStep);
    }
    if let Some(path) = shot {
        app.insert_resource(ShotWhenReady::new(path));
    }
}

/// The stall watchdog (`bevy_ts_remote::stall`): a headless run exits when a frame stops, a window
/// or viewport warns (macOS stops frames to a hidden window), and `--inspect` has none (a breakpoint
/// stops Update). `BEVY_TS_STALL_SECS=<seconds>` sets its limit, `0` turns it off.
fn stall_watchdog(app: &mut App, display: Display, inspect: bool) {
    use bevy_ts_remote::stall::{EXIT_CODE, LIMIT, StallAction, StallPlugin};
    let limit = match stall_secs(std::env::var_os("BEVY_TS_STALL_SECS").as_deref()) {
        Ok(limit) => limit.unwrap_or(LIMIT),
        Err(e) => {
            eprintln!("bevy-ts: {e}");
            std::process::exit(2);
        }
    };
    if inspect || limit.is_zero() {
        return;
    }
    let action = if display == Display::Headless { StallAction::Exit(EXIT_CODE) } else { StallAction::Warn };
    app.add_plugins(StallPlugin { limit, action });
}

/// `BEVY_TS_STALL_SECS`: a whole number of seconds, or none set.
fn stall_secs(value: Option<&std::ffi::OsStr>) -> Result<Option<Duration>, String> {
    let Some(value) = value else { return Ok(None) };
    let text = value.to_string_lossy();
    text.trim().parse::<u64>().map(|s| Some(Duration::from_secs(s))).map_err(|_| format!("BEVY_TS_STALL_SECS={text}: expected a whole number of seconds (0 turns the stall watchdog off)"))
}

/// `BEVY_TS_SIZE`: the headless image's `<width>x<height>` in pixels, or none set.
fn headless_size(value: Option<&std::ffi::OsStr>) -> Result<Option<UVec2>, String> {
    let Some(value) = value else { return Ok(None) };
    let text = value.to_string_lossy();
    let side = |s: &str| s.parse::<u32>().ok().filter(|n| *n >= 1 && s.bytes().all(|b| b.is_ascii_digit()));
    if let Some((Some(w), Some(h))) = text.trim().split_once('x').map(|(w, h)| (side(w), side(h))) {
        return Ok(Some(UVec2::new(w, h)));
    }
    Err(format!("BEVY_TS_SIZE={text}: expected <width>x<height> in pixels, each at least 1, as 2880x1620"))
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

/// Where a dev engine on the project at `root` keeps saves: in `.bevy-ts/`, already git-ignored.
pub fn dev_save_dir(root: &Path) -> PathBuf {
    root.join(".bevy-ts").join("saves")
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
    let diagnostics = project.as_ref().map(|p| p.config.diagnostics.clone()).unwrap_or_default();
    let assets = headless::asset_root(&compiler.root);
    let sources = project.as_ref().map(|p| p.asset_sources()).unwrap_or_default();
    let mut app = base_app_with_sources(&sources, engine, extra, display, V8Options { inspect: args.inspect }, Some(&assets), true, switches, &diagnostics, args.audio);
    // Compile only once the plugins have exported their params: the compile step reads them as
    // system param kinds.
    dev::spawn_watcher(compiler.clone(), tx);
    app.insert_resource(channel).insert_resource(DevSource(compiler.clone()));
    app.insert_resource(bevy_ts::saves::SaveDir(dev_save_dir(&compiler.root)));
    if let Some(project) = &project {
        if project.config.levels.prefab_edits.as_deref() == Some("source") {
            app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().prefab_edits = bevy_ts_levels::PrefabEdits::Source;
        }
        insert_net_policy(&mut app, &project.config.net.allow, &project.config.env);
    }
    app.add_plugins(bevy_ts_remote::BevyTsRemotePlugin { port: args.remote_port });
    if display == Display::Headless {
        // Pointer, key and IME input over `bevy-ts/viewport_input`: the pointer onto the image the
        // cameras render into, keys and IME onto the window-less primary window.
        let image = app.world().resource::<headless::Offscreen>().0.clone();
        app.add_plugins(bevy_ts_viewport::offscreen::OffscreenInputPlugin { image });
    }
    bevy_ts_tsx::protocol::add_methods(&mut app);
    bevy_ts_levels::protocol::add_methods(&mut app);
    {
        let mut info = app.world_mut().resource_mut::<RuntimeInfo>();
        info.entry = Some(compiler.entry.clone());
    }
    app.insert_resource(bevy_ts::backend::ScriptArgs::new(&compiler.entry, &args.game_args));
    if args.edit {
        // Edit before the first frame: the viewport's `Startup` enters edit mode, and nothing of
        // the game runs until the first play.
        app.insert_state(bevy_ts::EngineMode::Edit);
        app.world_mut().resource_mut::<bevy_ts::generation::Generations>().started = false;
        app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
        // The levels the editor left open and closed come back, and every change is remembered.
        app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().state_file = Some(compiler.root.join(".bevy-ts/editor.json"));
    }
    std::process::exit(crate::play::exit_code(app.run()));
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
    let mut app = base_app(engine, extra, display, V8Options::default(), None, true, Switches::default(), &[], false);
    let exit = app
        .insert_resource(channel)
        .add_systems(Startup, scene)
        .add_systems(PostUpdate, report_changed.after(bevy_ts::BevyTsSet::PostUpdate))
        .run();
    std::process::exit(crate::play::exit_code(exit));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dev_keeps_the_arguments_after_a_double_dash() {
        let a = parse_dev(&strings(&["src/main.ts", "--headless", "--", "--level", "arena"])).unwrap();
        assert_eq!(a.entry, "src/main.ts");
        assert!(a.headless);
        assert_eq!(a.game_args, ["--level", "arena"]);
        let a = parse_dev(&strings(&["src/main.ts", "--", "--headless", "--"])).unwrap();
        assert!(!a.headless, "after --, every argument is the game's");
        assert_eq!(a.game_args, ["--headless", "--"]);
        assert!(parse_dev(&strings(&["src/main.ts"])).unwrap().game_args.is_empty());
        assert!(parse_dev(&strings(&["src/main.ts", "--level"])).is_err(), "before --, an unknown option is refused");
    }

    #[test]
    fn play_takes_the_game_arguments_after_a_double_dash() {
        assert_eq!(parse_play(&strings(&["dist/arena/game", "--headless", "--", "--seed", "7"])), Ok(("dist/arena/game".into(), true, strings(&["--seed", "7"]))));
        assert_eq!(parse_play(&strings(&["dist/arena/game"])), Ok(("dist/arena/game".into(), false, vec![])));
        assert_eq!(parse_play(&strings(&["game", "--", "other"])), Ok(("game".into(), false, strings(&["other"]))));
        assert!(parse_play(&strings(&["game", "--seed"])).unwrap_err().contains("unknown option --seed"));
        assert!(parse_play(&strings(&["--", "game"])).unwrap_err().contains("one game folder"));
    }

    #[test]
    fn the_stall_limit_is_whole_seconds_and_names_its_variable() {
        use std::ffi::OsStr;
        assert_eq!(stall_secs(None), Ok(None));
        assert_eq!(stall_secs(Some(OsStr::new("45"))), Ok(Some(Duration::from_secs(45))));
        assert_eq!(stall_secs(Some(OsStr::new("0"))), Ok(Some(Duration::ZERO)), "0 turns it off");
        for bad in ["", "1.5", "-3", "ten"] {
            assert!(stall_secs(Some(OsStr::new(bad))).unwrap_err().starts_with("BEVY_TS_STALL_SECS="), "{bad:?}");
        }
    }

    #[test]
    fn the_headless_size_is_width_by_height_and_names_its_variable() {
        use std::ffi::OsStr;
        assert_eq!(headless_size(None), Ok(None));
        assert_eq!(headless_size(Some(OsStr::new("2880x1620"))), Ok(Some(UVec2::new(2880, 1620))));
        assert_eq!(headless_size(Some(OsStr::new(" 640x480 "))), Ok(Some(UVec2::new(640, 480))), "trimmed");
        for bad in ["2880", "0x540", "640x0", "640X480", "640x480x2", "-1x5", "ax b", "", "+640x480"] {
            assert!(headless_size(Some(OsStr::new(bad))).unwrap_err().starts_with("BEVY_TS_SIZE="), "{bad:?}");
        }
    }
}
