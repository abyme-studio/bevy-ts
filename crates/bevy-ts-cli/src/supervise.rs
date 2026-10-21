//! `bevy-ts dev` in a project with Rust (native decision 3): the prebuilt binary becomes a
//! supervisor. It runs the project engine as a child with the same arguments, watches the files
//! that enter the engine hash, rebuilds in the background on a change, and on success replaces
//! the child (same entry, flags and port; the TS generation loads as a restart). On failure the
//! old engine keeps running. TS edits never reach here: the child reloads them itself.

use crate::cli::DevArgs;
use crate::engine::{self, Origin, Plan, Profile};
use crate::project::{self, Project};
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// How long to wait for more file events before hashing, so one save rebuilds once.
const SETTLE: Duration = Duration::from_millis(200);

fn find_project(start: &Path) -> Result<Option<Project>, String> {
    project::find(start)
}

fn plan_for(start: &Path, profile: Profile) -> Result<Option<Plan>, String> {
    match find_project(start)? {
        Some(p) => Plan::new(&p, profile),
        None => Ok(None),
    }
}

fn spawn(exe: &Path, raw: &[String]) -> Result<Child, String> {
    Command::new(exe).arg("dev").args(raw).spawn().map_err(|e| format!("starting {}: {e}", exe.display()))
}

pub(crate) fn origin_note(o: &Origin) -> &'static str {
    match o {
        Origin::LocalCache => "cached",
        Origin::SharedCache => "from the shared cache",
        Origin::Built => "built",
    }
}

/// Runs `bevy-ts dev` through a project engine when the entry's project has Rust. `None` when it
/// does not: the caller runs the prebuilt engine in-process. Otherwise the exit code.
pub fn dev_in_project(args: &DevArgs, raw: &[String]) -> Option<i32> {
    let entry = PathBuf::from(&args.entry);
    let plan = match plan_for(&entry, Profile::Dev) {
        Ok(Some(p)) => p,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("bevy-ts dev: {e}");
            return Some(1);
        }
    };
    let (exe, mut hash, origin) = match engine::obtain(&plan, true, &mut engine::Sink::Stderr) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("bevy-ts dev: {e}");
            return Some(1);
        }
    };
    eprintln!("bevy-ts: running project engine {hash} ({})", origin_note(&origin));
    regenerate_types(&plan, &exe);
    let mut child = match spawn(&exe, raw) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bevy-ts dev: {e}");
            return Some(1);
        }
    };
    let changes = watch(&plan);
    let (built_tx, built) = channel::<Result<(PathBuf, String, Origin), String>>();
    let mut building = false;
    let mut again = false;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status.code().unwrap_or(1));
        }
        // A settled batch of changes to hashed inputs.
        if let Some(rx) = &changes
            && settled(rx)
        {
            if building {
                again = true;
            } else {
                building = start_build(&entry, &hash, &built_tx);
            }
        }
        match built.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok((exe, new_hash, origin))) => {
                building = false;
                eprintln!("bevy-ts: project engine {new_hash} ({}); restarting on it", origin_note(&origin));
                if let Ok(Some(plan)) = plan_for(&entry, Profile::Dev) {
                    regenerate_types(&plan, &exe);
                }
                let _ = child.kill();
                let _ = child.wait();
                hash = new_hash;
                child = match spawn(&exe, raw) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("bevy-ts dev: {e}");
                        return Some(1);
                    }
                };
            }
            Ok(Err(e)) => {
                building = false;
                eprintln!("bevy-ts: {e}; the running engine {hash} keeps going");
            }
            Err(_) => {}
        }
        if !building && again {
            again = false;
            building = start_build(&entry, &hash, &built_tx);
        }
    }
}

/// Starts a background build when the hash moved. Returns whether one started.
fn start_build(entry: &Path, current: &str, tx: &std::sync::mpsc::Sender<Result<(PathBuf, String, Origin), String>>) -> bool {
    let plan = match plan_for(entry, Profile::Dev) {
        Ok(Some(p)) => p,
        Ok(None) => return false,
        Err(e) => {
            let _ = tx.send(Err(e));
            return true;
        }
    };
    match plan.hash() {
        Ok(h) if h == current => return false,
        Ok(_) => {}
        Err(e) => {
            let _ = tx.send(Err(e));
            return true;
        }
    }
    let tx = tx.clone();
    std::thread::Builder::new()
        .name("bevy-ts-engine-build".into())
        .spawn(move || {
            let r = engine::obtain(&plan, true, &mut engine::Sink::Stderr);
            let _ = tx.send(r);
        })
        .expect("spawn the engine build thread");
    true
}

/// Drains file events; true when some arrived and no more came for [`SETTLE`].
fn settled(rx: &Receiver<()>) -> bool {
    if rx.try_recv().is_err() {
        return false;
    }
    let mut deadline = Instant::now() + SETTLE;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(()) => deadline = Instant::now() + SETTLE,
            Err(_) => return true,
        }
    }
}

/// Watches every file and directory that enters the hash. Events are only a hint: the hash
/// decides whether anything is rebuilt.
fn watch(plan: &Plan) -> Option<Receiver<()>> {
    let (tx, rx) = channel();
    let inputs = plan.inputs();
    let mut watcher = match notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
        if e.is_ok_and(|e| !e.kind.is_access()) {
            let _ = tx.send(());
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("bevy-ts: cannot watch the native crate ({e}); restart bevy-ts dev after Rust edits");
            return None;
        }
    };
    let mut dirs: Vec<PathBuf> = inputs.dirs.into_iter().collect();
    dirs.extend(inputs.files.iter().filter_map(|f| f.parent().map(Path::to_path_buf)));
    dirs.sort();
    dirs.dedup();
    for d in dirs {
        let _ = watcher.watch(&d, RecursiveMode::NonRecursive);
    }
    // The watcher lives as long as the process.
    std::mem::forget(watcher);
    Some(rx)
}

/// Writes `.bevy-ts/types/native.d.ts` from a project engine's registry.
fn regenerate_types(plan: &Plan, exe: &Path) {
    let out = plan.root.join(".bevy-ts/types/native.d.ts");
    match Command::new(exe).arg("types").arg("--out").arg(&out).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => eprintln!("bevy-ts: generating {} failed: {}", out.display(), String::from_utf8_lossy(&o.stderr).trim()),
        Err(e) => eprintln!("bevy-ts: generating {}: {e}", out.display()),
    }
}

/// `bevy-ts types` in a project with Rust: runs the project engine's `types`. `None` when the
/// project has none.
pub fn types_in_project(raw: &[String]) -> Option<i32> {
    let plan = match plan_for(Path::new("."), Profile::Dev) {
        Ok(Some(p)) => p,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("bevy-ts types: {e}");
            return Some(1);
        }
    };
    let (exe, _, _) = match engine::obtain(&plan, true, &mut engine::Sink::Stderr) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("bevy-ts types: {e}");
            return Some(1);
        }
    };
    let mut args = raw.to_vec();
    if !args.iter().any(|a| a == "--out") {
        args.push("--out".into());
        args.push(plan.root.join(".bevy-ts/types/native.d.ts").display().to_string());
    }
    Some(Command::new(exe).arg("types").args(&args).status().ok().and_then(|s| s.code()).unwrap_or(1))
}
