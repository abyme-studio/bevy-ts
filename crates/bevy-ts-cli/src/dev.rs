//! `bevy-ts dev`: compile on a background thread, watch every loaded file, and send each
//! compiled module set to the engine as the next generation.

use crate::compile::Compiler;
use bevy_ts::generation::DevEvent;
use notify::{RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::PathBuf;
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
    if !compile(&compiler, &tx) {
        return;
    }
    let (events_tx, events) = channel();
    let mut watcher = match notify::recommended_watcher(move |e| {
        let _ = events_tx.send(e);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("bevy-ts: cannot watch files ({e}); reload with bevy-ts/reload instead");
            return;
        }
    };
    // Watch directories, not files: editors save by replacing the file.
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    let mut files = compiler.files();
    let mut rewatch = |files: &BTreeSet<PathBuf>, dirs: &mut BTreeSet<PathBuf>| {
        for f in files {
            if let Some(d) = f.parent()
                && dirs.insert(d.to_path_buf())
                && let Err(e) = watcher.watch(d, RecursiveMode::NonRecursive)
            {
                eprintln!("bevy-ts: cannot watch {}: {e}", d.display());
            }
        }
    };
    rewatch(&files, &mut dirs);
    loop {
        let Ok(first) = events.recv() else { return };
        let mut relevant = touches(&first, &files);
        let deadline = Instant::now() + SETTLE;
        while let Ok(e) = events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            relevant |= touches(&e, &files);
        }
        if !relevant {
            continue;
        }
        if !compile(&compiler, &tx) {
            return;
        }
        files = compiler.files();
        rewatch(&files, &mut dirs);
    }
}

fn touches(e: &notify::Result<notify::Event>, files: &BTreeSet<PathBuf>) -> bool {
    let Ok(e) = e else { return false };
    if e.kind.is_access() {
        return false;
    }
    e.paths.iter().any(|p| files.contains(p))
}
