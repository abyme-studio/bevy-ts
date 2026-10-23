//! `bevy-ts dev`: compile on a background thread, watch every loaded file and every path the
//! resolver missed, and send each compiled module set to the engine as the next generation.

use crate::compile::{Compiler, SHADER_EXT};
use bevy_ts::generation::DevEvent;
use notify::{RecursiveMode, Watcher};
use std::collections::BTreeSet;
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

fn compile(compiler: &Compiler, tx: &Sender<DevEvent>) -> bool {
    let started = Instant::now();
    let _ = tx.send(DevEvent::Compiling);
    let result = compiler.compile_graph();
    tx.send(DevEvent::Compiled(result, started.elapsed())).is_ok()
}

fn watch(compiler: Arc<Compiler>, tx: Sender<DevEvent>) {
    // The first set is sent once its files are watched: an edit made as soon as the game has
    // loaded is never missed.
    let started = Instant::now();
    let _ = tx.send(DevEvent::Compiling);
    let first = compiler.compile_graph();
    let took = started.elapsed();
    let (events_tx, events) = channel();
    let mut watcher = match notify::recommended_watcher(move |e| {
        let _ = events_tx.send(e);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("bevy-ts: cannot watch files ({e}); reload with bevy-ts/reload instead");
            let _ = tx.send(DevEvent::Compiled(first, took));
            return;
        }
    };
    // Watch directories, not files: editors save by replacing the file.
    // A missing path is watched at its nearest folder that exists, so an install that creates it
    // reloads (hot-reload: "Installing a package reloads").
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    let mut files = compiler.files();
    let mut missing = compiler.missing();
    let mut rewatch = |files: &BTreeSet<PathBuf>, missing: &BTreeSet<PathBuf>, dirs: &mut BTreeSet<PathBuf>| {
        let parents = files.iter().filter_map(|f| f.parent().map(Path::to_path_buf));
        let nearest = missing.iter().filter_map(|m| nearest_folder(m, &compiler.root));
        for d in parents.chain(nearest) {
            if dirs.insert(d.clone())
                && let Err(e) = watcher.watch(&d, RecursiveMode::NonRecursive)
            {
                eprintln!("bevy-ts: cannot watch {}: {e}", d.display());
            }
        }
    };
    rewatch(&files, &missing, &mut dirs);
    if tx.send(DevEvent::Compiled(first, took)).is_err() {
        return;
    }
    loop {
        let Ok(first) = events.recv() else { return };
        let mut touched = BTreeSet::new();
        touches(&first, &files, &missing, &mut touched);
        let deadline = Instant::now() + SETTLE;
        while let Ok(e) = events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            touches(&e, &files, &missing, &mut touched);
        }
        if touched.is_empty() {
            continue;
        }
        // Shader files are not modules: a save of only those changes no JS, so it starts no
        // generation (script-materials design decision 3).
        if touched.iter().all(|p| p.to_string_lossy().ends_with(SHADER_EXT)) {
            if tx.send(DevEvent::FilesChanged(touched.into_iter().collect())).is_err() {
                return;
            }
            continue;
        }
        if !compile(&compiler, &tx) {
            return;
        }
        files = compiler.files();
        missing = compiler.missing();
        rewatch(&files, &missing, &mut dirs);
    }
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
fn touches(e: &notify::Result<notify::Event>, files: &BTreeSet<PathBuf>, missing: &BTreeSet<PathBuf>, out: &mut BTreeSet<PathBuf>) {
    let Ok(e) = e else { return };
    if e.kind.is_access() {
        return;
    }
    out.extend(e.paths.iter().filter(|p| files.contains(*p) || missing.iter().any(|m| m.starts_with(p))).cloned());
}
