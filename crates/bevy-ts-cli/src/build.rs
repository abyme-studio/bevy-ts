//! `bevy-ts build` (game-build): a project made into a folder another person can run, the
//! release engine renamed after the game beside `game/` (its `game.json`, compiled scripts and
//! assets). Four steps, reported as JSON lines on stdout (`--json`) or text on stderr. The folder
//! is written under another name and renamed onto `<out>` only when whole.

use crate::compile::Compiler;
use crate::engine::{self, Event, Profile, Sink};
use crate::play::{GAME_DIR, GAME_FILE, GameFile};
use crate::project;
use bevy_ts::backend::ScriptError;
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

/// The files an entry defaults to, in order (the studio's list).
pub const DEFAULT_ENTRIES: [&str; 4] = ["src/game.tsx", "src/game.ts", "src/main.tsx", "src/main.ts"];

const STEPS: [&str; 4] = ["Compiling scripts", "Preparing engine", "Copying assets", "Writing game"];

/// The platforms bevy-ts ships engines for: the npm packages' targets.
pub const TARGETS: [(&str, &str); 5] = [
    ("aarch64-apple-darwin", "macOS (Apple silicon)"),
    ("x86_64-apple-darwin", "macOS (Intel)"),
    ("aarch64-unknown-linux-gnu", "Linux (ARM64)"),
    ("x86_64-unknown-linux-gnu", "Linux (x64)"),
    ("x86_64-pc-windows-msvc", "Windows (x64)"),
];

const NOT_READY: &str = "cross builds are not supported yet";

/// One row of `bevy-ts build --targets`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Target {
    pub id: &'static str,
    pub label: &'static str,
    pub host: bool,
    pub ready: bool,
    pub why: Option<&'static str>,
}

/// Every target, as the running engine sees them: only its own builds.
pub fn targets() -> Vec<Target> {
    TARGETS
        .iter()
        .map(|&(id, label)| {
            let host = id == engine::TARGET;
            Target { id, label, host, ready: host, why: (!host).then_some(NOT_READY) }
        })
        .collect()
}

/// Where a build's steps, logs and problems go.
pub struct Reporter {
    json: bool,
}

impl Reporter {
    fn emit(&self, v: serde_json::Value) {
        println!("{v}");
    }

    pub fn step(&self, index: usize) {
        let name = STEPS[index - 1];
        if self.json {
            self.emit(json!({ "event": "step", "name": name, "index": index, "total": STEPS.len() }));
        } else {
            eprintln!("bevy-ts build: [{index}/{}] {name}", STEPS.len());
        }
    }

    pub fn log(&self, line: &str) {
        if self.json {
            self.emit(json!({ "event": "log", "line": line }));
        } else {
            eprintln!("{line}");
        }
    }

    pub fn progress(&self, done: usize, total: usize) {
        if self.json {
            self.emit(json!({ "event": "progress", "done": done, "total": total }));
        }
    }

    pub fn problem(&self, e: &ScriptError) {
        let at = e.location.as_ref().map(|l| (std::path::absolute(&l.file).unwrap_or_else(|_| PathBuf::from(&l.file)), l.line, l.column));
        if self.json {
            let mut v = json!({ "event": "problem", "message": e.message });
            if let Some((file, line, column)) = &at {
                v["file"] = json!(file.display().to_string());
                v["line"] = json!(line);
                v["column"] = json!(column);
            }
            self.emit(v);
        } else {
            match at {
                Some((file, line, column)) => eprintln!("{}:{line}:{column}: {}", file.display(), e.message),
                None => eprintln!("{}", e.message),
            }
        }
    }

    pub fn done(&self, out: &Path, run: &Path) {
        if self.json {
            self.emit(json!({ "event": "done", "out": out.display().to_string(), "run": run.display().to_string() }));
        } else {
            eprintln!("bevy-ts build: wrote {}", out.display());
            println!("{}", run.display());
        }
    }

    pub fn failed(&self, message: &str) {
        if self.json {
            self.emit(json!({ "event": "failed", "message": message }));
        } else {
            eprintln!("bevy-ts build: {message}");
        }
    }
}

/// Parsed `bevy-ts build` options.
#[derive(Debug, Default)]
struct Args {
    dir: Option<String>,
    out: Option<String>,
    target: Option<String>,
    entry: Option<String>,
    json: bool,
    targets: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} takes a value"));
        match arg.as_str() {
            "--json" => a.json = true,
            "--targets" => a.targets = true,
            "--out" => a.out = Some(value("--out")?),
            "--target" => a.target = Some(value("--target")?),
            "--entry" => a.entry = Some(value("--entry")?),
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s if a.dir.is_none() => a.dir = Some(s.to_string()),
            s => return Err(format!("unexpected argument {s}")),
        }
    }
    Ok(a)
}

/// The project root: `start` or its nearest ancestor holding `bevy-ts.json`, `tsconfig.json` or
/// `package.json`, as `bevy-ts dev` finds it from the entry; else `start`.
pub fn project_root(start: &Path) -> Result<PathBuf, String> {
    let start = start.canonicalize().map_err(|e| format!("{}: {e}", start.display()))?;
    Ok(start
        .ancestors()
        .find(|d| [project::FILE, "tsconfig.json", "package.json"].iter().any(|f| d.join(f).is_file()))
        .unwrap_or(&start)
        .to_path_buf())
}

/// The game's name and entry: `bevy-ts.json`'s `name`, else the root folder's name; `--entry`
/// (relative to the working directory), else `bevy-ts.json`'s `entry` (relative to the root),
/// else the first default that exists.
pub fn name_and_entry(root: &Path, config: Option<&project::ProjectConfig>, flag: Option<&str>) -> Result<(String, PathBuf), String> {
    let name = match config.and_then(|c| c.name.clone()) {
        Some(n) => n,
        None => root.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("the project folder has no name; set `name` in bevy-ts.json")?,
    };
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(format!("{name:?} is not a file name; set `name` in bevy-ts.json"));
    }
    // The executable sits beside `game/`; on a case-insensitive disk `Game` is the same name.
    if name.eq_ignore_ascii_case(GAME_DIR) {
        return Err(format!("a game cannot be named {name:?}: its executable would collide with the {GAME_DIR}/ folder; set another `name` in bevy-ts.json"));
    }
    let entry = if let Some(e) = flag {
        std::path::absolute(e).map_err(|err| format!("{e}: {err}"))?
    } else if let Some(e) = config.and_then(|c| c.entry.as_deref()) {
        root.join(e)
    } else {
        DEFAULT_ENTRIES.iter().map(|e| root.join(e)).find(|p| p.is_file()).ok_or_else(|| {
            format!("no entry: none of {} exists in {}; pass --entry or set `entry` in bevy-ts.json", DEFAULT_ENTRIES.join(", "), root.display())
        })?
    };
    if !entry.is_file() {
        return Err(format!("the entry {} does not exist", entry.display()));
    }
    Ok((name, entry))
}

/// The folder to write: `--out` (relative to the working directory), else `<root>/dist/<target>`,
/// with its parent made and resolved through symlinks. Refuses a folder holding the project or
/// inside its assets, a file, and an existing folder that is not an earlier build (no
/// `game/game.json`), since the build replaces it.
pub fn resolve_out(root: &Path, flag: Option<&str>, target: &str) -> Result<PathBuf, String> {
    let out = match flag {
        Some(o) => std::path::absolute(o).map_err(|e| format!("{o}: {e}"))?,
        None => root.join("dist").join(target),
    };
    let (Some(parent), Some(leaf)) = (out.parent(), out.file_name()) else {
        return Err(format!("--out {}: not a folder the build can replace", out.display()));
    };
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let out = parent.canonicalize().map_err(|e| format!("{}: {e}", parent.display()))?.join(leaf);
    let shown = out.display();
    if root.starts_with(&out) || out.starts_with(root.join("assets")) {
        return Err(format!("--out {shown} would overwrite the project or its assets"));
    }
    if out.is_file() {
        return Err(format!("--out {shown} is a file"));
    }
    if out.is_dir() && !out.join(GAME_DIR).join(GAME_FILE).is_file() {
        return Err(format!("--out {shown} exists and is not an earlier build (no {GAME_DIR}/{GAME_FILE}); the build replaces it, so name a new folder"));
    }
    Ok(out)
}

/// `bevy-ts build …`; returns the exit code.
pub fn run(engine: &crate::cli::Engine, extra: impl bevy::prelude::Plugin, raw: &[String]) -> i32 {
    let json = raw.iter().any(|a| a == "--json");
    let report = Reporter { json };
    let args = match parse(raw) {
        Ok(a) => a,
        Err(e) => {
            report.failed(&e);
            if !json {
                eprintln!("usage: bevy-ts build [<project dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--json]");
            }
            return 2;
        }
    };
    if args.targets {
        list_targets(json);
        return 0;
    }
    let target = args.target.as_deref().unwrap_or(engine::TARGET);
    match targets().into_iter().find(|t| t.id == target) {
        Some(t) if t.ready => {}
        Some(t) => {
            report.failed(&format!("{target}: {}", t.why.unwrap_or(NOT_READY)));
            return 1;
        }
        None => {
            report.failed(&format!("{target}: not a target bevy-ts ships engines for; see bevy-ts build --targets"));
            return 1;
        }
    }
    // The native modules (`bevy-ts/input`, `bevy-ts/camera`, ...) register while the app builds,
    // the same app `types` walks: build it before the scripts compile, so their imports resolve.
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::types::types_app(engine, extra)));
    if let Err(p) = built {
        let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
        report.failed(&format!("a plugin failed to build without a GPU device, so the native modules are unavailable: {msg}"));
        return 1;
    }
    match build(&args, target, &report) {
        Ok(()) => 0,
        Err(e) => {
            report.failed(&e);
            1
        }
    }
}

fn list_targets(json: bool) {
    let all = targets();
    if json {
        println!("{}", json!({ "targets": all }));
        return;
    }
    for t in all {
        let note = if t.host { " (this machine)".to_string() } else { format!(": {}", t.why.unwrap_or_default()) };
        println!("{:<28} {}{note}", t.id, t.label);
    }
}

/// Removes the partial folder unless the build kept it.
struct Partial(Option<PathBuf>);

impl Drop for Partial {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

fn build(args: &Args, target: &str, report: &Reporter) -> Result<(), String> {
    let start = PathBuf::from(args.dir.as_deref().unwrap_or("."));
    let root = project_root(&start)?;
    let project = project::find(&root)?;
    let (name, entry) = name_and_entry(&root, project.as_ref().map(|p| &p.config), args.entry.as_deref())?;
    let out = resolve_out(&root, args.out.as_deref(), target)?;
    let parent = out.parent().expect("resolve_out names a folder with a parent");
    let leaf = out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let partial_dir = parent.join(format!("{leaf}.partial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&partial_dir);
    let game = partial_dir.join(GAME_DIR);
    std::fs::create_dir_all(&game).map_err(|e| format!("{}: {e}", game.display()))?;
    let mut partial = Partial(Some(partial_dir.clone()));

    // 1. Scripts, compiled for release, each at its shipped path.
    report.step(1);
    let mut compiler = Compiler::for_release(&entry).map_err(|e| format!("{}: {e}", entry.display()))?;
    // One root for the folder's layout and the source maps' paths.
    compiler.root = root.clone();
    let set = match compiler.compile_graph() {
        Ok(set) => set,
        Err(errors) => {
            for e in &errors {
                report.problem(e);
            }
            let n = errors.len();
            return Err(format!("{n} script error{}", if n == 1 { "" } else { "s" }));
        }
    };
    // Game code at its path under the root, package code under `node_modules/<name>@<version>/`:
    // the compiler's layout, which every import was rewritten for.
    let layout = compiler.layout();
    let scripts = game.join("scripts");
    for m in &set.modules {
        let dst = scripts.join(layout.place(&m.path)?);
        // A `.wgsl` import's module only names the file: ship the shader, not the build machine's path.
        if m.path.to_string_lossy().ends_with(crate::compile::SHADER_EXT) {
            write(&dst, &std::fs::read(&m.path).map_err(|e| format!("{}: {e}", m.path.display()))?)?;
        } else {
            write(&dst, m.code.as_bytes())?;
        }
    }
    let entry_rel = crate::reader::relative(&root, &set.entry);
    report.log(&format!("compiled {} modules from {}", set.modules.len(), entry_rel));

    // 2. The release engine: the project engine for a project with Rust, else this binary.
    report.step(2);
    let plan = match &project {
        Some(p) => engine::Plan::new(p, Profile::Release)?,
        None => None,
    };
    let (engine_exe, engine_id) = match plan {
        None => {
            let exe = std::env::current_exe().map_err(|e| format!("finding the running engine: {e}"))?;
            report.log(&format!("the prebuilt engine {} {}", engine::VERSION, engine::TARGET));
            (exe, "prebuilt".to_string())
        }
        Some(plan) => {
            let mut events = |e: Event| match e {
                Event::Log(l) => report.log(&l),
                Event::Progress { done, total } => report.progress(done, total),
            };
            let (exe, hash, origin) = engine::obtain(&plan, true, &mut Sink::Events(&mut events))?;
            report.log(&format!("project engine {hash} ({})", crate::supervise::origin_note(&origin)));
            (exe, hash)
        }
    };

    // 3. Assets, levels included.
    report.step(3);
    let assets = crate::headless::asset_root(&root);
    let copied = if assets.is_dir() { copy_dir(&assets, &game.join("assets"))? } else { 0 };
    report.log(&format!("copied {copied} asset files"));

    // 4. game.json and the executable, then the folder onto <out>.
    report.step(4);
    let (net, env) = project.as_ref().map(|p| (p.config.net.clone(), p.config.env.clone())).unwrap_or_default();
    let mesh_picking = project.as_ref().is_some_and(|p| p.config.mesh_picking);
    let diagnostics = project.as_ref().map(|p| p.config.diagnostics.clone()).unwrap_or_default();
    let file = GameFile { name: name.clone(), entry: entry_rel, bevy_ts: engine::VERSION.to_string(), engine: engine_id, mesh_picking, diagnostics, net, env };
    let text = serde_json::to_string_pretty(&file).expect("game.json serialises");
    write(&game.join(GAME_FILE), format!("{text}\n").as_bytes())?;
    let exe_name = if target.contains("windows") { format!("{name}.exe") } else { name };
    let exe = partial_dir.join(&exe_name);
    std::fs::copy(&engine_exe, &exe).map_err(|e| format!("{} -> {}: {e}", engine_exe.display(), exe.display()))?;
    engine::make_executable(&exe)?;
    replace(&partial_dir, &out)?;
    partial.0 = None;
    report.done(&out, &out.join(&exe_name));
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Copies `from` into `to`; returns the number of files.
fn copy_dir(from: &Path, to: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut n = 0;
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let e = e.map_err(|e| format!("{}: {e}", from.display()))?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            n += copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("{} -> {}: {e}", src.display(), dst.display()))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Renames `partial` onto `out`, moving any earlier `out` aside first and removing it after.
fn replace(partial: &Path, out: &Path) -> Result<(), String> {
    let old = out.with_file_name(format!("{}.old-{}", out.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
    let had = out.exists();
    if had {
        std::fs::rename(out, &old).map_err(|e| format!("moving the earlier {} aside: {e}", out.display()))?;
    }
    if let Err(e) = std::fs::rename(partial, out) {
        if had {
            let _ = std::fs::rename(&old, out);
        }
        return Err(format!("{} -> {}: {e}", partial.display(), out.display()));
    }
    if had {
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bevy-ts-build-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    #[test]
    fn no_entry_names_the_four_files() {
        let d = tmp("no-entry");
        let e = name_and_entry(&d, None, None).unwrap_err();
        for f in DEFAULT_ENTRIES {
            assert!(e.contains(f), "{e}");
        }
    }

    #[test]
    fn entry_and_name_resolve_in_order() {
        let d = tmp("arena");
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/main.ts"), "").unwrap();
        let (name, entry) = name_and_entry(&d, None, None).unwrap();
        assert_eq!(name, format!("bevy-ts-build-arena-{}", std::process::id()));
        assert_eq!(entry, d.join("src/main.ts"));
        std::fs::write(d.join("src/game.ts"), "").unwrap();
        assert_eq!(name_and_entry(&d, None, None).unwrap().1, d.join("src/game.ts"));
        std::fs::write(d.join("src/other.ts"), "").unwrap();
        let config = project::parse(r#"{ "name": "arena", "entry": "src/other.ts" }"#).unwrap();
        let (name, entry) = name_and_entry(&d, Some(&config), None).unwrap();
        assert_eq!((name.as_str(), entry), ("arena", d.join("src/other.ts")));
        let flag = d.join("src/main.ts");
        assert_eq!(name_and_entry(&d, Some(&config), Some(flag.to_str().unwrap())).unwrap().1, flag);
        let bad = project::parse(r#"{ "name": "../x" }"#).unwrap();
        assert!(name_and_entry(&d, Some(&bad), None).is_err());
    }

    #[test]
    fn a_game_named_game_is_refused() {
        let d = tmp("named-game");
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/game.ts"), "").unwrap();
        for n in ["game", "Game"] {
            let config = project::parse(&format!(r#"{{ "name": "{n}" }}"#)).unwrap();
            let e = name_and_entry(&d, Some(&config), None).unwrap_err();
            assert!(e.contains("collide"), "{e}");
        }
    }

    #[test]
    fn out_refuses_what_it_would_destroy() {
        let root = tmp("out");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("notes.txt"), "keep").unwrap();
        let at = |p: &Path| p.to_str().unwrap().to_string();
        // The project, through a symlinked path to it (macOS /var is /private/var).
        let linked = std::env::temp_dir().join(format!("bevy-ts-build-out-{}", std::process::id()));
        let e = resolve_out(&root, Some(&at(&linked)), "t").unwrap_err();
        assert!(e.contains("overwrite the project"), "{e}");
        assert!(resolve_out(&root, Some(&at(&root.join("assets/x"))), "t").unwrap_err().contains("assets"));
        // An existing folder that is not a build, and a file.
        assert!(resolve_out(&root, Some(&at(&root.join("src"))), "t").unwrap_err().contains("not an earlier build"));
        assert!(resolve_out(&root, Some(&at(&root.join("notes.txt"))), "t").unwrap_err().contains("is a file"));
        // A new folder, the default, and an earlier build are fine.
        assert_eq!(resolve_out(&root, None, "t").unwrap(), root.join("dist/t"));
        let earlier = root.join("dist/old");
        std::fs::create_dir_all(earlier.join("game")).unwrap();
        std::fs::write(earlier.join("game/game.json"), "{}").unwrap();
        assert_eq!(resolve_out(&root, Some(&at(&earlier)), "t").unwrap(), earlier);
        assert!(root.join("src").is_dir() && root.join("notes.txt").is_file());
    }

    #[test]
    fn only_the_host_is_ready() {
        let all = targets();
        assert_eq!(all.len(), 5);
        let ready: Vec<_> = all.iter().filter(|t| t.ready).collect();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, engine::TARGET);
        assert!(all.iter().all(|t| t.host == t.ready && t.why.is_none() == t.ready));
    }
}
