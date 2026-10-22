//! Playing a built game (shipped-game): the compiled modules under `game/scripts/` load once as
//! one generation, with no compiler, file watcher, remote protocol or inspector, bevy-ts in
//! non-dev mode, and assets read from `game/assets/` unwatched.

use crate::cli::{Display, Engine, base_app};
use bevy::prelude::*;
use bevy_ts::backend::{CompiledModule, ModuleSet};
use bevy_ts::generation::{DevChannel, DevEvent};
use bevy_ts::runtime::RuntimeInfo;
use bevy_ts_v8::V8Options;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    /// `bevy-ts.json`'s `meshPicking`, absent when off.
    #[serde(default, rename = "meshPicking", skip_serializing_if = "is_false")]
    pub mesh_picking: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

/// The game folder beside the running executable, when it holds a `game.json`.
pub fn beside_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let dir = exe.parent()?.join(GAME_DIR);
    dir.join(GAME_FILE).is_file().then_some(dir)
}

/// Reads `game.json`, refusing one built by another bevy-ts version.
pub fn read_game(folder: &Path) -> Result<GameFile, String> {
    let file = folder.join(GAME_FILE);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let game: GameFile = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    if game.bevy_ts != crate::engine::VERSION {
        return Err(format!(
            "{} was built by bevy-ts {}, but this engine is bevy-ts {}; rebuild the game",
            game.name,
            game.bevy_ts,
            crate::engine::VERSION
        ));
    }
    Ok(game)
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
        if path.to_string_lossy().ends_with(crate::compile::SHADER_EXT) {
            modules.push(CompiledModule { code: crate::compile::shader_module(&path), path, source_map: None });
            continue;
        }
        let code = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let source_map = crate::compile::inline_source_map(&code);
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

/// The app that plays the game in `folder` (the one holding `game.json`), its one generation
/// queued: bevy-ts in non-dev mode, assets unwatched, no protocol, inspector or studio tools.
pub fn app(engine: &Engine, extra: impl Plugin, folder: &Path, display: Display) -> Result<App, String> {
    let folder = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
    let game = read_game(&folder)?;
    let set = read_scripts(&folder, &game)?;
    let entry = set.entry.clone();
    let (tx, channel) = DevChannel::new();
    tx.send(DevEvent::Compiled(Ok(set), Duration::ZERO)).expect("the channel is open");
    let assets = folder.join("assets");
    let mut app = base_app(engine, extra, display, V8Options::default(), Some(&assets), false, game.mesh_picking);
    app.insert_resource(channel);
    app.world_mut().resource_mut::<RuntimeInfo>().entry = Some(entry);
    Ok(app)
}

/// Plays the game in `folder`. Returns the exit code when it cannot.
pub fn run(engine: &Engine, extra: impl Plugin, folder: &Path, headless: bool) -> i32 {
    let display = if headless { Display::Headless } else { Display::Window };
    match app(engine, extra, folder, display) {
        Ok(mut app) => {
            app.run();
            0
        }
        Err(e) => {
            eprintln!("bevy-ts play: {e}");
            1
        }
    }
}
