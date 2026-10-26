//! Playing a built game (shipped-game): the compiled modules under `game/scripts/` load once as
//! one generation, with no compiler, file watcher, remote protocol or inspector, bevy-ts in
//! non-dev mode, and assets read from `game/assets/` unwatched. The CLI's `bevy-ts play` and an
//! embedding host build the app the same way: [`play_app`] on a base app of their own display.

use crate::modules::{SHADER_EXT, inline_source_map, shader_module};
use bevy::prelude::*;
use bevy_ts::backend::{CompiledModule, ModuleSet};
use bevy_ts::engine::EngineIdentity;
use bevy_ts::generation::{DevChannel, DevEvent};
use bevy_ts::runtime::RuntimeInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The bevy-ts version of this engine: a game must have been built by it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The folder beside the executable a bare start plays.
pub const GAME_DIR: &str = "game";
/// The file that marks a game folder.
pub const GAME_FILE: &str = "game.json";

/// `game/game.json`: what built the game.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameFile {
    pub name: String,
    /// The entry module, relative to `scripts/`, with `/` separators.
    pub entry: String,
    /// The bevy-ts version that built it.
    #[serde(rename = "bevyTs")]
    pub bevy_ts: String,
    /// `prebuilt`, or the project engine's hash.
    pub engine: String,
    /// `bevy-ts.json`'s switches, each absent when off: a `game.json` from before has none.
    #[serde(flatten)]
    pub switches: Switches,
    /// `bevy-ts.json`'s `diagnostics`, absent when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// `net` and `env` from `bevy-ts.json`, unexpanded: `${NAME}` reads this machine's
    /// environment when the game starts. A `game.json` from before has neither: no network.
    #[serde(default)]
    pub net: NetConfig,
    #[serde(default)]
    pub env: Vec<String>,
    /// The extra asset sources, name to folder: `~/` read on the playing machine, a relative one
    /// from the game folder (`sources/<name>` after `--copy-sources`). Absent when none.
    #[serde(default, rename = "assetSources", skip_serializing_if = "BTreeMap::is_empty")]
    pub asset_sources: BTreeMap<String, String>,
}

/// `game.json`'s sources resolved on this machine; an absent folder fails naming the source, the
/// folder and the fix, before anything starts.
pub fn sources(game: &GameFile, folder: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let home = home();
    let mut out = Vec::new();
    for (name, written) in &game.asset_sources {
        let dir = resolve_source(written, folder, home.as_deref());
        if !dir.is_dir() {
            return Err(format!("asset source \"{name}\": {} does not exist; put the folder there, or build the game with --copy-sources", dir.display()));
        }
        out.push((name.clone(), dir));
    }
    Ok(out)
}

/// A source folder as written, resolved: `~/` under `home`, a relative one under `base`, made
/// canonical when it exists (Bevy's watcher reports canonical paths, as for the default root).
pub fn resolve_source(folder: &str, base: &Path, home: Option<&Path>) -> PathBuf {
    let path = match (folder.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => base.join(folder),
    };
    let path = std::path::absolute(&path).unwrap_or(path);
    path.canonicalize().unwrap_or(path)
}

/// This machine's home folder, for `~/`.
pub fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Bevy plugins a project opts into, each off by default as Bevy keeps it out of
/// `DefaultPlugins`, and `ImagePlugin`'s default sampler: read from `bevy-ts.json` under `dev` and from `game.json` when playing, where
/// `bevy-ts build` writes each that is on. None is an engine hash input: the prebuilt engine has
/// every plugin, and [`crate::shipped::finish_shipped`] adds the ones switched on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Switches {
    /// `MeshPickingPlugin`: 3D meshes and `<mesh2d>`s take pointer events.
    #[serde(default, rename = "meshPicking", deserialize_with = "mesh_picking", skip_serializing_if = "is_false")]
    pub mesh_picking: bool,
    /// `TabNavigationPlugin`: Tab and Shift-Tab move focus in tab order.
    #[serde(default, rename = "tabNavigation", deserialize_with = "tab_navigation", skip_serializing_if = "is_false")]
    pub tab_navigation: bool,
    /// `DirectionalNavigationPlugin`: the map `DirectionalNavigation` navigates.
    #[serde(default, rename = "directionalNavigation", deserialize_with = "directional_navigation", skip_serializing_if = "is_false")]
    pub directional_navigation: bool,
    /// `imageSampler`: `ImagePlugin`'s sampler for every image whose sampler is `Default`.
    #[serde(default, rename = "imageSampler", skip_serializing_if = "DefaultSampler::is_linear")]
    pub image_sampler: DefaultSampler,
}

/// `imageSampler` in `bevy-ts.json`: Bevy's default, linear, or `ImagePlugin::default_nearest()`
/// for pixel art. An image given its own sampler keeps it.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DefaultSampler {
    #[default]
    Linear,
    Nearest,
}

impl DefaultSampler {
    fn is_linear(&self) -> bool {
        *self == DefaultSampler::Linear
    }

    /// `plugins` with `ImagePlugin` set to this sampler.
    pub fn set(self, plugins: bevy::app::PluginGroupBuilder) -> bevy::app::PluginGroupBuilder {
        match self {
            DefaultSampler::Nearest => plugins.set(ImagePlugin::default_nearest()),
            DefaultSampler::Linear => plugins,
        }
    }
}

impl<'de> Deserialize<'de> for DefaultSampler {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) if s == "linear" => Ok(DefaultSampler::Linear),
            serde_json::Value::String(s) if s == "nearest" => Ok(DefaultSampler::Nearest),
            v => Err(serde::de::Error::custom(format!("imageSampler must be \"nearest\" or \"linear\", not {v}"))),
        }
    }
}

fn is_false(b: &bool) -> bool {
    !b
}

/// A switch as a bool, its error naming the field (serde's names only the type).
fn switch<'de, D: serde::Deserializer<'de>>(d: D, field: &str) -> Result<bool, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(b) => Ok(b),
        v => Err(serde::de::Error::custom(format!("{field} must be true or false, not {v}"))),
    }
}

fn mesh_picking<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    switch(d, "meshPicking")
}

fn tab_navigation<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    switch(d, "tabNavigation")
}

fn directional_navigation<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    switch(d, "directionalNavigation")
}

/// `net` in `bevy-ts.json` and `game.json`: deny by default.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetConfig {
    /// `host`, `host:port`, a URL, or `${NAME}` read from the environment at start.
    #[serde(default)]
    pub allow: Vec<String>,
}

/// Reads `game.json`, refusing one built by another bevy-ts version.
pub fn read_game(folder: &Path) -> Result<GameFile, String> {
    let file = folder.join(GAME_FILE);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let game: GameFile = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    if game.bevy_ts != VERSION {
        return Err(format!(
            "{} was built by bevy-ts {}, but this engine is bevy-ts {}; rebuild the game",
            game.name,
            game.bevy_ts,
            VERSION
        ));
    }
    crate::diagnostics::check_names(&game.diagnostics).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(game)
}

/// Refuses a game built for another engine: a project engine's game runs only on that engine,
/// the executable its build put beside `game/`.
pub fn check_engine(game: &GameFile, folder: &Path, identity: &EngineIdentity) -> Result<(), String> {
    let prebuilt = identity.kind == bevy_ts::engine::EngineKind::Prebuilt;
    let fits = if game.engine == "prebuilt" { prebuilt } else { game.engine == identity.hash };
    if fits {
        return Ok(());
    }
    let exe_name = if cfg!(windows) { format!("{}.exe", game.name) } else { game.name.clone() };
    let exe = folder.parent().map_or_else(|| PathBuf::from(&exe_name), |d| d.join(&exe_name));
    Err(if game.engine == "prebuilt" {
        format!("{} was built for the prebuilt engine, but this is project engine {}; play it with the prebuilt bevy-ts, or {}", game.name, identity.hash, exe.display())
    } else {
        format!("{} was built for its project engine {}, not this engine ({}); play it with {}", game.name, game.engine, identity.hash, exe.display())
    })
}

/// The extensions a compiled module keeps: the build writes each with its source's name, and a
/// shader as the `.wgsl` file itself.
const SCRIPT_EXTENSIONS: [&str; 9] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "wgsl"];

/// Every module under `scripts/`, its source map taken from the code. Other files (a Finder
/// `.DS_Store`) are skipped.
pub fn read_scripts(folder: &Path, game: &GameFile) -> Result<ModuleSet, String> {
    let scripts = folder.join("scripts");
    let scripts = scripts.canonicalize().map_err(|e| format!("{}: {e}", scripts.display()))?;
    let mut files = Vec::new();
    walk(&scripts, &mut files).map_err(|e| format!("{}: {e}", scripts.display()))?;
    let mut modules = Vec::with_capacity(files.len());
    for path in files {
        // A shipped shader is the WGSL itself; its module names the shipped file, as `bevy-ts dev`'s
        // names the project's.
        if path.to_string_lossy().ends_with(SHADER_EXT) {
            modules.push(CompiledModule { code: shader_module(&path), path, source_map: None });
            continue;
        }
        let code = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let source_map = inline_source_map(&code);
        modules.push(CompiledModule { path, code, source_map });
    }
    let entry = scripts.join(&game.entry);
    if !modules.iter().any(|m| m.path == entry) {
        return Err(format!("{}: the entry {} is not under scripts/", folder.join(GAME_FILE).display(), game.entry));
    }
    Ok(ModuleSet { entry, modules })
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let hidden = e.file_name().to_string_lossy().starts_with('.');
        if e.file_type()?.is_dir() {
            if !hidden {
                walk(&path, out)?;
            }
        } else if !hidden && path.extension().is_some_and(|x| SCRIPT_EXTENSIONS.iter().any(|s| x == *s)) {
            out.push(path);
        }
    }
    Ok(())
}

/// A game folder read and checked: its `game.json`, its modules and its asset sources.
pub struct Game {
    /// The folder holding `game.json`, absolute.
    pub folder: PathBuf,
    pub file: GameFile,
    pub modules: ModuleSet,
    /// `assetSources`, each name and its folder on this machine.
    pub sources: Vec<(String, PathBuf)>,
}

impl Game {
    /// Reads the game in `folder`, refusing one built by another version or for another engine, or
    /// one whose asset source is not there.
    pub fn load(folder: &Path, identity: &EngineIdentity) -> Result<Game, String> {
        let folder = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
        let file = read_game(&folder)?;
        check_engine(&file, &folder, identity)?;
        let modules = read_scripts(&folder, &file)?;
        let sources = sources(&file, &folder)?;
        Ok(Game { folder, file, modules, sources })
    }

    /// The game's asset root, `assets/` in its folder.
    pub fn assets(&self) -> PathBuf {
        self.folder.join("assets")
    }
}

/// The app that plays `game`, its one generation queued. `env` is what `${NAME}` in `net.allow`
/// and `process.env` read: the process's environment, or an embedding host's map. `args` are the
/// game's, `process.argv.slice(2)`. `base` builds the app for the game's asset root, asset sources,
/// switches and diagnostics: Bevy's plugins for its display, then [`crate::shipped`]'s set and the
/// engine's own.
pub fn play_app(
    game: Game,
    env: &dyn Fn(&str) -> Option<String>,
    args: &[String],
    base: impl FnOnce(&Path, &[(String, PathBuf)], Switches, &[String]) -> App,
) -> App {
    let mut app = base(&game.assets(), &game.sources, game.file.switches, &game.file.diagnostics);
    let entry = game.modules.entry.clone();
    let (tx, channel) = DevChannel::new();
    tx.send(DevEvent::Compiled(Ok(game.modules), Duration::ZERO)).expect("the channel is open");
    app.insert_resource(channel);
    crate::shipped::insert_net_policy(&mut app, &game.file.net.allow, &game.file.env, env);
    app.insert_resource(bevy_ts::backend::ScriptArgs::new(&entry, args));
    app.world_mut().resource_mut::<RuntimeInfo>().entry = Some(entry);
    app
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_and_env_round_trip_unexpanded() {
        let net = NetConfig { allow: vec!["127.0.0.1:4477".into(), "${ABYME_URL}".into()] };
        let env = vec!["ABYME_URL".to_string(), "ABYME_KEY".to_string()];
        let file = GameFile {
            name: "town".into(),
            entry: "src/game.ts".into(),
            bevy_ts: VERSION.into(),
            engine: "prebuilt".into(),
            switches: Default::default(),
            diagnostics: vec![],
            net,
            env,
            asset_sources: Default::default(),
        };
        let text = serde_json::to_string_pretty(&file).unwrap();
        assert!(text.contains("\"${ABYME_URL}\""), "{text}");
        let back: GameFile = serde_json::from_str(&text).unwrap();
        assert_eq!(back, file);
        // Started with ABYME_URL unset, the game may connect only to 127.0.0.1:4477.
        let (policy, _) = bevy_ts::net::NetPolicy::from_config(&back.net.allow, &back.env, &|_| None);
        assert!(policy.check("ws://127.0.0.1:4477/api/stream", &[], false).is_ok());
        assert!(policy.check("ws://127.0.0.1:5000/", &[], false).is_err());
        assert!(policy.env.is_empty());
    }

    #[test]
    fn an_older_game_json_has_no_network() {
        let old = r#"{ "name": "cube", "entry": "src/game.ts", "bevyTs": "0.1.0", "engine": "prebuilt" }"#;
        let game: GameFile = serde_json::from_str(old).unwrap();
        assert!(game.net.allow.is_empty() && game.env.is_empty());
    }
}
