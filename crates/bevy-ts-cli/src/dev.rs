//! `bevy-ts dev`: compile on a background thread, watch every loaded file and every path the
//! resolver missed, and send each compiled module set to the engine as the next generation.

use crate::compile::{Compiler, SHADER_EXT};
use bevy_ts_levels::engine::content_hash;
use bevy_ts::generation::DevEvent;
use notify::{RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

/// How long to wait for more file events before compiling, so one save compiles once.
const SETTLE: Duration = Duration::from_millis(15);

/// Starts the watcher thread: compiles once now, then again after any change to a file of the
/// graph. Returns at once; the thread lives as long as the process.
pub fn spawn_watcher(compiler: Arc<Compiler>, tx: Sender<DevEvent>) {
    std::thread::Builder::new()
        .name("bevy-ts-dev".into())
        .spawn(move || watch(compiler, tx))
        .expect("spawn the bevy-ts-dev thread");
}

fn watch(compiler: Arc<Compiler>, tx: Sender<DevEvent>) {
    let started = Instant::now();
    let _ = tx.send(DevEvent::Compiling);
    let result = compiler.compile_graph();
    let took = started.elapsed();
    let (events_tx, events) = channel();
    let mut watcher = match notify::recommended_watcher(move |e| {
        let _ = events_tx.send(e);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("bevy-ts: cannot watch files ({e}); reload with bevy-ts/reload instead");
            let _ = tx.send(DevEvent::Compiled(result, took));
            return;
        }
    };
    // Watch directories, not files: editors save by replacing the file.
    // A missing path is watched at its nearest folder that exists, so an install that creates it
    // reloads (hot-reload: "Installing a package reloads").
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    // True when a change may have come before its folder was watched: a file in a folder newly
    // watched no longer has the text compiled, or a missing path moved. Then it compiles again.
    let mut rewatch = |seen: &Seen, dirs: &mut BTreeSet<PathBuf>| {
        let parents = seen.files.keys().filter_map(|f| f.parent().map(Path::to_path_buf));
        let nearest = seen.missing.values().filter_map(|(_, n)| n.clone());
        let mut new = BTreeSet::new();
        for d in parents.chain(nearest) {
            if dirs.insert(d.clone()) {
                if let Err(e) = watcher.watch(&d, RecursiveMode::NonRecursive) {
                    eprintln!("bevy-ts: cannot watch {}: {e}", d.display());
                }
                new.insert(d);
            }
        }
        let files = seen.files.keys().filter(|f| f.parent().is_some_and(|d| new.contains(d)));
        files.chain(seen.missing.keys()).any(|p| seen.changed(p))
    };
    // Each set is sent once its files are watched: an edit made as soon as its report arrives is
    // never missed (hot-reload: "A reload report means its files are watched").
    let mut seen = Seen::of(&compiler);
    let mut again = rewatch(&seen, &mut dirs);
    if tx.send(DevEvent::Compiled(result, took)).is_err() {
        return;
    }
    loop {
        if !again {
            let Ok(first) = events.recv() else { return };
            let mut touched = BTreeSet::new();
            touches(&first, &seen, &mut touched);
            let deadline = Instant::now() + SETTLE;
            while let Ok(e) = events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                touches(&e, &seen, &mut touched);
            }
            // A late report of a write from before the watch, or of one already compiled,
            // changes nothing: macOS reports writes made just before a watch starts.
            touched.retain(|p| seen.changed(p));
            if touched.is_empty() {
                continue;
            }
            // Shader files are not modules: a save of only those changes no JS, so it starts no
            // generation (script-materials design decision 3).
            if touched.iter().all(|p| p.to_string_lossy().ends_with(SHADER_EXT)) {
                for p in &touched {
                    seen.files.insert(p.clone(), hash_of(p));
                }
                if tx.send(DevEvent::FilesChanged(touched.into_iter().collect())).is_err() {
                    return;
                }
                continue;
            }
        }
        let started = Instant::now();
        if tx.send(DevEvent::Compiling).is_err() {
            return;
        }
        let result = compiler.compile_graph();
        let took = started.elapsed();
        seen = Seen::of(&compiler);
        again = rewatch(&seen, &mut dirs);
        if tx.send(DevEvent::Compiled(result, took)).is_err() {
            return;
        }
    }
}

/// What the last compile saw: each file's content hash (`None` when unreadable), and for each
/// missing path, whether something is there (the resolver may have looked for a file where a
/// folder is) and its nearest watched folder.
struct Seen {
    files: BTreeMap<PathBuf, Option<u64>>,
    missing: BTreeMap<PathBuf, (bool, Option<PathBuf>)>,
    root: PathBuf,
}

impl Seen {
    fn of(compiler: &Compiler) -> Seen {
        let hashes = compiler.hashes();
        let files = compiler.files().into_iter().map(|f| {
            let h = hashes.get(&f).copied().or_else(|| hash_of(&f));
            (f, h)
        });
        let missing = compiler.missing().into_iter().map(|m| {
            let n = (m.exists(), nearest_folder(&m, &compiler.root));
            (m, n)
        });
        Seen { files: files.collect(), missing: missing.collect(), root: compiler.root.clone() }
    }

    /// Whether `path` differs from what the compile saw: a file's bytes, or a missing path that
    /// appeared or whose nearest folder moved.
    fn changed(&self, path: &Path) -> bool {
        if let Some(h) = self.files.get(path) {
            return hash_of(path) != *h;
        }
        let moved = |(m, seen): (&PathBuf, &(bool, Option<PathBuf>))| (m.exists(), nearest_folder(m, &self.root)) != *seen;
        self.missing.iter().filter(|(m, _)| m.starts_with(path)).any(moved)
    }
}

fn hash_of(path: &Path) -> Option<u64> {
    std::fs::read(path).ok().map(|b| content_hash(&b))
}

/// The nearest folder of `missing` that exists, unless it is above the project root: an install
/// in the project never creates a folder there, and watching `/` or the home folder would be
/// all noise.
fn nearest_folder(missing: &Path, root: &Path) -> Option<PathBuf> {
    let dir = missing.ancestors().skip(1).find(|d| d.is_dir())?;
    (!(root.starts_with(dir) && root != dir)).then(|| dir.to_path_buf())
}

/// Adds the graph's files, and the missing paths, an event touched to `out`: a missing path is
/// touched when it or a folder above it appears.
fn touches(e: &notify::Result<notify::Event>, seen: &Seen, out: &mut BTreeSet<PathBuf>) {
    let Ok(e) = e else { return };
    if e.kind.is_access() {
        return;
    }
    out.extend(e.paths.iter().filter(|p| seen.files.contains_key(*p) || seen.missing.keys().any(|m| m.starts_with(p))).cloned());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bevy-ts-dev-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (f, src) in [("package.json", "{\"name\": \"game\", \"type\": \"module\"}")].iter().chain(files) {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, src).unwrap();
        }
        dir.canonicalize().unwrap()
    }

    #[test]
    fn a_late_report_of_the_compiled_text_is_no_change() {
        let dir = project("late", &[("game.ts", "export const a = 1;\n")]);
        let compiler = Compiler::new(dir.join("game.ts"), true).unwrap();
        compiler.compile_graph().unwrap();
        let seen = Seen::of(&compiler);
        let game = dir.join("game.ts");
        assert!(!seen.changed(&game), "the text it compiled");
        std::fs::write(&game, "export const a = 1;\n").unwrap();
        assert!(!seen.changed(&game), "saved again unchanged");
        std::fs::write(&game, "export const a = 2;\n").unwrap();
        assert!(seen.changed(&game), "new text of the same length");
        assert!(!seen.changed(&dir.join("other.ts")), "not of the graph");
    }

    #[test]
    fn an_install_is_a_change_of_its_missing_path() {
        let dir = project("install", &[("game.ts", "import { pad } from \"lodash-es\";\nconsole.log(pad);\n")]);
        let compiler = Compiler::new(dir.join("game.ts"), true).unwrap();
        compiler.compile_graph().unwrap_err();
        let seen = Seen::of(&compiler);
        let modules = dir.join("node_modules");
        // The resolver missed `node_modules`, watched at the project folder.
        assert!(!seen.changed(&modules), "nothing installed yet");
        std::fs::create_dir_all(modules.join("lodash-es")).unwrap();
        assert!(seen.changed(&modules), "the install's folder appeared");
    }
}
