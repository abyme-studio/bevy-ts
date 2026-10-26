//! `bevy-ts play` and a built game's bare start: the game folder from argv or beside the
//! executable, played by `bevy_ts_embed::play` with the process environment, saves in the
//! player's data folder.

use crate::cli::{Display, Engine, base_app_with_sources};
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
/// `args` are the game's: `process.argv.slice(2)`.
pub fn app(engine: &Engine, extra: impl Plugin, folder: &Path, display: Display, args: &[String]) -> Result<App, String> {
    let game = Game::load(folder, &engine.identity)?;
    let saves = save_dir(&game.file.name);
    let env = |name: &str| std::env::var(name).ok();
    let mut app = play_app(game, &env, args, |assets, sources, switches, diagnostics| {
        base_app_with_sources(sources, engine, extra, display, V8Options::default(), Some(assets), false, switches, diagnostics, false)
    });
    if let Some(dir) = saves {
        app.insert_resource(bevy_ts::saves::SaveDir(dir));
    }
    Ok(app)
}

/// A played game's saves: `<data>/<game name>/saves`, the player's own data folder, never the game
/// folder (read-only once shipped). None for a name that is not one folder.
pub fn save_dir(game: &str) -> Option<PathBuf> {
    let one_folder = !game.is_empty() && game != "." && game != ".." && !game.contains(['/', '\\', ':']);
    one_folder.then(|| bevy_ts::saves::game_save_dir(game)).flatten()
}

/// Plays the game in `folder` with the game's arguments `args`. Returns the code of the game's
/// `AppExit`, or 1 when it cannot start.
pub fn run(engine: &Engine, extra: impl Plugin, folder: &Path, headless: bool, args: &[String]) -> i32 {
    let display = if headless { Display::Headless } else { Display::Window };
    match app(engine, extra, folder, display, args) {
        Ok(mut app) => exit_code(app.run()),
        Err(e) => {
            eprintln!("bevy-ts play: {e}");
            1
        }
    }
}

/// The process exit code of a Bevy `AppExit`: 0, or its error's code.
pub fn exit_code(exit: AppExit) -> i32 {
    match exit {
        AppExit::Success => 0,
        AppExit::Error(code) => i32::from(code.get()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_and_env_round_trip_unexpanded() {
        let config = crate::project::parse(
            r#"{ "net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] }, "env": ["ABYME_URL", "ABYME_KEY"] }"#,
        )
        .unwrap();
        let file = GameFile {
            name: "town".into(),
            entry: "src/game.ts".into(),
            bevy_ts: crate::engine::VERSION.into(),
            engine: "prebuilt".into(),
            switches: Default::default(),
            diagnostics: vec![],
            net: config.net,
            env: config.env,
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
        assert!(game.diagnostics.is_empty(), "and no diagnostics");
    }

    #[test]
    fn a_built_game_keeps_its_diagnostics() {
        let config = crate::project::parse(r#"{ "diagnostics": ["EntityCountDiagnosticsPlugin"] }"#).unwrap();
        let file = GameFile {
            name: "town".into(),
            entry: "src/game.ts".into(),
            bevy_ts: crate::engine::VERSION.into(),
            engine: "prebuilt".into(),
            switches: Default::default(),
            diagnostics: config.diagnostics,
            net: Default::default(),
            env: vec![],
            asset_sources: Default::default(),
        };
        let text = serde_json::to_string(&file).unwrap();
        assert!(text.contains(r#""diagnostics":["EntityCountDiagnosticsPlugin"]"#), "{text}");
        let back: GameFile = serde_json::from_str(&text).unwrap();
        assert_eq!(back.diagnostics, ["EntityCountDiagnosticsPlugin"]);
        let none = GameFile { diagnostics: vec![], ..file };
        assert!(!serde_json::to_string(&none).unwrap().contains("diagnostics"), "absent when none");
    }

    #[test]
    fn a_hand_edited_game_json_with_an_unknown_plugin_is_refused() {
        let dir = std::env::temp_dir().join(format!("bevy-ts-game-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = format!(r#"{{ "name": "town", "entry": "src/game.ts", "bevyTs": "{}", "engine": "prebuilt", "diagnostics": ["FpsPlugin"] }}"#, crate::engine::VERSION);
        std::fs::write(dir.join(GAME_FILE), text).unwrap();
        let e = read_game(&dir).unwrap_err();
        assert!(e.contains("\"FpsPlugin\" is not a diagnostics plugin"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
