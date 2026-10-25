//! `bevy-ts play` and a built game's bare start: the game folder from argv or beside the
//! executable, played by `bevy_ts_embed::play` with the process environment.

use crate::cli::{Display, Engine, base_app};
use bevy::prelude::*;
pub use bevy_ts_embed::play::{GAME_DIR, GAME_FILE, GameFile, check_engine, read_game, read_scripts};
use bevy_ts_embed::play::{Game, play_app};
use bevy_ts_v8::V8Options;
use std::path::{Path, PathBuf};

/// The game folder beside the running executable, when it holds a `game.json`.
pub fn beside_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let dir = exe.parent()?.join(GAME_DIR);
    dir.join(GAME_FILE).is_file().then_some(dir)
}

/// The app that plays the game in `folder` (the one holding `game.json`), its one generation
/// queued: bevy-ts in non-dev mode, assets unwatched, no protocol, inspector or studio tools.
pub fn app(engine: &Engine, extra: impl Plugin, folder: &Path, display: Display) -> Result<App, String> {
    let game = Game::load(folder, &engine.identity)?;
    let env = |name: &str| std::env::var(name).ok();
    Ok(play_app(game, &env, |assets, mesh_picking| base_app(engine, extra, display, V8Options::default(), Some(assets), false, mesh_picking)))
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
